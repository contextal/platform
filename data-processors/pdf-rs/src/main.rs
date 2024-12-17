//! PDF backend
use backend_utils::objects::{
    BackendRequest, BackendResultChild, BackendResultKind, BackendResultOk,
};
use ocr_rs::{Dpi, TessBaseApi, TessPageSegMode};
use pdf_rs::{
    annotations::{Annotations, NumberOfAnnotations},
    attachments::{Attachments, NumberOfAttachments},
    bookmarks::{Bookmarks, NumberOfBookmarks},
    builtin_metadata::{BuiltinMetadata, BuiltinMetadataContainer},
    config::{Config, OcrMode},
    document_text::DocumentText,
    fonts::Fonts,
    links::{Links, NumberOfLinks},
    objects::{NumberOfObjects, Objects},
    ocr_text::OcrText,
    paper_sizes::{PaperSizeMillimeters, PaperSizes},
    rendered_page::RenderedPage,
    signatures::{Signature, Signatures},
    thumbnails::Thumbnails,
    PageWithIndex, PdfBackendError, PdfDocumentVersionWrapper, PdfFormTypeWrapper,
};
use pdfium_render::prelude::*;

use serde::Serialize;
use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    fs::{self, read, File},
    io::{self, Write},
    iter::once,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    ptr,
};
use tempfile::NamedTempFile;
use tracing::{error, instrument, trace, warn};
use tracing_subscriber::prelude::*;
use url::Url;

fn main() -> Result<(), PdfBackendError> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::new()?;

    backend_utils::work_loop!(None, None, |request| { process_request(request, &config) })?;

    Ok(())
}

/// A structure to hold metadata attributes of a PDF file.
#[derive(Debug, Serialize)]
struct Metadata {
    /// PDF standard version of the document.
    version: String,

    /// PDF document builtin metadata.
    builtin_metadata: BuiltinMetadata,

    /// PDF form type.
    form_type: String,

    /// List of font names used in the document.
    fonts: Vec<String>,

    /// List of hashes of embedded page thumbnails. Most PDFs don't have them.
    embedded_thumbnails: Vec<String>,

    /// A set of unique tuples containing page sizes in millimeters which appear in a document and
    /// their corresponding standard paper size name (if such standard size exists).
    paper_sizes_mm: Vec<PaperSizeMillimeters>,

    /// Number of annotations in the document.
    number_of_annotations: NumberOfAnnotations,

    /// A structure which holds number of different types of links in a document.
    number_of_links: NumberOfLinks,

    /// A list of unique URIs from document links and annotations links.
    uris: Vec<String>,

    /// Number of objects in the document.
    number_of_objects: NumberOfObjects,

    /// Number of pages in the document.
    number_of_pages: usize,

    /// Number of attachments in the document.
    number_of_attachments: NumberOfAttachments,

    /// List of document signatures
    signatures: Vec<Signature>,

    /// Number of signatures unable to read
    number_of_unreadable_signatures: usize,

    /// Number of bookmarks in the document.
    number_of_bookmarks: NumberOfBookmarks,

    /// Issues faced while processing the document.
    issues: HashSet<&'static str>,

    /// Password used to decrypt document
    password: Option<String>,

    unique_hosts: Vec<String>,
    unique_domains: Vec<String>,
}

