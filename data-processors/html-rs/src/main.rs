mod config;

use backend_utils::objects::*;
use chardetng::EncodingDetector;
use ctxutils::io::{LimitedWriter, WriteLimitExceededError};
use data_url::DataUrl;
use select::document::Document;
use select::predicate::Name;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Error, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::EnvFilter;
use url::Url;

#[derive(Serialize)]
struct HtmlMeta<'a> {
    lang: Option<&'a str>,
    encoding: String,
    href: Vec<String>,
    img_src: Vec<String>,
    img_data_src: Vec<String>,
    unique_hosts: Vec<String>,
    unique_domains: Vec<String>,
    input_types: Vec<String>,
    tag_counters: HashMap<&'a str, usize>,
    tag_count: usize,
    forms: Vec<HashMap<&'a str, &'a str>>,
    scripts: Vec<HashMap<&'a str, &'a str>>,
}

#[derive(Serialize)]
struct DomainMetadata {
    name: String,
}

macro_rules! chkpush {
    ($vec:expr, $url:expr, $children:expr, $max_size:expr, $output_path:expr, $max_children:expr, $limits_reached:expr) => {
        if $url.starts_with("data:") {
            if $children.len() < $max_children as usize {
                if let Ok(child) = process_dataurl($url, $max_size, $output_path) {
                    $children.push(child);
                    if $children.len() == $max_children as usize {
                        $limits_reached = true;
                    }
                }
            }
        } else if $url.len() <= 1024 {
            $vec.push($url.to_string())
        }
    };
}

#[derive(Serialize)]
struct DataUrlMeta {
    mime_type: String,
    decoded_size: usize,
    encoded_size: usize,
}

fn process_dataurl(
    input_url: &str,
    max_child_output_size: u64,
    output_path: Option<&str>,
) -> Result<BackendResultChild, Error> {
    let mut path = None;
    let mut mime_type = String::new();
    let mut decoded_size = 0;

    if let Some(opath) = output_path {
        if let Ok(url) = DataUrl::process(input_url) {
            if let Ok((body, _)) = url.decode_to_vec() {
                if body.len() <= max_child_output_size as usize {
                    let mut output_file = tempfile::NamedTempFile::new_in(opath)?;
                    output_file.write_all(&body)?;
                    path = Some(
                        output_file
                            .into_temp_path()
                            .keep()
                            .unwrap()
                            .into_os_string()
                            .into_string()
                            .unwrap(),
                    );
                    mime_type = format!("{}/{}", url.mime_type().type_, url.mime_type().subtype);
                    decoded_size = body.len();
                }
            }
        }
    }

    if path.is_none() {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Couldn't extract data url",
        ))
    } else {
        let url_meta = DataUrlMeta {
            mime_type,
            decoded_size,
            encoded_size: input_url.len(),
        };
        Ok(BackendResultChild {
            path,
            force_type: None,
            symbols: vec!["RFC2397".to_string()],
            relation_metadata: match serde_json::to_value(url_meta).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
        })
    }
}

