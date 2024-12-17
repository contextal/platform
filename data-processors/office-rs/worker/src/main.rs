mod config;
mod error;
#[cfg(test)]
mod tests;

use backend_utils::objects::{
    BackendRequest, BackendResultChild, BackendResultKind, BackendResultOk, Metadata,
};
use ctxole::{
    crypto::OleCrypto,
    oleps::{self, DocumentSummaryInformation, SummaryInformation},
    Encryption, NoValidPasswordError, Ole,
};
use ctxutils::io::LimitedWriter;
use doc::{Doc, DocPart, WordChar};
use error::OfficeError;
use ooxml::{
    relationship::TargetMode, DocumentSecurity as DocSec, Ooxml, ProcessingSummary,
    RelationshipType,
};
use scopeguard::ScopeGuard;
use serde::Serialize;
use std::{
    collections::HashMap,
    fmt::{self, Debug},
    fs::{self, File},
    io::{self, BufReader, Read, Seek, Write},
    path::PathBuf,
};
use tempfile::{tempfile, NamedTempFile, TempPath};
use time::{Duration, OffsetDateTime};
use tracing::{debug, instrument, warn};
use tracing_subscriber::prelude::*;
use url::Url;
use vba::{
    forms::{Control, ParentControl},
    ModuleGeneric, ModuleTrait, ProjectTrait, Vba, VbaDocument,
};
use xls::{
    macrosheet::MacroSheet,
    worksheet::{self, Worksheet},
    Xls,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = config::Config::new()?;
    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        process_request(request, &config)
    })?;
    unreachable!()
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, Box<dyn std::error::Error>> {
    let passwords = [
        "contextal",
        "Password1234_",
        "openwall",
        "hashcat",
        "1234567890123456",
        "123456789012345",
        "myhovercraftisfullofeels",
        "myhovercraftisf",
        "VelvetSweatshop",
    ];
    //TODO: Add passwords from request

    let input_path = PathBuf::from(&config.objects_path).join(&request.object.object_id);
    let input_path = input_path.to_str().ok_or("Invalid path")?;

    let mut symbols = Vec::<String>::new();

    let mut files_for_cleanup = scopeguard::guard(vec![], |files| {
        files.into_iter().for_each(|file| {
            let _ = fs::remove_file(file);
        })
    });

    let processing_result = match process_file(input_path, &passwords, config, &mut symbols) {
        Ok(result) => result,
        Err(e) => {
            if e.is_data_error() {
                return Ok(BackendResultKind::error(format!(
                    "Invalid Office file: {e}"
                )));
            } else if e.is_no_valid_password_error() {
                let mut relation_metadata = Metadata::new();
                if let Some(algorithm) = e.get_no_valid_password_error_algorithm() {
                    relation_metadata.insert(
                        "algorithm".to_string(),
                        serde_json::Value::String(algorithm),
                    );
                }
                let children = vec![BackendResultChild {
                    path: None,
                    force_type: None,
                    symbols: vec!["ENCRYPTED".to_string()],
                    relation_metadata,
                }];
                return Ok(BackendResultKind::ok(BackendResultOk {
                    symbols: vec!["ENCRYPTED".to_string()],
                    object_metadata: Metadata::new(),
                    children,
                }));
            } else {
                return Err(e.into());
            }
        }
    };

    if processing_result.metadata.encryption.is_some() {
        symbols.push("ENCRYPTED".to_string());
        symbols.push("DECRYPTED".to_string());
    }

    let mut children = Vec::<BackendResultChild>::new();
    for mut child in processing_result.children {
        let path = match child.file {
            Some(f) => Some(
                f.keep()
                    .map_err(|e| format!("failed to preserve a temporary file: {e}"))?
                    .into_os_string()
                    .into_string()
                    .map_err(|s| format!("failed to convert OsString {s:?} to String"))?,
            ),
            None => None,
        };

        if let Some(path) = &path {
            files_for_cleanup.push(path.clone());
        }

        if processing_result.metadata.encryption.is_some() {
            child.symbols.push("ENCRYPTED".to_string());
            child.symbols.push("DECRYPTED".to_string());
        }

        children.push(BackendResultChild {
            path,
            force_type: child.enforced_type,
            symbols: child.symbols,
            relation_metadata: child.relation_metadata,
        });
    }

    let mut unique_hosts = Vec::<String>::new();
    let mut unique_domains = Vec::<String>::new();
    for input in &processing_result.metadata.hyperlinks {
        if let Ok(url) = Url::parse(input) {
            if let Some(host) = url.host_str() {
                unique_hosts.push(host.to_string());
                if let Ok(domain) = addr::parse_domain_name(host) {
                    if let Some(root) = domain.root() {
                        unique_domains.push(root.to_string());
                    }
                }
            }
        }
    }
    unique_hosts.sort_unstable();
    unique_hosts.dedup();
    unique_domains.sort_unstable();
    unique_domains.dedup();
    let mut limits_reached = processing_result.limits_reached;
    if config.create_domain_children {
        for domain in unique_domains.iter() {
            if children.len() >= config.max_children as usize {
                limits_reached = true;
                break;
            }
            let mut domain_file = tempfile::NamedTempFile::new_in(&config.output_path)?;
            if domain_file.write_all(&domain.clone().into_bytes()).is_ok() {
                children.push(BackendResultChild {
                    path: Some(
                        domain_file
                            .into_temp_path()
                            .keep()
                            .unwrap()
                            .into_os_string()
                            .into_string()
                            .unwrap(),
                    ),
                    force_type: Some("Domain".to_string()),
                    symbols: vec![],
                    relation_metadata: match serde_json::to_value(DomainMetadata {
                        name: domain.clone(),
                    })? {
                        serde_json::Value::Object(v) => v,
                        _ => unreachable!(),
                    },
                });
            }
        }
    }

    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let mut object_metadata = match serde_json::to_value(processing_result.metadata)
        .map_err(|e| format!("failed to serialize Metadata: {e}"))?
    {
        serde_json::Value::Object(v) => v,
        _ => unreachable!(),
    };
    if !unique_hosts.is_empty() {
        object_metadata.insert(
            "unique_hosts".to_string(),
            serde_json::to_value(unique_hosts)?,
        );
    }
    if !unique_domains.is_empty() {
        object_metadata.insert(
            "unique_domains".to_string(),
            serde_json::to_value(unique_domains)?,
        );
    }

    let result = BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata,
        children,
    });

    ScopeGuard::into_inner(files_for_cleanup); // disarm garbage collection

    Ok(result)
}

#[derive(Debug)]
struct Child {
    file: Option<TempPath>,
    enforced_type: Option<String>,
    symbols: Vec<String>,
    relation_metadata: Metadata,
}
#[derive(Debug)]
struct ProcessingResult {
    children: Vec<Child>,
    limits_reached: bool,
    metadata: OfficeMetadata,
}