/// Processes a PDF document.
///
/// # Returns #
/// * Err for transient retriable errors (I/O, memory allocation, etc)
/// * Ok(BackendResultKind::ok) for success
/// * Ok(BackendResultKind::error) for permanent errors (e.g. invalid or corrupted PDF file,
/// invalid or missing password for password-protected PDF file)
#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &Config,
) -> Result<BackendResultKind, PdfBackendError> {
    thread_local! {
        // A placeholder to keep initialized API handlers between function invocations.
        static STATE_CELL: (RefCell<Option<Pdfium>>, RefCell<Option<TessBaseApi>>) = const { (RefCell::new(None), RefCell::new(None)) };
    }

    let input_path = PathBuf::from(&config.objects_path).join(&request.object.object_id);
    let file = File::open(&input_path).inspect_err(|_| {
        error!("failed to open an input file: {input_path:?}");
    })?;
    let file_size = file
        .metadata()
        .inspect_err(|_| {
            error!("failed to read an input file metadata");
        })?
        .len();
    if file_size > config.max_processed_size {
        return Ok(BackendResultKind::error(format!(
            "PDF file size ({file_size}) exceeds the limit ({})",
            config.max_processed_size
        )));
    }

    // Initialize Pdfium and Tesseract APIs if necessary
    STATE_CELL
        .with(|(pdfium, tesseract)| {
            if pdfium.borrow().is_none() {
                *pdfium.borrow_mut() = Some(Pdfium::new(
                    Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
                        .or_else(|_| Pdfium::bind_to_system_library())
                        .map_err(|e| {
                            error!("failed to initialize Pdfium API: {e}");
                            e
                        })?,
                ));
                trace!("successfully initialized Pdfium library API");
            }
            if tesseract.borrow().is_none() {
                *tesseract.borrow_mut() = Some(
                    TessBaseApi::new("eng", TessPageSegMode::PSM_AUTO, Some(Dpi(150))).map_err(
                        |e| {
                            error!("failed to initialize Tesseract API: {e}");
                            e
                        },
                    )?,
                );
                trace!("successfully initialized Tesseract library API");
            }
            Ok::<_, PdfBackendError>(())
        })
        .inspect_err(|_| {
            error!("failed to initialize API endpoints");
        })?;

    let mut possible_passwords: Vec<Option<&str>> = vec![None];
    if let Some(serde_json::Value::Object(glob)) = request.relation_metadata.get("_global") {
        if let Some(serde_json::Value::Array(passwords)) = glob.get("possible_passwords") {
            for pwd in passwords {
                if let serde_json::Value::String(s) = pwd {
                    possible_passwords.push(Some(s));
                }
            }
        }
    }

    STATE_CELL.with(|(pdfium, tesseract)| {
        let borrow = pdfium.borrow();
        let pdfium = match borrow.as_ref() {
            Some(v) => v,
            None => unreachable!(),
        };
        let borrow = tesseract.borrow();
        let tesseract = match borrow.as_ref() {
            Some(v) => v,
            None => unreachable!(),
        };

        let mut page_render_config = PdfRenderConfig::new();
        if let Some(width) = config.render_page_width {
            page_render_config = page_render_config.set_target_width(width);
        }
        if let Some(height) = config.render_page_height {
            page_render_config = page_render_config.set_maximum_height(height);
        }

        let mut issues = HashSet::new();

        let file_bytes = read(&input_path)?;
        match file_bytes.as_slice() {
            &[b'%', b'P', b'D', b'F', b'-', major, b'.', minor, ..]
                if major.is_ascii_digit() && minor.is_ascii_digit() => {}
            _ => {
                issues.insert("DATA_BEFORE_PDF_HEADER");
            }
        };

        static PDF_TRAILER: &str = "%%EOF";
        let tail_len = 1024.min(file_bytes.len());
        match &file_bytes[file_bytes.len() - tail_len..]
            .windows(PDF_TRAILER.len())
            .position(|v| v == PDF_TRAILER.as_bytes())
        {
            None => {
                issues.insert("NO_PDF_TRAILER_IN_LAST_1K");
            }
            Some(v)
                if !matches!(
                    &file_bytes[file_bytes.len() - tail_len + v + PDF_TRAILER.len()..],
                    b"" | b"\n" | b"\r" | b"\r\n" | b"\n\r",
                ) =>
            {
                issues.insert("DATA_AFTER_PDF_TRAILER");
            }
            _ => {}
        };

        let mut symbols = HashSet::new();

        let mut password: Option<String> = None;
        let mut document = None;
        for p in &possible_passwords {
            match pdfium.load_pdf_from_byte_vec(file_bytes.clone(), *p) {
                Ok(v) => {
                    document = Some(v);
                    password = p.map(|s| s.to_string());
                    break;
                }
                Err(PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError)) => {
                    error!("failed to load PDF from file: file format error");
                    return Ok(BackendResultKind::error("PDF file format error".into()));
                }
                Err(PdfiumError::PdfiumLibraryInternalError(
                    PdfiumInternalError::PasswordError,
                )) => {
                    continue;
                }
                Err(e) => {
                    error!("failed to load PDF from file: {e}");
                    Err(e)?
                }
            };
        }
        let document = match document {
            Some(d) => {
                if password.is_some() {
                    symbols.insert("ENCRYPTED");
                    symbols.insert("DECRYPTED");
                }
                d
            }
            None => {
                if possible_passwords.is_empty() {
                    trace!("PDF file is password-protected and no password has been provided");
                } else {
                    trace!("wrong password supplied for password-protected PDF");
                };
                let relation_metadata = serde_json::Map::<String, serde_json::Value>::new();
                let children = vec![BackendResultChild {
                    path: None,
                    force_type: None,
                    symbols: vec!["ENCRYPTED".to_string()],
                    relation_metadata,
                }];
                return Ok(BackendResultKind::ok(BackendResultOk {
                    symbols: vec!["ENCRYPTED".to_string()],
                    object_metadata: serde_json::Map::<String, serde_json::Value>::new(),
                    children,
                }));
            }
        };

        let version = PdfDocumentVersionWrapper(document.version()).into();
        let form_type = PdfFormTypeWrapper::from(document.form()).into();

        let annotations = Annotations::new(config);
        let attachments = Attachments::new(config);
        let bookmarks = Bookmarks::new(config.max_bookmarks);
        let document_text = DocumentText::new(config);
        let embedded_thumbnails = Thumbnails::new();
        let fonts = Fonts::new(config.max_fonts_per_page);
        let links = Links::new(config.max_links);
        let objects = Objects::new(config);
        let ocr_text = OcrText::new(config);
        let paper_sizes = PaperSizes::new();

        let mut document_context = DocumentContext {
            page_render_config,
            annotations,
            attachments,
            bookmarks,
            document_text,
            embedded_thumbnails,
            fonts,
            links,
            objects,
            ocr_text,
            paper_sizes,
        };

        let mut children = ChildrenGuard::new(
            config.max_children,
            config.max_child_output_size,
            config.max_processed_size,
        );

        let mut pages_results: Vec<Result<(), PdfBackendError>> = Vec::new();
        for (page_index, pdf_page) in (0..config.max_pages).zip(document.pages().iter()) {
            if children.limits_reached() {
                break;
            }
            let page_result = process_page(
                pdf_page,
                page_index,
                &document,
                &mut document_context,
                &mut children,
                tesseract,
                config,
            )?;
            pages_results.push(page_result)
        }

        let number_of_pages = pages_results.len();
        if number_of_pages as u32 == config.max_pages {
            symbols.insert("MAX_PAGES_REACHED");
        }
        if let Some(reason_to_stop) = process_errors(
            pages_results.into_iter().flat_map(|r| match r {
                Ok(_) => None,
                Err(e) => Some(e),
            }),
            "pages",
        ) {
            return reason_to_stop;
        }

        let mut errors = Vec::<PdfBackendError>::new();
        for result in document_context.attachments.process(&document).into_iter() {
            if children.limits_reached() {
                break;
            }
            match result {
                Ok(child) => children.push(child)?,
                Err(e) => errors.push(e),
            }
        }

        if let Some(reason_to_stop) = process_errors(errors.into_iter(), "attachments") {
            return reason_to_stop;
        }
        let Attachments {
            number_of_attachments,
            symbols: attachments_symbols,
            ..
        } = document_context.attachments;

        let Fonts {
            names: fonts_names,
            symbols: fonts_symbols,
            ..
        } = document_context.fonts;
        let mut fonts_names: Vec<_> = fonts_names.into_iter().collect();
        fonts_names.sort();

        if let Some(reason_to_stop) = process_errors(
            document_context
                .bookmarks
                .process(&document)
                .into_iter()
                .filter_map(|r| match r {
                    Ok(_) => None,
                    Err(e) => Some(e),
                }),
            "bookmarks",
        ) {
            return reason_to_stop;
        }
        let Bookmarks {
            uris: bookmarks_uris,
            number_of_bookmarks,
            symbols: bookmarks_symbols,
            issues: bookmarks_issues,
            ..
        } = document_context.bookmarks;

        let Links {
            uris: links_uris,
            number_of_links,
            symbols: links_symbols,
            issues: links_issues,
            ..
        } = document_context.links;

        let Annotations {
            annotations_text,
            uris: annotations_uris,
            number_of_annotations,
            symbols: annotations_symbols,
            issues: annotations_issues,
            ..
        } = document_context.annotations;
        if !annotations_text.is_empty() && !children.limits_reached() {
            match annotations_text.consume() {
                Ok(v) => children.push(v)?,
                Err(e) => {
                    if let Some(reason_to_stop) = process_errors(once(e), "annotations text") {
                        return reason_to_stop;
                    }
                }
            }
        }

        if (config.ocr_mode == OcrMode::Always
            || (config.ocr_mode == OcrMode::IfNoDocumentTextAvailable
                && document_context.document_text.is_empty()))
            && !document_context.ocr_text.is_empty()
            && !children.limits_reached()
        {
            match document_context.ocr_text.consume() {
                Ok(v) => children.push(v)?,
                Err(e) => {
                    if let Some(reason_to_stop) = process_errors(once(e), "OCR text") {
                        return reason_to_stop;
                    }
                }
            }
        }

        let document_text_issues = document_context.document_text.issues.clone();
        if document_context.document_text.is_empty() {
            symbols.insert("NOTEXT");
        } else if !children.limits_reached() {
            match document_context.document_text.consume() {
                Ok(v) => children.push(v)?,
                Err(e) => {
                    if let Some(reason_to_stop) = process_errors(once(e), "document text") {
                        return reason_to_stop;
                    }
                }
            }
        }

        let Objects {
            number_of_objects,
            symbols: objects_symbols,
            issues: objects_issues,
            ..
        } = document_context.objects;

        let BuiltinMetadataContainer {
            builtin_metadata,
            issues: metadata_issues,
        } = document.metadata().into();

        let Thumbnails {
            hashes: embedded_thumbnails,
            issues: thumbnails_issues,
        } = document_context.embedded_thumbnails;

        [
            document_text_issues,
            annotations_issues,
            bookmarks_issues,
            links_issues,
            metadata_issues,
            objects_issues,
            thumbnails_issues,
        ]
        .into_iter()
        .for_each(|v| issues.extend(v));

        if let Err(err) = process_javascript(&document, password.as_deref(), &mut children, config)
        {
            if let Some(reason_to_stop) = process_errors(once(err), "javascript") {
                return reason_to_stop;
            }
        }

        let uris: Vec<String> = links_uris
            .extend(bookmarks_uris)
            .extend(annotations_uris)
            .into();

        let mut unique_hosts = Vec::<String>::new();
        let mut unique_domains = Vec::<String>::new();
        for input in uris.iter() {
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
        if config.create_domain_children {
            for domain in unique_domains.iter() {
                if children.limits_reached() {
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
                            domain: domain.clone(),
                        })? {
                            serde_json::Value::Object(v) => v,
                            _ => unreachable!(),
                        },
                    })?;
                }
            }
        }

        if !issues.is_empty() {
            symbols.insert("ISSUES");
        }
        if children.limits_reached() {
            symbols.insert("LIMITS_REACHED");
        }

        let signatures = Signatures::from_pdf_document(&document, config);
        let signatures_symbols = signatures.symbols;

        let metadata = Metadata {
            builtin_metadata,
            version,
            form_type,
            fonts: fonts_names,
            uris,
            embedded_thumbnails,
            paper_sizes_mm: document_context.paper_sizes.into(),
            number_of_annotations,
            number_of_attachments,
            number_of_bookmarks,
            number_of_links,
            number_of_objects,
            number_of_pages,
            issues,
            password,
            signatures: signatures.signatures,
            number_of_unreadable_signatures: signatures.number_of_unreadable_signatures,
            unique_domains,
            unique_hosts,
        };

        [
            annotations_symbols,
            attachments_symbols,
            bookmarks_symbols,
            fonts_symbols,
            links_symbols,
            objects_symbols,
            signatures_symbols,
        ]
        .into_iter()
        .for_each(|v| symbols.extend(v));

        let symbols: Vec<String> = symbols.into_iter().map(String::from).collect();

        Ok(BackendResultKind::ok(BackendResultOk {
            symbols,
            object_metadata: match serde_json::to_value(metadata)? {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            children: children.into_inner(), // disarms ScopeGuard
        }))
    })
}