fn process_html(
    input_name: &PathBuf,
    output_path: Option<&str>,
    max_processed_size: u64,
    max_child_output_size: u64,
    max_children: u32,
    process_domains: bool,
) -> Result<BackendResultKind, Error> {
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut limits_reached = false;

    let mut f = File::open(input_name)?;
    if f.metadata().unwrap().len() > max_processed_size {
        limits_reached = true;
    }
    let mut handle = f.take(max_processed_size);
    let mut data = String::new();
    let mut symbols: Vec<String> = Vec::new();

    let encoding;
    let html = match handle.read_to_string(&mut data) {
        Ok(_) => {
            encoding = String::from("utf-8");
            Document::from(data.as_str())
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::InvalidData {
                return Err(e);
            }
            let mut buffer = Vec::new();
            f = handle.into_inner();
            f.seek(SeekFrom::Start(0))?;
            handle = f.take(max_processed_size);
            handle.read_to_end(&mut buffer)?;

            let mut detector = EncodingDetector::new();
            detector.feed(&buffer, true);
            let (enc, score) = detector.guess_assess(None, false);
            if !score {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Failed to recognize character encoding",
                ));
            }
            let (cow, _, had_errors) = enc.decode(&buffer);
            if had_errors {
                symbols.push("CHAR_DECODING_ERRORS".into());
            }
            encoding = enc.name().to_string();
            Document::from(cow.to_string().as_str())
        }
    };

    // lang & charset
    let lang = html.find(Name("html")).find_map(|n| n.attr("lang"));
    let charset = html.find(Name("meta")).find_map(|n| n.attr("charset"));

    // HTML tag counters
    // we skip <body>, <head> and <html> as select.rs adds them on its own
    let tags = [
        "!--...--",
        "!DOCTYPE",
        "a",
        "abbr",
        "acronym",
        "address",
        "applet",
        "area",
        "article",
        "aside",
        "audio",
        "b",
        "base",
        "basefont",
        "bdi",
        "bdo",
        "big",
        "blockquote",
        "br",
        "button",
        "canvas",
        "caption",
        "center",
        "cite",
        "code",
        "col",
        "colgroup",
        "data",
        "datalist",
        "dd",
        "del",
        "details",
        "dfn",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "em",
        "embed",
        "fieldset",
        "figcaption",
        "figure",
        "font",
        "footer",
        "form",
        "frame",
        "frameset",
        "h1",
        "header",
        "hgroup",
        "hr",
        "i",
        "iframe",
        "img",
        "input",
        "ins",
        "kbd",
        "label",
        "legend",
        "li",
        "link",
        "main",
        "map",
        "mark",
        "menu",
        "meta",
        "meter",
        "nav",
        "noframes",
        "noscript",
        "object",
        "ol",
        "optgroup",
        "option",
        "output",
        "p",
        "param",
        "picture",
        "pre",
        "progress",
        "q",
        "rp",
        "rt",
        "ruby",
        "s",
        "samp",
        "script",
        "search",
        "section",
        "select",
        "small",
        "source",
        "span",
        "strike",
        "strong",
        "style",
        "sub",
        "summary",
        "sup",
        "svg",
        "table",
        "tbody",
        "td",
        "template",
        "textarea",
        "tfoot",
        "th",
        "thead",
        "time",
        "title",
        "tr",
        "track",
        "tt",
        "u",
        "ul",
        "var",
        "video",
        "wbr",
    ];

    let mut tag_counters = HashMap::new();
    let mut tag_count = 0;
    for tag in tags {
        let count = html.find(Name(tag)).count();
        if count > 0 {
            tag_count += count;
            tag_counters.insert(tag, count);
        }
    }

    // bail out if there are no tags - most likely the file is not html at
    // all or it only contains the three basic tags (see above - we can't
    // properly check for their presence as select.rs would add them anyway
    // if missing in the source file)
    if tag_count == 0 && lang.is_none() && charset.is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "No HTML data found",
        ));
    }

    // collect input types
    let mut input_types = vec![];
    html.find(Name("input"))
        .filter_map(|n| n.attr("type"))
        .for_each(|x| {
            chkpush!(
                input_types,
                x,
                children,
                max_child_output_size,
                output_path,
                max_children,
                limits_reached
            )
        });
    input_types.sort_unstable();
    input_types.dedup();

    // collect hrefs
    let mut href = vec![];
    html.find(Name("a"))
        .filter_map(|n| n.attr("href"))
        .for_each(|x| {
            chkpush!(
                href,
                x,
                children,
                max_child_output_size,
                output_path,
                max_children,
                limits_reached
            )
        });
    href.sort_unstable();
    href.dedup();

    // collect img srcs
    let mut img_src = vec![];
    html.find(Name("img"))
        .filter_map(|n| n.attr("src"))
        .for_each(|x| {
            chkpush!(
                img_src,
                x,
                children,
                max_child_output_size,
                output_path,
                max_children,
                limits_reached
            )
        });
    img_src.sort_unstable();
    img_src.dedup();

    // collect img data-srcs
    let mut img_data_src = vec![];
    html.find(Name("img"))
        .filter_map(|n| n.attr("data-src"))
        .for_each(|x| {
            chkpush!(
                img_data_src,
                x,
                children,
                max_child_output_size,
                output_path,
                max_children,
                limits_reached
            )
        });
    img_data_src.sort_unstable();
    img_data_src.dedup();

    // collect script srcs
    let mut script_src = vec![];
    html.find(Name("script"))
        .filter_map(|n| n.attr("src"))
        .for_each(|x| {
            chkpush!(
                script_src,
                x,
                children,
                max_child_output_size,
                output_path,
                max_children,
                limits_reached
            )
        });
    script_src.sort_unstable();
    script_src.dedup();

    // determine unique hosts and domains
    let mut unique_hosts = vec![];
    let mut unique_domains = vec![];
    for n in href
        .iter()
        .chain(img_src.iter())
        .chain(img_data_src.iter())
        .chain(script_src.iter())
    {
        if let Ok(url) = Url::parse(n) {
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

    // extract text
    let text_tags = [
        "h1", "h2", "h3", "h4", "h5", "h6", "p", "label", "textarea", "dialog",
    ];
    let mut have_text_tags = false;
    for t in &text_tags {
        if tag_counters.contains_key(t) {
            have_text_tags = true;
            break;
        }
    }

    if have_text_tags {
        if let Some(path) = output_path {
            let mut output_text_file = tempfile::NamedTempFile::new_in(path)?;
            let mut text_writer = LimitedWriter::new(&mut output_text_file, max_child_output_size);

            let mut text_symbols: Vec<String> = Vec::new();
            for t in text_tags {
                for e in html.find(Name(t)) {
                    let text = e.text() + "\n";
                    if text.len() > 1 {
                        let r = text_writer.write_all(text.as_bytes());
                        if let Err(err) = r {
                            if err
                                .get_ref()
                                .is_some_and(|e| e.is::<WriteLimitExceededError>())
                            {
                                text_symbols.push("TOOBIG".to_string());
                                limits_reached = true;
                            } else {
                                return Err(err);
                            }
                        }
                    }
                }
            }

            if text_writer.written() > 0 {
                children.push(BackendResultChild {
                    path: Some(
                        output_text_file
                            .into_temp_path()
                            .keep()
                            .unwrap()
                            .into_os_string()
                            .into_string()
                            .unwrap(),
                    ),
                    force_type: Some("Text".to_string()),
                    symbols: text_symbols,
                    relation_metadata: Metadata::new(),
                });
            }
        }
    }

    // collect information about forms
    let mut forms = vec![];
    if tag_counters.contains_key("form") {
        for form in html.find(Name("form")) {
            let mut fattrs = HashMap::new();
            for (k, v) in form.attrs() {
                if k.len() <= 512 && v.len() <= 512 {
                    fattrs.insert(k, v);
                }
            }
            if !fattrs.is_empty() {
                forms.push(fattrs);
            }
        }
    }

    // collect information about scripts
    let mut scripts = vec![];
    if tag_counters.contains_key("script") {
        for script in html.find(Name("script")) {
            let mut attrs = HashMap::new();
            for (k, v) in script.attrs() {
                if k.len() <= 512 && v.len() <= 512 {
                    attrs.insert(k, v);
                }
            }
            if !attrs.is_empty() {
                scripts.push(attrs);
            }
        }
    }

    // create Domain children
    if process_domains {
        if let Some(path) = output_path {
            for domain in unique_domains.iter() {
                if children.len() >= max_children as usize {
                    limits_reached = true;
                    break;
                }
                let mut domain_file = tempfile::NamedTempFile::new_in(path)?;
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
    }

    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    let html_meta = HtmlMeta {
        lang,
        encoding,
        href,
        img_src,
        img_data_src,
        unique_hosts,
        unique_domains,
        input_types,
        tag_counters,
        tag_count,
        forms,
        scripts,
    };
    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(html_meta).unwrap() {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children,
    }))
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());
    match process_html(
        &input_name,
        Some(&config.output_path),
        config.max_processed_size,
        config.max_child_output_size,
        config.max_children,
        config.create_domain_children,
    ) {
        Ok(h) => Ok(h),
        Err(e) => Ok(BackendResultKind::error(format!(
            "Error processing HTML data: {}",
            e
        ))),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::builder()
        .from_env()?
        .add_directive("html5ever::tree_builder=error".parse()?);

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = config::Config::new()?;
    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        process_request(request, &config)
    })?;
    unreachable!()
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct HtmlMetaOwned {
    lang: Option<String>,
    encoding: String,
    href: Vec<String>,
    img_src: Vec<String>,
    img_data_src: Vec<String>,
    unique_hosts: Vec<String>,
    input_types: Vec<String>,
    tag_counters: HashMap<String, usize>,
    tag_count: usize,
    forms: Vec<HashMap<String, String>>,
    scripts: Vec<HashMap<String, String>>,
}