#[derive(Debug, Serialize)]
struct OfficeMetadata {
    properties: Properties,
    user_properties: HashMap<String, UserDefinedProperty>,
    encryption: Option<Encryption>,
    vba: Option<Metadata>,
    #[serde(skip_serializing_if = "Option::is_none", flatten)]
    excel_only: Option<ExcelOnlyMetadata>,
    #[serde(skip_serializing_if = "Option::is_none", flatten)]
    old_doc_only: Option<LegacyWordOnlyMetadata>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    external_resources: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hyperlinks: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ExcelOnlyMetadata {
    num_sheets_detected: u32,
    num_sheets_processed: u32,
    num_cells_detected: u64,
    num_cells_processed: u64,
    sheets: Vec<SheetInfo>,
}

#[derive(Debug, Serialize)]
struct LegacyWordOnlyMetadata {
    dop: Option<doc::dop::Dop>,
    associations: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
enum UserDefinedProperty {
    String(String),
    Int(i32),
    Real(f64),
    Bool(bool),
    DateTime(OffsetDateTimeWrapper),
    Undecoded,
}

fn process_vba<R: Read + Seek>(
    vba: &Vba<'_, R>,
    pc: bool,
    output_path: &str,
) -> Result<Child, io::Error> {
    let mut res = Child {
        file: None,
        enforced_type: Some("Text".to_string()),
        symbols: vec!["VBA".to_string()],
        relation_metadata: Metadata::new(),
    };
    if pc {
        res.symbols.push("DECOMPILED".to_string());
    }
    res.relation_metadata.insert(
        "vba_version".to_string(),
        vba.vba_project.vba_version.into(),
    );
    let project = if pc {
        vba.project_pc().map(|p| Box::new(p as &dyn ProjectTrait))
    } else {
        vba.project().map(|p| Box::new(p as &dyn ProjectTrait))
    };
    if project.is_err() {
        res.symbols.push("CORRUPTED".to_string());
        return Ok(res);
    }
    let project = project.unwrap();
    res.relation_metadata
        .insert("name".to_string(), project.name().into());
    res.relation_metadata
        .insert("sys_kind".to_string(), project.sys_kind().into());
    res.relation_metadata
        .insert("lcid".to_string(), project.lcid().into());
    res.relation_metadata
        .insert("lcid_for_invoke".to_string(), project.lcid_invoke().into());
    res.relation_metadata
        .insert("code_page".to_string(), project.codepage().into());
    res.relation_metadata
        .insert("description".to_string(), project.docstring().into());
    res.relation_metadata
        .insert("help_file".to_string(), project.help().into());
    res.relation_metadata
        .insert("help_context".to_string(), project.help_context().into());
    res.relation_metadata.insert(
        "libflags".to_string(),
        project
            .lib_flags()
            .map(|v| {
                let mut flags: Vec<&str> = Vec::new();
                if (v & 1) != 0 {
                    flags.push("RESTRICTED");
                }
                if (v & 2) != 0 {
                    flags.push("CONTROL");
                }
                if (v & 4) != 0 {
                    flags.push("HIDDEN");
                }
                if (v & 8) != 0 {
                    flags.push("HASDISK");
                }
                if (v & !0xf) != 0 {
                    flags.push("INVALID");
                }
            })
            .into(),
    );
    res.relation_metadata
        .insert("major_version".to_string(), project.version_major().into());
    res.relation_metadata
        .insert("minor_version".to_string(), project.version_minor().into());
    res.relation_metadata
        .insert("cookie".to_string(), project.cookie().into());
    let modules: Result<Vec<ModuleGeneric>, io::Error> = if pc {
        vba.modules_pc()
            .map(|mods| mods.map(|m| m.as_gen()).collect())
    } else {
        vba.modules().map(|mods| mods.map(|m| m.as_gen()).collect())
    };
    if modules.is_err() {
        res.symbols.push("CORRUPTED".to_string());
        return Ok(res);
    }
    let modules = modules.unwrap();
    let mut tempfile = NamedTempFile::new_in(output_path)?;
    let mut mods_meta: Vec<Metadata> = Vec::with_capacity(modules.len());
    let mut all_good = true;
    for module in modules {
        let mut mod_meta = Metadata::new();
        mod_meta.insert("names".to_string(), module.names().into());
        mod_meta.insert("stream_names".to_string(), module.stream_names().into());
        mod_meta.insert("descriptions".to_string(), module.docstrings().into());
        mod_meta.insert("stream_offset".to_string(), module.offset().into());
        mod_meta.insert("help_context".to_string(), module.help_context().into());
        mod_meta.insert("cookie".to_string(), module.cookie().into());
        mod_meta.insert("procedural".to_string(), module.is_procedural().into());
        mod_meta.insert(
            "Non-Procedural".to_string(),
            module.is_non_procedural().into(),
        );
        mod_meta.insert("read_only".to_string(), module.is_read_only().into());
        mod_meta.insert("private".to_string(), module.is_private().into());
        let mut code_ok = true;
        writeln!(
            &mut tempfile,
            "' === {} ===",
            module.names().first().unwrap_or(&"<NONAME>")
        )?;
        if pc {
            // Decompile
            if let Ok(decompiler) = vba.get_decompiler(&module) {
                let (lines, nc_lines) = decompiler.num_lines();
                let mut line_meta = Metadata::new();
                line_meta.insert("total".to_string(), lines.into());
                line_meta.insert("non_contiguous".to_string(), nc_lines.into());
                let fragmentation = f64::from(nc_lines) / f64::from(lines);
                if !fragmentation.is_nan() {
                    line_meta.insert("fragmentation".to_string(), fragmentation.into());
                }
                mod_meta.insert("lines".to_string(), line_meta.into());
                for line in decompiler.iter() {
                    match line {
                        Ok(l) => writeln!(&mut tempfile, "{}", l)?,
                        Err(e) => {
                            code_ok = false;
                            writeln!(&mut tempfile, "' <Decompiler error: {}>", e)?
                        }
                    }
                }
            } else {
                writeln!(
                    &mut tempfile,
                    "' <Decompiler error: failed to get a decompiler>"
                )?;
                code_ok = false;
            }
        } else {
            // Source code
            if let Ok(mut stream) = vba.get_code_stream(&module) {
                match std::io::copy(&mut stream, &mut tempfile) {
                    Err(e) if e.kind() == io::ErrorKind::InvalidData => code_ok = false,
                    Err(e) => return Err(e),
                    Ok(_) => {}
                }
            } else {
                code_ok = false;
            }
        }
        mod_meta.insert("code_ok".to_string(), code_ok.into());
        all_good &= code_ok;
        mods_meta.push(mod_meta);
    }
    if !all_good {
        res.symbols.push("CORRUPTED".to_string());
    }
    res.relation_metadata
        .insert("modules".to_string(), mods_meta.into());
    res.file = Some(tempfile.into_temp_path());
    Ok(res)
}

fn add_parent_meta<'a, R: 'a + Read + Seek, P: ParentControl<'a, R>>(
    p: &'a P,
    meta: &mut Metadata,
) {
    fn add_size_pos(meta: &mut Metadata, k: &str, v: (i32, i32)) {
        let (x, y) = v;
        meta.insert(k.to_string(), vec![x, y].into());
    }
    meta.insert("name".to_string(), p.get_name().into());
    meta.insert("version".to_string(), p.get_version().into());
    meta.insert("back_color".to_string(), p.get_back_color().into());
    meta.insert("fore_color".to_string(), p.get_fore_color().into());
    meta.insert(
        "next_available_id".to_string(),
        p.get_next_available_id().into(),
    );
    meta.insert("border_style".to_string(), p.get_border_style().into());
    meta.insert("mouse_pointer".to_string(), p.get_mouse_pointer().into());
    meta.insert("scroll_bars".to_string(), p.get_scroll_bars().into());
    meta.insert("group_count".to_string(), p.get_group_count().into());
    meta.insert("cycle".to_string(), p.get_cycle().into());
    meta.insert("special_effect".to_string(), p.get_special_effect().into());
    meta.insert("border_color".to_string(), p.get_border_color().into());
    meta.insert("zoom".to_string(), p.get_zoom().into());
    meta.insert(
        "picture_alignment".to_string(),
        p.get_picture_alignment().into(),
    );
    meta.insert("picture_tiling".to_string(), p.get_picture_tiling().into());
    meta.insert(
        "picture_size_mode".to_string(),
        p.get_picture_size_mode().into(),
    );
    meta.insert("shape_cookie".to_string(), p.get_shape_cookie().into());
    meta.insert("draw_buffer".to_string(), p.get_draw_buffer().into());
    add_size_pos(meta, "displayed_size", p.get_displayed_size());
    add_size_pos(meta, "local_size", p.get_logical_size());
    add_size_pos(meta, "scroll_position", p.get_scroll_position());
    meta.insert("caption".to_string(), p.get_caption().into());
    meta.insert(
        "has_custom_mouse_icon".to_string(),
        p.get_mouse_icon().is_some().into(),
    );
    meta.insert(
        "has_custom_picture".to_string(),
        p.get_picture().is_some().into(),
    );
    meta.insert(
        "font".to_string(),
        p.get_font().map(|f| f.name.clone()).into(),
    );
    meta.insert("anomalies".to_string(), p.get_anomalies().into());
    let mut children: Vec<Metadata> = Vec::new();
    for control in p.children().flatten() {
        let mut c_meta = Metadata::new();
        match control {
            Control::Frame(p) => {
                c_meta.insert("type".to_string(), "Frame".into());
                c_meta.insert("cname".to_string(), p.ci.name.as_str().into());
                add_parent_meta(&p, &mut c_meta);
            }
            Control::MultiPage(p) => {
                c_meta.insert("type".to_string(), "Multipage".into());
                c_meta.insert("cname".to_string(), p.frame.ci.name.as_str().into());
                add_parent_meta(&p, &mut c_meta);
            }
            Control::Page(p) => {
                c_meta.insert("type".to_string(), "Page".into());
                c_meta.insert("cname".to_string(), p.frame.ci.name.as_str().into());
                add_parent_meta(&p, &mut c_meta);
            }
            Control::CommandButton(c) => {
                c_meta.insert("type".to_string(), "CommandButton".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("caption".to_string(), c.caption.into());
            }
            Control::SpinButton(c) => {
                c_meta.insert("type".to_string(), "SpinButton".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("min".to_string(), c.min_value.into());
                c_meta.insert("max".to_string(), c.max_value.into());
                c_meta.insert("value".to_string(), c.value.into());
            }
            Control::Image(c) => {
                c_meta.insert("type".to_string(), "Image".into());
                c_meta.insert("name".to_string(), c.control.name.into());
            }
            Control::Label(c) => {
                c_meta.insert("type".to_string(), "Label".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("caption".to_string(), c.caption.into());
            }
            Control::CheckBox(c) => {
                c_meta.insert("type".to_string(), "CheckBox".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("caption".to_string(), c.caption.into());
                c_meta.insert("value".to_string(), c.value.into());
            }
            Control::ComboBox(c) => {
                c_meta.insert("type".to_string(), "ComboBox".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("value".to_string(), c.value.into());
            }
            Control::ListBox(c) => {
                c_meta.insert("type".to_string(), "ListBox".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("value".to_string(), c.value.into());
            }
            Control::OptionButton(c) => {
                c_meta.insert("type".to_string(), "OptionButton".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("value".to_string(), c.value.into());
            }
            Control::TextBox(c) => {
                c_meta.insert("type".to_string(), "TextBox".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("value".to_string(), c.value.into());
            }
            Control::ToggleButton(c) => {
                c_meta.insert("type".to_string(), "ToggleButton".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                c_meta.insert("value".to_string(), c.value.into());
            }
            Control::ScrollBar(c) => {
                c_meta.insert("type".to_string(), "ScrollBar".into());
                c_meta.insert("name".to_string(), c.control.name.into());
            }
            Control::TabStrip(c) => {
                c_meta.insert("type".to_string(), "TabStrip".into());
                c_meta.insert("name".to_string(), c.control.name.into());
                c_meta.insert("enabled".to_string(), c.enabled.into());
                let tabs: Vec<String> = c.tabs.into_iter().map(|t| t.caption).collect();
                c_meta.insert("tabs".to_string(), tabs.into());
            }
            Control::UnknownType(_) => {}
        }
        children.push(c_meta);
    }
    meta.insert("children".to_string(), children.into());
}

fn process_vba_document<'a, R: Read + Seek + 'a, V: VbaDocument<'a, R>>(
    v: V,
    output_path: &str,
    object_symbols: &mut Vec<String>,
    children: &mut Vec<Child>,
) -> Result<Option<Metadata>, io::Error> {
    if let Some(vba) = v.vba() {
        if let Ok(vba) = vba {
            object_symbols.push("VBA".to_string());
            children.push(process_vba(&vba, false, output_path)?);
            children.push(process_vba(&vba, true, output_path)?);
            let mut vba_meta = Metadata::new();
            vba_meta.insert("version".to_string(), vba.vba_project.vba_version.into());
            vba_meta.insert("rsvd2".to_string(), vba.vba_project.rsvd2.into());
            vba_meta.insert("rsvd3".to_string(), vba.vba_project.rsvd3.into());
            let mut forms: Vec<Metadata> = Vec::new();
            for (name, form) in vba.forms() {
                let mut formdata = Metadata::new();
                formdata.insert("name".to_string(), name.into());
                if let Ok(form) = form {
                    add_parent_meta(&form, &mut formdata);
                    formdata.insert("stream".to_string(), name.into());
                    formdata.insert("module".to_string(), form.di.module_name.into());
                    formdata.insert("caption".to_string(), form.di.caption.into());
                    formdata.insert("left".to_string(), form.di.left.into());
                    formdata.insert("top".to_string(), form.di.top.into());
                    formdata.insert("width".to_string(), form.di.width.into());
                    formdata.insert("height".to_string(), form.di.height.into());
                    formdata.insert("enabled".to_string(), form.di.enabled.into());
                    formdata.insert(
                        "help_context_id".to_string(),
                        form.di.help_context_id.into(),
                    );
                    formdata.insert("rtl".to_string(), form.di.rtl.into());
                    formdata.insert("modal".to_string(), form.di.modal.into());
                    formdata.insert("start_position".to_string(), form.di.start_position.into());
                    formdata.insert("tag".to_string(), form.di.tag.into());
                    formdata.insert("type_info_ver".to_string(), form.di.type_info_ver.into());
                    formdata.insert("visible".to_string(), form.di.visible.into());
                    formdata.insert("help_btn".to_string(), form.di.help_btn.into());
                    formdata.insert("help_topic".to_string(), form.di.help_topic.into());
                    if let Some(serde_json::Value::Array(anom)) = formdata.get_mut("anomalies") {
                        for a in form.di.anomalies {
                            anom.push(a.into());
                        }
                    } else {
                        formdata.insert("anomalies".to_string(), form.di.anomalies.into());
                    }
                }
                forms.push(formdata);
            }
            if !forms.is_empty() {
                object_symbols.push("HAS_FORMS".to_string());
                vba_meta.insert("forms".to_string(), forms.into());
            }
            return Ok(Some(vba_meta));
        } else {
            object_symbols.push("CORRUPTED_VBA".to_string());
        }
    }
    Ok(None)
}

fn process_doc<R: Read + Seek>(
    ole: Ole<R>,
    passwords: &[&str],
    config: &config::Config,
    object_symbols: &mut Vec<String>,
) -> Result<ProcessingResult, OfficeError> {
    let pair = get_ole_properties(&ole);
    let properties = pair.0;
    let user_properties = pair.1;
    let mut doc = Doc::new(ole, passwords)?;
    let encryption = doc.encryption().cloned();
    let mut hyperlinks = Vec::<String>::new();
    let mut child_symbols = Vec::<String>::new();
    let relation_metadata = Metadata::new();
    let mut enforced_type = None;

    let old_doc_only = Some(LegacyWordOnlyMetadata {
        dop: doc.get_dop(),
        associations: doc.get_associations().map(|assocs| assocs.strings.to_vec()),
    });
    let mut writer = LimitedWriter::new(
        NamedTempFile::new_in(&config.output_path)?,
        config.max_processed_size,
    );
    let mut limits_reached = false;
    for c in doc.char_iter(DocPart::MainDocument)? {
        match c {
            WordChar::Char(c) => {
                if let Err(error) = writer.write_fmt(format_args!("{c}")) {
                    let error: OfficeError = error.into();
                    if error.is_write_limit_error() {
                        limits_reached = true;
                        break;
                    } else {
                        return Err(error);
                    }
                }
            }
            WordChar::Hyperlink {
                text,
                uri,
                extra_data: _,
            } => {
                if !uri.is_empty() {
                    hyperlinks.push(uri);
                }
                if let Err(error) = writer.write_fmt(format_args!("{text}")) {
                    let error: OfficeError = error.into();
                    if error.is_write_limit_error() {
                        limits_reached = true;
                        break;
                    } else {
                        return Err(error);
                    }
                }
            }
            _ => {}
        }
    }
    let file = if limits_reached {
        child_symbols.push("TOOBIG".to_string());
        None
    } else {
        writer.flush()?;
        enforced_type = Some("Text".to_string());
        Some(writer.into_inner().into_temp_path())
    };
    let mut children = Vec::<Child>::new();
    children.push(Child {
        enforced_type,
        file,
        symbols: child_symbols,
        relation_metadata,
    });
    let vba = process_vba_document(&doc, &config.output_path, object_symbols, &mut children)?;
    //TODO: Extract embedded objects
    object_symbols.push("DOC".to_string());

    Ok(ProcessingResult {
        children,
        limits_reached,
        metadata: OfficeMetadata {
            properties,
            user_properties,
            hyperlinks,
            encryption,
            vba,
            excel_only: None,
            old_doc_only,
            external_resources: Vec::new(),
        },
    })
}

enum Sheet<'a, R: Read + Seek> {
    Worksheet(Worksheet<'a, R>),
    Macrosheet(MacroSheet<'a, R>),
}

#[derive(Debug, Serialize)]
struct SheetInfo {
    name: String,
    #[serde(rename = "type")]
    sheet_type: String,
    visibility: String,
    num_cells_detected: u64,
    num_cells_processed: u64,
    output_size: u64,
    limit_reached: bool,
}

fn process_sheet<R: Read + Seek, W: Write>(
    sheet: Sheet<'_, R>,
    writer: W,
    limit: u64,
) -> Result<SheetInfo, OfficeError> {
    let mut prcessing_result = worksheet::ProcessingResult::default();
    let mut tempfile = LimitedWriter::new(writer, limit);
    let name;
    let sheet_type;
    let visibility;
    let result = match sheet {
        Sheet::Worksheet(worksheet) => {
            name = worksheet.worksheet_info.name.to_string();
            visibility = worksheet.worksheet_info.state.to_string();
            sheet_type = worksheet.worksheet_info.sheet_type.name().to_string();
            worksheet.process(&mut tempfile, &mut prcessing_result)
        }
        Sheet::Macrosheet(macrosheet) => {
            name = macrosheet.worksheet_info.name.to_string();
            visibility = macrosheet.worksheet_info.state.to_string();
            sheet_type = macrosheet.worksheet_info.sheet_type.name().to_string();
            macrosheet.process(&mut tempfile, &mut prcessing_result)
        }
    };
    let limit_reached = if let Err(error) = result {
        let error: OfficeError = error.into();
        if error.is_write_limit_error() {
            true
        } else {
            return Err(error);
        }
    } else {
        false
    };
    let processed_size = tempfile.written();

    Ok(SheetInfo {
        name,
        sheet_type,
        visibility,
        num_cells_detected: prcessing_result.num_cells_detected,
        num_cells_processed: prcessing_result.num_cells_processed,
        output_size: processed_size,
        limit_reached,
    })
}

fn process_xls<R: Read + Seek>(
    ole: Ole<R>,
    passwords: &[&str],
    config: &config::Config,
    object_symbols: &mut Vec<String>,
) -> Result<ProcessingResult, OfficeError> {
    let pair = get_ole_properties(&ole);
    let properties = pair.0;
    let user_properties = pair.1;
    let xls = Xls::new(&ole, passwords)?;
    let encryption = xls.encryption().cloned();
    let workbook = &xls.workbook;
    let mut remaining_processed_size = config.max_processed_size;
    let mut limits_reached = false;
    let mut has_macrosheets = false;
    let mut tempfile = NamedTempFile::new_in(&config.output_path)?;

    let mut num_sheets_processed = 0;
    let mut num_cells_processed = 0;
    let mut num_cells_detected = 0;

    let mut sheets = Vec::<SheetInfo>::new();

    for sheet in workbook.macrosheets() {
        let sheet_info = process_sheet(
            Sheet::Macrosheet(sheet),
            &mut tempfile,
            config.sheet_size_limit.min(remaining_processed_size),
        )?;
        remaining_processed_size = remaining_processed_size.saturating_sub(sheet_info.output_size);
        if sheet_info.limit_reached {
            limits_reached = true;
        }
        num_sheets_processed += 1;
        num_cells_detected += sheet_info.num_cells_detected;
        num_cells_processed += sheet_info.num_cells_processed;
        sheets.push(sheet_info);
    }
    for sheet in workbook.worksheets() {
        let sheet_info = process_sheet(
            Sheet::Worksheet(sheet),
            &mut tempfile,
            config.sheet_size_limit.min(remaining_processed_size),
        )?;
        remaining_processed_size = remaining_processed_size.saturating_sub(sheet_info.output_size);
        if sheet_info.limit_reached {
            limits_reached = true;
        }
        num_sheets_processed += 1;
        num_cells_detected += sheet_info.num_cells_detected;
        num_cells_processed += sheet_info.num_cells_processed;
        sheets.push(sheet_info);
    }
    let mut num_sheets_detected = num_sheets_processed;
    for sheet in workbook.additional_sheets() {
        has_macrosheets = true;
        num_sheets_detected += 1;
        let sheet_info = SheetInfo {
            name: sheet.name.to_string(),
            sheet_type: sheet.sheet_type.name().to_string(),
            visibility: sheet.state.to_string(),
            num_cells_detected: 0,
            num_cells_processed: 0,
            output_size: 0,
            limit_reached: false,
        };
        sheets.push(sheet_info);
    }
    let relation_metadata = Metadata::new();
    let enforced_type = Some("Text".to_string());
    let mut child_symbols = Vec::<String>::new();
    if limits_reached {
        child_symbols.push("TOOBIG".to_string());
    }
    if has_macrosheets {
        object_symbols.push("HAS_MACRO_SHEET".to_string());
    }

    let file = Some(tempfile.into_temp_path());
    let mut children = Vec::<Child>::new();
    children.push(Child {
        enforced_type,
        file,
        symbols: child_symbols,
        relation_metadata,
    });

    let vba = process_vba_document(&xls, &config.output_path, object_symbols, &mut children)?;
    //TODO: Extract embedded objects
    object_symbols.push("XLS".to_string());
    Ok(ProcessingResult {
        children,
        limits_reached,
        metadata: OfficeMetadata {
            properties,
            user_properties,
            hyperlinks: Vec::new(),
            encryption,
            vba,
            excel_only: Some(ExcelOnlyMetadata {
                num_sheets_detected,
                num_sheets_processed,
                num_cells_detected,
                num_cells_processed,
                sheets,
            }),
            old_doc_only: None,
            external_resources: Vec::new(),
        },
    })
}

fn process_file(
    path: &str,
    passwords: &[&str],
    config: &config::Config,
    object_symbols: &mut Vec<String>,
) -> Result<ProcessingResult, OfficeError> {
    let mut file = File::open(path)?;
    let output_path = &config.output_path;
    let mut remaining_processed_size = config.max_processed_size;
    let mut encryption: Option<Encryption> = None;
    let mut limits_reached = false;
    let mut children = Vec::<Child>::new();
    let mut hyperlinks = Vec::<String>::new();
    let mut external_resources = Vec::<String>::new();
    let mut sheets = Vec::<SheetInfo>::new();
    let mut vba = None;
    let mut excel_only: Option<ExcelOnlyMetadata> = None;
    if let Ok(ole) = Ole::new(BufReader::new(&mut file)) {
        debug!("OLE file detected");
        object_symbols.push("OLE".to_string());
        if ole.get_entry_by_name("EncryptionInfo").is_ok()
            && ole.get_entry_by_name("EncryptedPackage").is_ok()
        {
            debug!("Found 'EncryptionInfo' and 'EncryptedPackage' streams");
            let crypto = OleCrypto::new(&ole)?;
            let algorithm = match &crypto.encryption_info.encryption_type {
                ctxole::crypto::EncryptionType::Standard(enc) => [
                    "StandardEncryption",
                    enc.header.algorithm.to_string().as_str(),
                ]
                .join(" "),
                ctxole::crypto::EncryptionType::Agile(enc) => [
                    "AgileEncryption",
                    enc.key_data.cipher_algorithm.as_str(),
                    enc.key_data.hash_algorithm.as_str(),
                ]
                .join(" "),
            };
            let mut key = None;
            for password in passwords {
                if let Some(k) = crypto.get_key(password) {
                    key = Some(k);
                    encryption = Some(Encryption {
                        algorithm: algorithm.to_string(),
                        password: password.to_string(),
                    });
                    break;
                }
            }
            let key = key.ok_or(NoValidPasswordError::new_io_error(algorithm))?;
            let mut decrypted_file = tempfile()?;
            debug!("Decrypting");
            crypto
                .decrypt(&key, &ole, &mut decrypted_file)
                .map_err(|e| {
                    warn!("Decryption error: {e}");
                    e
                })?;
            decrypted_file.rewind()?;
            if let Ok(ole) = Ole::new(BufReader::new(&decrypted_file)) {
                if ole.get_entry_by_name("WordDocument").is_ok() {
                    return process_doc(ole, passwords, config, object_symbols);
                } else if ole.get_entry_by_name("Workbook").is_ok() {
                    return process_xls(ole, passwords, config, object_symbols);
                }
            }
            decrypted_file.rewind()?;
            file = decrypted_file;
        } else if ole.get_entry_by_name("WordDocument").is_ok() {
            debug!("Found 'WordDocument' stream");
            return process_doc(ole, passwords, config, object_symbols);
        } else if ole.get_entry_by_name("Workbook").is_ok() {
            debug!("Found 'Workbook' stream");
            return process_xls(ole, passwords, config, object_symbols);
        } else {
            return Err("Unknown ole format".into());
        }
    };

    if let Ok(mut ooxml) = Ooxml::new(file, config.shared_strings_cache_limit) {
        let files_to_process;
        match &mut ooxml.document {
            ooxml::Document::Docx(docx) => {
                object_symbols.push("DOCX".to_string());
                let mut child_symbols = Vec::<String>::new();
                let mut relation_metadata = Metadata::new();
                let mut enforced_type = None;
                relation_metadata.insert(
                    "path".to_string(),
                    serde_json::Value::String(docx.path().to_string()),
                );
                let limit = std::cmp::min(remaining_processed_size, config.max_child_output_size);
                let mut writer = LimitedWriter::new(NamedTempFile::new_in(output_path)?, limit);
                let mut processing_summary = ProcessingSummary::default();
                let r = docx.process(&mut writer, &mut processing_summary);
                remaining_processed_size =
                    remaining_processed_size.saturating_sub(writer.written());
                let limit_reached = if let Err(error) = r {
                    let error: OfficeError = error.into();
                    if error.is_write_limit_error() {
                        true
                    } else {
                        return Err(error);
                    }
                } else {
                    false
                };
                let file = if limit_reached {
                    limits_reached = true;
                    child_symbols.push("TOOBIG".to_string());
                    None
                } else {
                    writer.flush()?;
                    enforced_type = Some("Text".to_string());
                    Some(writer.into_inner().into_temp_path())
                };
                children.push(Child {
                    enforced_type,
                    file,
                    symbols: child_symbols,
                    relation_metadata,
                });
                hyperlinks = processing_summary.hyperlinks;
                files_to_process = processing_summary.files_to_process;
                external_resources = processing_summary.external_resources;
                for relationship in docx.relationships() {
                    if let TargetMode::External(target) = &relationship.target {
                        if !hyperlinks.contains(target) && !external_resources.contains(target) {
                            external_resources.push(target.clone());
                        }
                    }
                }
            }
            ooxml::Document::Xlsx(xlsx) => {
                object_symbols.push("XLSX".to_string());
                let mut processing_summary = ProcessingSummary::default();
                let mut has_macrosheet = false;

                let mut symbols = Vec::<String>::new();
                let relation_metadata = Metadata::new();
                let mut tempfile = NamedTempFile::new_in(output_path)?;
                let mut remaining_tempfile_size = config.max_child_output_size;

                for relationship in xlsx.relationships() {
                    if let TargetMode::External(target) = &relationship.target {
                        if !hyperlinks.contains(target) && !external_resources.contains(target) {
                            external_resources.push(target.clone());
                        }
                    }
                }

                for mut sheet in xlsx
                    .iter()
                    .filter(|s| s.info().sheet_type == ooxml::SheetType::Macrosheet)
                {
                    for relationship in sheet.relationships() {
                        if let TargetMode::External(target) = &relationship.target {
                            if !hyperlinks.contains(target) && !external_resources.contains(target)
                            {
                                external_resources.push(target.clone());
                            }
                        }
                    }
                    has_macrosheet = true;
                    if process_ooxml_sheet(
                        &mut sheet,
                        &mut tempfile,
                        &mut remaining_tempfile_size,
                        &mut remaining_processed_size,
                        config,
                        &mut sheets,
                        &mut limits_reached,
                        &mut processing_summary,
                    )? {
                        limits_reached = true;
                    }
                }

                for mut sheet in xlsx
                    .iter()
                    .filter(|s| s.info().sheet_type == ooxml::SheetType::Worksheet)
                {
                    for relationship in sheet.relationships() {
                        if let TargetMode::External(target) = &relationship.target {
                            external_resources.push(target.clone());
                        }
                    }
                    if process_ooxml_sheet(
                        &mut sheet,
                        &mut tempfile,
                        &mut remaining_tempfile_size,
                        &mut remaining_processed_size,
                        config,
                        &mut sheets,
                        &mut limits_reached,
                        &mut processing_summary,
                    )? {
                        limits_reached = true;
                    }
                }

                for sheet in xlsx.iter().filter(|s| {
                    s.info().sheet_type != ooxml::SheetType::Macrosheet
                        && s.info().sheet_type != ooxml::SheetType::Worksheet
                }) {
                    let sheet_info = SheetInfo {
                        name: sheet.info().name.to_string(),
                        sheet_type: sheet.info().sheet_type.name().to_string(),
                        visibility: sheet.info().state.to_string(),
                        num_cells_detected: 0,
                        num_cells_processed: 0,
                        output_size: 0,
                        limit_reached: false,
                    };
                    sheets.push(sheet_info);
                }
                excel_only = Some(ExcelOnlyMetadata {
                    num_sheets_detected: processing_summary.num_sheets_detected,
                    num_sheets_processed: processing_summary.num_sheets_processed,
                    num_cells_detected: processing_summary.num_cells_detected,
                    num_cells_processed: processing_summary.num_cells_processed,
                    sheets,
                });
                hyperlinks = processing_summary.hyperlinks;
                files_to_process = processing_summary.files_to_process;
                for target in &processing_summary.external_resources {
                    if !hyperlinks.contains(target) && !external_resources.contains(target) {
                        external_resources.push(target.clone());
                    }
                }

                if limits_reached {
                    symbols.push("LIMITS_REACHED".to_string());
                }

                let enforced_type = Some("Text".to_string());
                let file = Some(tempfile.into_temp_path());
                children.push(Child {
                    enforced_type,
                    file,
                    symbols,
                    relation_metadata,
                });

                if has_macrosheet {
                    object_symbols.push("HAS_MACRO_SHEET".to_string());
                }
            }
            ooxml::Document::Xlsb(xlsb) => {
                object_symbols.push("XLSB".to_string());
                let mut processing_summary = ProcessingSummary::default();
                let mut has_macrosheet = false;

                let mut symbols = Vec::<String>::new();
                let relation_metadata = Metadata::new();
                let mut tempfile = NamedTempFile::new_in(output_path)?;
                let mut remaining_tempfile_size = config.max_child_output_size;

                for relationship in xlsb.relationships() {
                    if let TargetMode::External(target) = &relationship.target {
                        if !hyperlinks.contains(target) && !external_resources.contains(target) {
                            external_resources.push(target.clone());
                        }
                    }
                }

                for mut sheet in xlsb
                    .iter()
                    .filter(|s| s.info().sheet_type == ooxml::SheetType::Macrosheet)
                {
                    for relationship in sheet.relationships() {
                        if let TargetMode::External(target) = &relationship.target {
                            if !hyperlinks.contains(target) && !external_resources.contains(target)
                            {
                                external_resources.push(target.clone());
                            }
                        }
                    }
                    has_macrosheet = true;
                    if process_ooxml_binary_sheet(
                        &mut sheet,
                        &mut tempfile,
                        &mut remaining_tempfile_size,
                        &mut remaining_processed_size,
                        config,
                        &mut sheets,
                        &mut limits_reached,
                        &mut processing_summary,
                    )? {
                        limits_reached = true;
                    }
                }

                for mut sheet in xlsb
                    .iter()
                    .filter(|s| s.info().sheet_type == ooxml::SheetType::Worksheet)
                {
                    for relationship in sheet.relationships() {
                        if let TargetMode::External(target) = &relationship.target {
                            external_resources.push(target.clone());
                        }
                    }
                    if process_ooxml_binary_sheet(
                        &mut sheet,
                        &mut tempfile,
                        &mut remaining_tempfile_size,
                        &mut remaining_processed_size,
                        config,
                        &mut sheets,
                        &mut limits_reached,
                        &mut processing_summary,
                    )? {
                        limits_reached = true;
                    }
                }

                for sheet in xlsb.iter().filter(|s| {
                    s.info().sheet_type != ooxml::SheetType::Macrosheet
                        && s.info().sheet_type != ooxml::SheetType::Worksheet
                }) {
                    let sheet_info = SheetInfo {
                        name: sheet.info().name.to_string(),
                        sheet_type: sheet.info().sheet_type.name().to_string(),
                        visibility: sheet.info().state.to_string(),
                        num_cells_detected: 0,
                        num_cells_processed: 0,
                        output_size: 0,
                        limit_reached: false,
                    };
                    sheets.push(sheet_info);
                }
                excel_only = Some(ExcelOnlyMetadata {
                    num_sheets_detected: processing_summary.num_sheets_detected,
                    num_sheets_processed: processing_summary.num_sheets_processed,
                    num_cells_detected: processing_summary.num_cells_detected,
                    num_cells_processed: processing_summary.num_cells_processed,
                    sheets,
                });
                hyperlinks = processing_summary.hyperlinks;
                files_to_process = processing_summary.files_to_process;
                for target in &processing_summary.external_resources {
                    if !hyperlinks.contains(target) && !external_resources.contains(target) {
                        external_resources.push(target.clone());
                    }
                }

                if limits_reached {
                    symbols.push("LIMITS_REACHED".to_string());
                }

                let enforced_type = Some("Text".to_string());
                let file = Some(tempfile.into_temp_path());
                children.push(Child {
                    enforced_type,
                    file,
                    symbols,
                    relation_metadata,
                });

                if has_macrosheet {
                    object_symbols.push("HAS_MACRO_SHEET".to_string());
                }
            }
        };
        let pair = get_ooxml_properties(&ooxml);
        let properties = pair.0;
        let user_properties = pair.1;

        if let Some(vba_entry) = ooxml.get_vba_entry() {
            if let Ok(vba_entry) = vba_entry {
                vba = process_vba_document(
                    &vba_entry,
                    &config.output_path,
                    object_symbols,
                    &mut children,
                )?;
            } else {
                object_symbols.push("CORRUPTED_VBA".to_string());
            }
        }

        for file in files_to_process {
            if children.len() >= usize::try_from(config.max_children)? {
                limits_reached = true;
                break;
            }
            let mut symbols = Vec::<String>::new();
            let mut relation_metadata = Metadata::new();
            relation_metadata.insert(
                "path".to_string(),
                serde_json::Value::String(file.path.clone()),
            );

            let limit = std::cmp::min(remaining_processed_size, config.max_child_output_size);
            let mut writer = LimitedWriter::new(NamedTempFile::new_in(output_path)?, limit);
            let r = ooxml.extract_file_to_writer(&file.path, &mut writer);
            remaining_processed_size = remaining_processed_size.saturating_sub(writer.written());

            if matches!(r, Ok(false)) {
                // File not found in archive
                symbols.push("NOT_FOUND".to_string());
                children.push(Child {
                    enforced_type: None,
                    file: None,
                    symbols,
                    relation_metadata,
                });
                continue;
            }
            relation_metadata.insert(
                "relationship_type".to_string(),
                serde_json::Value::String(file.rel_type.name().to_string()),
            );
            if file.rel_type == RelationshipType::Image {
                relation_metadata.insert("request_ocr".to_string(), serde_json::Value::Bool(true));
            }

            let limit_reached = if let Err(error) = r {
                let error: OfficeError = error.into();
                if error.is_write_limit_error() {
                    true
                } else {
                    return Err(error);
                }
            } else {
                false
            };

            let file = if limit_reached {
                limits_reached = true;
                symbols.push("TOOBIG".to_string());
                None
            } else {
                writer.flush()?;
                Some(writer.into_inner().into_temp_path())
            };

            children.push(Child {
                enforced_type: None,
                file,
                symbols,
                relation_metadata,
            });
        }

        return Ok(ProcessingResult {
            children,
            limits_reached,
            metadata: OfficeMetadata {
                properties,
                user_properties,
                hyperlinks,
                encryption,
                vba,
                excel_only,
                old_doc_only: None,
                external_resources,
            },
        });
    }

    Err("Unable to recognize file format".into())
}

/// Resturns Ok(true) if sheet processing was interuped due to size limits
#[allow(clippy::too_many_arguments)]
fn process_ooxml_sheet<R: Read + Seek>(
    sheet: &mut ooxml::Sheet<R>,
    tempfile: &mut NamedTempFile,
    remaining_tempfile_size: &mut u64,
    remaining_processed_size: &mut u64,
    config: &config::Config,
    sheets: &mut Vec<SheetInfo>,
    limits_reached: &mut bool,
    processing_summary: &mut ProcessingSummary,
) -> Result<bool, OfficeError> {
    if *remaining_tempfile_size == 0 {
        let sheet_info = SheetInfo {
            name: sheet.info().name.to_string(),
            sheet_type: sheet.info().sheet_type.name().to_string(),
            visibility: sheet.info().state.to_string(),
            num_cells_detected: 0,
            num_cells_processed: 0,
            output_size: 0,
            limit_reached: false,
        };
        sheets.push(sheet_info);
        return Ok(true);
    }
    processing_summary.num_sheets_detected += 1;
    let limit = (*remaining_tempfile_size).min(config.sheet_size_limit);

    let last_cells_detected = processing_summary.num_cells_detected;
    let last_cells_processed = processing_summary.num_cells_processed;

    let mut limited_writer = LimitedWriter::new(tempfile, limit);
    let limit_reached = if let Err(error) = sheet.process(&mut limited_writer, processing_summary) {
        let error: OfficeError = error.into();
        if error.is_write_limit_error() {
            *limits_reached = true;
            true
        } else {
            return Err(error);
        }
    } else {
        false
    };

    limited_writer.flush()?;
    *remaining_tempfile_size = remaining_tempfile_size.saturating_sub(limited_writer.written());
    *remaining_processed_size = remaining_processed_size.saturating_sub(limited_writer.written());

    processing_summary.num_sheets_processed += 1;

    let sheet_info = SheetInfo {
        name: sheet.info().name.to_string(),
        sheet_type: sheet.info().sheet_type.name().to_string(),
        visibility: sheet.info().state.to_string(),
        num_cells_detected: processing_summary
            .num_cells_detected
            .saturating_sub(last_cells_detected),
        num_cells_processed: processing_summary
            .num_cells_processed
            .saturating_sub(last_cells_processed),
        output_size: limited_writer.written(),
        limit_reached,
    };
    sheets.push(sheet_info);

    Ok(limit_reached)
}

/// Resturns Ok(true) if sheet processing was interuped due to size limits
#[allow(clippy::too_many_arguments)]
fn process_ooxml_binary_sheet<R: Read + Seek>(
    sheet: &mut ooxml::BinarySheet<R>,
    tempfile: &mut NamedTempFile,
    remaining_tempfile_size: &mut u64,
    remaining_processed_size: &mut u64,
    config: &config::Config,
    sheets: &mut Vec<SheetInfo>,
    limits_reached: &mut bool,
    processing_summary: &mut ProcessingSummary,
) -> Result<bool, OfficeError> {
    if *remaining_tempfile_size == 0 {
        let sheet_info = SheetInfo {
            name: sheet.info().name.to_string(),
            sheet_type: sheet.info().sheet_type.name().to_string(),
            visibility: sheet.info().state.to_string(),
            num_cells_detected: 0,
            num_cells_processed: 0,
            output_size: 0,
            limit_reached: false,
        };
        sheets.push(sheet_info);
        return Ok(true);
    }
    processing_summary.num_sheets_detected += 1;
    let limit = (*remaining_tempfile_size).min(config.sheet_size_limit);

    let last_cells_detected = processing_summary.num_cells_detected;
    let last_cells_processed = processing_summary.num_cells_processed;

    let mut limited_writer = LimitedWriter::new(tempfile, limit);
    let limit_reached = if let Err(error) = sheet.process(&mut limited_writer, processing_summary) {
        let error: OfficeError = error.into();
        if error.is_write_limit_error() {
            *limits_reached = true;
            true
        } else {
            return Err(error);
        }
    } else {
        false
    };

    limited_writer.flush()?;
    *remaining_tempfile_size = remaining_tempfile_size.saturating_sub(limited_writer.written());
    *remaining_processed_size = remaining_processed_size.saturating_sub(limited_writer.written());

    processing_summary.num_sheets_processed += 1;

    let sheet_info = SheetInfo {
        name: sheet.info().name.to_string(),
        sheet_type: sheet.info().sheet_type.name().to_string(),
        visibility: sheet.info().state.to_string(),
        num_cells_detected: processing_summary
            .num_cells_detected
            .saturating_sub(last_cells_detected),
        num_cells_processed: processing_summary
            .num_cells_processed
            .saturating_sub(last_cells_processed),
        output_size: limited_writer.written(),
        limit_reached,
    };
    sheets.push(sheet_info);

    Ok(limit_reached)
}

#[derive(Debug, Default, Serialize)]
struct DocumentSecurity {
    password_protected: bool,
    read_only_recommended: bool,
    read_only_enforced: bool,
    locked: bool,
}

#[derive(Debug, Default, Serialize)]
struct Properties {
    pub application: Option<String>,
    pub app_version: Option<String>,
    pub category: Option<String>,
    pub characters: Option<i32>,
    pub characters_with_spaces: Option<i32>,
    pub company: Option<String>,
    pub content_status: Option<String>,
    pub created: Option<OffsetDateTimeWrapper>,
    pub creator: Option<String>,
    pub description: Option<String>,
    pub doc_security: DocumentSecurity,
    pub hidden_slides: Option<i32>,
    pub hyperlink_base: Option<String>,
    pub hyperlinks_changed: Option<bool>,
    pub identifier: Option<String>,
    pub keywords: Option<String>,
    pub language: Option<String>,
    pub last_modified_by: Option<String>,
    pub last_printed: Option<OffsetDateTimeWrapper>,
    pub lines: Option<i32>,
    pub links_dirty: Option<bool>,
    pub links_up_to_date: Option<bool>,
    pub manager: Option<String>,
    pub mm_clips: Option<i32>,
    pub modified: Option<OffsetDateTimeWrapper>,
    pub notes: Option<i32>,
    pub pages: Option<i32>,
    pub paragraphs: Option<i32>,
    pub presentation_format: Option<String>,
    pub revision: Option<String>,
    pub scale_crop: Option<bool>,
    pub shared_doc: Option<bool>,
    pub slides: Option<i32>,
    pub subject: Option<String>,
    pub template: Option<String>,
    pub title: Option<String>,
    pub total_time: Option<DurationWrapper>,
    pub version: Option<String>,
    pub words: Option<i32>,
}

pub struct DurationWrapper(Duration);

impl fmt::Debug for DurationWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for DurationWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

pub struct OffsetDateTimeWrapper(OffsetDateTime);

impl fmt::Debug for OffsetDateTimeWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for OffsetDateTimeWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[derive(Serialize)]
struct DomainMetadata {
    name: String,
}

fn get_ole_properties<R: Read + Seek>(
    ole: &Ole<R>,
) -> (Properties, HashMap<String, UserDefinedProperty>) {
    let mut properties = Properties::default();
    let mut custom_properties = HashMap::<String, UserDefinedProperty>::new();
    if let Ok(entry) = ole.get_entry_by_name("\u{5}SummaryInformation") {
        let mut stream = ole.get_stream_reader(&entry);
        if let Ok(si) = SummaryInformation::new(&mut stream) {
            properties.title = si.title.map(|s| s.trim_end_matches('\0').to_string());
            properties.subject = si.subject.map(|s| s.trim_end_matches('\0').to_string());
            properties.creator = si.author.map(|s| s.trim_end_matches('\0').to_string());
            properties.keywords = si.keywords.map(|s| s.trim_end_matches('\0').to_string());
            properties.description = si.comments.map(|s| s.trim_end_matches('\0').to_string());
            properties.template = si.template.map(|s| s.trim_end_matches('\0').to_string());
            properties.last_modified_by =
                si.last_author.map(|s| s.trim_end_matches('\0').to_string());
            properties.revision = si.revision;
            properties.total_time = si.edit_time.map(DurationWrapper);
            properties.last_printed = si.last_printed_dt.map(OffsetDateTimeWrapper);
            properties.created = si.created_dt.map(OffsetDateTimeWrapper);
            properties.modified = si.last_saved_dt.map(OffsetDateTimeWrapper);
            properties.pages = si.pages;
            properties.words = si.words;
            properties.characters = si.chars;
            properties.application = si
                .application_name
                .map(|s| s.trim_end_matches('\0').to_string());
            properties.doc_security.locked = si.locked;
            properties.doc_security.password_protected = si.password_protected;
            properties.doc_security.read_only_enforced = si.readonly_enforced;
            properties.doc_security.read_only_recommended = si.readonly_recommend;
        }
    }
    if let Ok(entry) = ole.get_entry_by_name("\u{5}DocumentSummaryInformation") {
        let mut stream = ole.get_stream_reader(&entry);
        if let Ok(dsi) = DocumentSummaryInformation::new(&mut stream) {
            properties.category = dsi.category.map(|s| s.trim_end_matches('\0').to_string());
            properties.presentation_format = dsi
                .presentation_format
                .map(|s| s.trim_end_matches('\0').to_string());
            properties.lines = dsi.lines;
            properties.paragraphs = dsi.paragraphs;
            properties.slides = dsi.slides;
            properties.notes = dsi.notes;
            properties.hidden_slides = dsi.hidden_slides;
            properties.mm_clips = dsi.mmclips;
            properties.scale_crop = dsi.scale;
            properties.manager = dsi.manager.map(|s| s.trim_end_matches('\0').to_string());
            properties.company = dsi.company.map(|s| s.trim_end_matches('\0').to_string());
            properties.links_dirty = dsi.links_dirty;
            properties.characters = dsi.characters;
            properties.hyperlinks_changed = dsi.hyperlinks_changed;
            properties.app_version = dsi.version.map(|v| format!("{}.{}", v.major, v.minor));
            properties.content_status = dsi
                .content_status
                .map(|s| s.trim_end_matches('\0').to_string());
            properties.language = dsi.language.map(|s| s.trim_end_matches('\0').to_string());
            properties.version = dsi.docversion;
            properties.hyperlink_base = match dsi.user_defined_properties.get("_PID_LINKBASE") {
                Some(oleps::UserDefinedProperty::String(s)) => {
                    Some(s.trim_end_matches('\0').to_string())
                }
                _ => None,
            };

            for pair in dsi.user_defined_properties {
                let key = pair.0.clone();
                if key == "_PID_LINKBASE" {
                    continue;
                }
                let property = match pair.1 {
                    oleps::UserDefinedProperty::String(v) => UserDefinedProperty::String(v.clone()),
                    oleps::UserDefinedProperty::Int(v) => UserDefinedProperty::Int(v),
                    oleps::UserDefinedProperty::Real(v) => UserDefinedProperty::Real(v),
                    oleps::UserDefinedProperty::Bool(v) => UserDefinedProperty::Bool(v),
                    oleps::UserDefinedProperty::DateTime(v) => {
                        UserDefinedProperty::DateTime(OffsetDateTimeWrapper(v))
                    }
                    oleps::UserDefinedProperty::Undecoded => UserDefinedProperty::Undecoded,
                };
                custom_properties.insert(key, property);
            }
        }
    }
    (properties, custom_properties)
}

fn get_ooxml_properties<R: Read + Seek>(
    ooxml: &Ooxml<R>,
) -> (Properties, HashMap<String, UserDefinedProperty>) {
    let mut properties = Properties::default();
    let mut custom_properties = HashMap::<String, UserDefinedProperty>::new();

    let core = ooxml.properties.core_properties.clone();
    properties.category = core.category;
    properties.content_status = core.content_status;
    properties.created = core.created.map(OffsetDateTimeWrapper);
    properties.creator = core.creator;
    properties.description = core.description;
    properties.identifier = core.identifier;
    properties.keywords = core.keywords;
    properties.language = core.language;
    properties.last_modified_by = core.last_modified_by;
    properties.last_printed = core.last_printed.map(OffsetDateTimeWrapper);
    properties.modified = core.modified.map(OffsetDateTimeWrapper);
    properties.revision = core.revision;
    properties.subject = core.subject;
    properties.title = core.title;
    properties.version = core.version;

    let extended = ooxml.properties.extended_properties.clone();
    properties.application = extended.application;
    properties.app_version = extended.app_version;
    properties.characters = extended.characters;
    properties.characters_with_spaces = extended.characters_with_spaces;
    properties.company = extended.company;
    properties.doc_security.locked = extended
        .doc_security
        .as_ref()
        .map(|v| v.contains(DocSec::ANNOTATION_LOCKED))
        .unwrap_or_default();
    properties.doc_security.password_protected = extended
        .doc_security
        .as_ref()
        .map(|v| v.contains(DocSec::PASSWORD_PROTECTED))
        .unwrap_or_default();
    properties.doc_security.read_only_enforced = extended
        .doc_security
        .as_ref()
        .map(|v| v.contains(DocSec::READ_ONLY_ENFORCED))
        .unwrap_or_default();
    properties.doc_security.read_only_recommended = extended
        .doc_security
        .as_ref()
        .map(|v| v.contains(DocSec::READ_ONLY_RECOMMENDED))
        .unwrap_or_default();
    properties.hidden_slides = extended.hidden_slides;
    properties.hyperlink_base = extended.hyperlink_base;
    properties.hyperlinks_changed = extended.hyperlinks_changed;
    properties.lines = extended.lines;
    properties.links_up_to_date = extended.links_up_to_date;
    properties.manager = extended.manager;
    properties.mm_clips = extended.mm_clips;
    properties.notes = extended.notes;
    properties.pages = extended.pages;
    properties.paragraphs = extended.paragraphs;
    properties.presentation_format = extended.presentation_format;
    properties.scale_crop = extended.scale_crop;
    properties.shared_doc = extended.shared_doc;
    properties.slides = extended.slides;
    properties.template = extended.template;
    properties.total_time = extended
        .total_time
        .map(|minutes| DurationWrapper(Duration::seconds(60 * i64::from(minutes))));
    properties.words = extended.words;

    for pair in &ooxml.properties.custom_properties {
        let key = pair.0.clone();
        let property = match &pair.1 {
            ooxml::UserDefinedProperty::String(v) => UserDefinedProperty::String(v.clone()),
            ooxml::UserDefinedProperty::Int(v) => UserDefinedProperty::Int(*v),
            ooxml::UserDefinedProperty::Real(v) => UserDefinedProperty::Real(*v),
            ooxml::UserDefinedProperty::Bool(v) => UserDefinedProperty::Bool(*v),
            ooxml::UserDefinedProperty::DateTime(v) => {
                UserDefinedProperty::DateTime(OffsetDateTimeWrapper(*v))
            }
            ooxml::UserDefinedProperty::Undecoded => UserDefinedProperty::Undecoded,
        };
        custom_properties.insert(key, property);
    }

    (properties, custom_properties)
}