/// Goes over `PdfBackendError` iterator, logs all errors, and constructs early-return-value
/// (depending on Error type transiency) to interrupt processing of backend request.
///
/// Returns `None` if there are no errors.
fn process_errors(
    errors: impl Iterator<Item = PdfBackendError>,
    entity_name: &str,
) -> Option<Result<BackendResultKind, PdfBackendError>> {
    let (mut transient, mut nontransient): (VecDeque<_>, VecDeque<_>) = errors
        .into_iter()
        .inspect(|e| match e.is_nontransient() {
            true => error!("nontransient error while processing {entity_name}: {e}"),
            false => warn!("transient error while processing {entity_name}: {e}"),
        })
        .partition(|e| e.is_transient());

    if let Some(e) = transient.pop_front() {
        error!("transient error while processing PDF {entity_name}");
        return Some(Err(e));
    }
    if let Some(e) = nontransient.pop_front() {
        error!("nontransient error while processing PDF {entity_name}");
        return Some(Ok(BackendResultKind::error(format!("{e}"))));
    }

    None
}

struct ChildrenGuard {
    children: Vec<BackendResultChild>,
    children_limit: usize,
    child_size_limit: u64,
    remaining_size: u64,
}

impl ChildrenGuard {
    fn new(max_children: usize, max_child_output_size: u64, max_processed_size: u64) -> Self {
        Self {
            children: Vec::new(),
            children_limit: max_children,
            child_size_limit: max_child_output_size,
            remaining_size: max_processed_size,
        }
    }
    fn push(&mut self, mut child: BackendResultChild) -> Result<(), io::Error> {
        if self.children.len() >= self.children_limit || self.remaining_size == 0 {
            return Ok(());
        }
        if let Some(path) = &mut child.path {
            let metadata = fs::metadata(&path)?;
            if metadata.size() > self.child_size_limit || metadata.size() > self.remaining_size {
                child.symbols.push("TOOBIG".to_string());
                fs::remove_file(path)?;
                child.path = None;
            }
            self.remaining_size = self.remaining_size.saturating_sub(metadata.size());
        }
        self.children.push(child);
        Ok(())
    }
    fn limits_reached(&self) -> bool {
        self.children.len() >= self.children_limit || self.remaining_size == 0
    }
    fn into_inner(mut self) -> Vec<BackendResultChild> {
        let mut result = Vec::new();
        std::mem::swap(&mut self.children, &mut result);
        result
    }
}