#[test]
fn parse_html() {
    let path = PathBuf::from("tests/test_data/test.html");
    let brk = process_html(&path, None, 65535, 65535, 50, false).unwrap();
    let hm: HtmlMetaOwned;
    if let BackendResultKind::ok(br) = brk {
        hm = serde_json::from_value(serde_json::Value::Object(br.object_metadata)).unwrap();
    } else {
        panic!("Invalid result");
    }

    assert_eq!(hm.lang, Some("en-US".to_string()), "lang mismatch");
    assert_eq!(hm.encoding, "utf-8".to_string(), "encoding mismatch");
    assert_eq!(
        hm.href[0],
        "https://contextal.com/".to_string(),
        "href[0] mismatch"
    );
    assert_eq!(
        hm.unique_hosts[0],
        "contextal.com".to_string(),
        "unique_hosts[0] mismatch"
    );
    assert_eq!(
        hm.input_types[0],
        "password".to_string(),
        "input_types[0] mismatch"
    );
    assert_eq!(
        hm.input_types[2],
        "text".to_string(),
        "input_types[2] mismatch"
    );
    assert_eq!(
        hm.input_types[2],
        "text".to_string(),
        "input_types[2] mismatch"
    );
    assert_eq!(
        hm.forms[0].get("action"),
        Some(&"/foo_action.php".to_string()),
        "forms[0] action mismatch"
    );
    assert_eq!(
        hm.tag_counters.get("br"),
        Some(&5),
        "tag_counters <br> mismatch"
    );
}