impl Drop for ChildrenGuard {
    fn drop(&mut self) {
        self.children
            .iter()
            .filter_map(|child| child.path.as_ref())
            .for_each(|file| {
                let _ = fs::remove_file(file);
            });
    }
}

struct DocumentContext<'a> {
    annotations: Annotations<'a>,
    attachments: Attachments<'a>,
    bookmarks: Bookmarks,
    document_text: DocumentText<'a>,
    embedded_thumbnails: Thumbnails,
    fonts: Fonts,
    links: Links,
    objects: Objects<'a>,
    ocr_text: OcrText<'a>,
    paper_sizes: PaperSizes,
    page_render_config: PdfRenderConfig,
}

fn process_page(
    pdf_page: PdfPage<'_>,
    page_index: u32,
    document: &PdfDocument,
    document_context: &mut DocumentContext,
    children: &mut ChildrenGuard,
    tesseract: &TessBaseApi,
    config: &Config,
) -> Result<Result<(), PdfBackendError>, io::Error> {
    trace!("Page index: {page_index:03}");
    let page = PageWithIndex::from((pdf_page, page_index));

    document_context.annotations.append_from(&page);
    document_context.fonts.append_from(&page);
    document_context.paper_sizes.append_from(&page);
    document_context.links.append_from(&page);

    // Embedded thumbnails are considered to be a source of information of low
    // importance and in case of a error it is logged and ignored.
    let _ = document_context
        .embedded_thumbnails
        .append_from(&page)
        .map_err(|e| {
            warn!("failed to read an embedded thumbnail: {e}");
            e
        });

    // A failure to accumulate page text causes creation of a symbol, but the error
    // itself doesn't interrupt processing of a document/doesn't affect backend
    // response type.
    let _ = document_context
        .document_text
        .append_from(&page)
        .map_err(|e| {
            warn!("failed to obtain page text: {e}");
            e
        });

    // Any object processing errors are counted and logged when appearing. But at
    // the end these errors don't affect backend response type. As it is expected
    // to be quite common to see PDFs which are usable/renderable in most of the
    // viewers, but still have parts/objects which are either not supported by
    // Pdfium, or violating the standard.
    for child in document_context
        .objects
        .append_from(&page, document)
        .into_iter()
        .flatten()
        .flatten()
    {
        children.push(child)?;
    }

    if config.render_pages
        || config.ocr_mode == OcrMode::Always
        || (config.ocr_mode == OcrMode::IfNoDocumentTextAvailable
            && document_context.document_text.is_empty())
    {
        let rendered_page =
            match RenderedPage::try_from((&page, &document_context.page_render_config, config)) {
                Ok(rendered_page) => rendered_page,
                Err(e) => {
                    warn!("failed to render a page: {e}");
                    return Ok(Err(e));
                }
            };

        if config.render_pages {
            let child = match rendered_page.save() {
                Ok(child) => child,
                Err(e) => {
                    warn!("failed to save a rendered page: {e}");
                    return Ok(Err(e));
                }
            };
            children.push(child)?;
        }

        if config.ocr_mode == OcrMode::Always
            || (config.ocr_mode == OcrMode::IfNoDocumentTextAvailable
                && document_context.document_text.is_empty())
        {
            if let Err(e) = document_context
                .ocr_text
                .append_from(rendered_page, tesseract)
            {
                warn!("failed to perform OCR of a rendered page: {e}");
                return Ok(Err(e));
            }
        }
    }
    Ok(Ok(()))
}

#[cfg(test)]
mod test;

macro_rules! extract_string {
    ($bindings:ident, $function:ident, $javascript:expr) => {
        || -> Option<String> {
            let mut size = $bindings.$function($javascript, ptr::null_mut(), 0);
            if size == 0 {
                warn!("{}: failed", stringify!($function));
                return None;
            }
            let mut buffer = vec![0u16; size as usize];
            size = $bindings.$function($javascript, buffer.as_mut_ptr(), size);
            if size == 0 {
                warn!("{}: failed", stringify!($function));
                return None;
            }
            let result: Option<String> = Some(String::from_utf16_lossy(&buffer));
            result
        }()
    };
}
fn process_javascript(
    document: &PdfDocument,
    password: Option<&str>,
    children: &mut ChildrenGuard,
    config: &Config,
) -> Result<(), PdfBackendError> {
    let bytes = document.save_to_bytes()?;
    let bindings = document.bindings();
    let document = bindings.FPDF_LoadMemDocument(&bytes, password);
    if document.is_null() {
        warn!("FPDF_LoadMemDocument() failed");
        return Ok(());
    }
    let count = bindings.FPDFDoc_GetJavaScriptActionCount(document);
    let mut result = Ok(());

    for index in 0..count {
        if children.limits_reached() {
            break;
        }
        let javascript = bindings.FPDFDoc_GetJavaScriptAction(document, index);
        if javascript.is_null() {
            continue;
        }
        let name: Option<String> =
            extract_string!(bindings, FPDFJavaScriptAction_GetName, javascript);
        let script: Option<String> =
            extract_string!(bindings, FPDFJavaScriptAction_GetScript, javascript);
        result = || -> Result<(), PdfBackendError> {
            let Some(name) = name else {
                return Ok(());
            };
            let Some(script) = script else {
                return Ok(());
            };
            let mut output_file = NamedTempFile::new_in(&config.output_path)?;
            output_file.write_all(script.as_bytes())?;
            let output_file = output_file
                .into_temp_path()
                .keep()?
                .into_os_string()
                .into_string()
                .map_err(|path| {
                    io::Error::new(io::ErrorKind::Other, format!("Invalid path: {path:?}"))
                })?;
            let mut relation_metadata = serde_json::Map::<String, serde_json::Value>::new();
            relation_metadata.insert("name".into(), name.into());
            let child = BackendResultChild {
                path: Some(output_file),
                symbols: vec!["JAVASCRIPT".into()],
                relation_metadata,
                force_type: Some("Text".to_string()),
            };
            children.push(child)?;
            Ok(())
        }();
        bindings.FPDFDoc_CloseJavaScriptAction(javascript);
        if result.is_err() {
            break;
        }
    }
    bindings.FPDF_CloseDocument(document);
    result
}

#[derive(Serialize)]
struct DomainMetadata {
    domain: String,
}
