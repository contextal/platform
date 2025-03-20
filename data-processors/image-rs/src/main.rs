mod config;

use backend_utils::objects::{
    BackendRequest, BackendResultChild, BackendResultKind, BackendResultOk,
};
use exif::In;
use flate2::read::GzDecoder;
use image::{DynamicImage, EncodableLayout, GenericImage, ImageError, RgbaImage};
use imageproc::filter::laplacian_filter;
use ocr_rs::{Dpi, TessBaseApi, TessPageSegMode};
use resvg::tiny_skia;
use resvg::usvg::{self, fontdb, Options, Size};
use serde::Serialize;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use tracing::*;
use tracing_subscriber::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = config::Config::new()?;
    let global_context = Rc::new(RefCell::new(GlobalContext::default()));
    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        process_request(request, &config, &mut global_context.borrow_mut())
    })?;
    unreachable!()
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
    global_context: &mut GlobalContext,
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());

    let request_ocr = matches!(
        request.relation_metadata.get("request_ocr"),
        Some(Value::Bool(true))
    );
    let image_info = match ImageInfo::load(input_name, request_ocr, global_context) {
        Ok(v) => v,
        Err(e)
            if e.kind() == std::io::ErrorKind::InvalidData
                || e.kind() == std::io::ErrorKind::UnexpectedEof =>
        {
            return Ok(BackendResultKind::error(format!("Invalid data ({})", e)))
        }
        Err(e) => return Err(e),
    };

    let blur_treshold = 100_f64;
    let blurred = image_info.metadata.laplacian_variance < Some(blur_treshold);

    let object_metadata = match serde_json::to_value(image_info.metadata).unwrap() {
        serde_json::Value::Object(v) => v,
        _ => unreachable!(),
    };

    let mut limits_reached = false;
    let mut children = Vec::<BackendResultChild>::new();
    if let Some(ocr) = image_info.ocr {
        let mut bytes = ocr.as_bytes();
        let mut file = tempfile::NamedTempFile::new_in(&config.output_path)?;
        let mut writer =
            ctxutils::io::LimitedWriter::new(file.as_file_mut(), config.max_child_output_size);
        let mut entry_symbols = vec!["OCR".to_string()];
        let path = match std::io::copy(&mut bytes, &mut writer) {
            Ok(_) => {
                let output_file = file
                    .into_temp_path()
                    .keep()?
                    .into_os_string()
                    .into_string()
                    .map_err(|s| {
                        std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("failed to convert OsString {s:?} to String"),
                        )
                    })?;
                Some(output_file)
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::Other => {
                    entry_symbols.push("TOOBIG".to_string());
                    limits_reached = true;
                    None
                }
                _ => {
                    return Err(e);
                }
            },
        };
        children.push(BackendResultChild {
            path,
            symbols: entry_symbols,
            relation_metadata: serde_json::Map::<String, Value>::new(),
            force_type: Some("Text/OCR".to_string()),
        });
    }

    if let Some(qrcode) = image_info.qrcode {
        let mut bytes = qrcode.data.as_bytes();
        let mut file = tempfile::NamedTempFile::new_in(&config.output_path)?;
        let mut writer =
            ctxutils::io::LimitedWriter::new(file.as_file_mut(), config.max_child_output_size);
        let mut entry_symbols = vec!["QRCODE".to_string()];
        let path = match std::io::copy(&mut bytes, &mut writer) {
            Ok(_) => {
                let output_file = file
                    .into_temp_path()
                    .keep()?
                    .into_os_string()
                    .into_string()
                    .map_err(|s| {
                        std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("failed to convert OsString {s:?} to String"),
                        )
                    })?;
                Some(output_file)
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::Other => {
                    entry_symbols.push("TOOBIG".to_string());
                    limits_reached = true;
                    None
                }
                _ => {
                    return Err(e);
                }
            },
        };
        if path.is_some() {
            children.push(BackendResultChild {
                path,
                symbols: entry_symbols,
                force_type: Some("Text/QR-Code".to_string()),
                relation_metadata: match serde_json::to_value(qrcode).unwrap() {
                    serde_json::Value::Object(v) => v,
                    _ => unreachable!(),
                },
            });
        }
    }

    let mut symbols = Vec::<String>::new();
    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }
    if blurred {
        symbols.push("BLURRED".to_string());
    }
    let mut image_symbols = image_info.symbols;
    symbols.append(&mut image_symbols);

    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata,
        children,
    }))
}

#[derive(Serialize)]
struct QrMeta {
    size: usize,
    ecc_level: u16,
    mask: u16,
}

#[derive(Serialize)]
struct QrCode {
    qrcode: Vec<QrMeta>,
    #[serde(skip_serializing)]
    data: String,
}

struct ImageInfo {
    metadata: ImageInfoMetadata,
    ocr: Option<String>,
    qrcode: Option<QrCode>,
    symbols: Vec<String>,
}

#[derive(Serialize)]
struct ImageInfoMetadata {
    format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pixel_format: Option<String>,
    width: u32,
    height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    exif: Option<Exif>,
    nsfw_predictions: Option<HashMap<String, f64>>,
    nsfw_verdict: Option<String>,
    laplacian_variance: Option<f64>,
}

#[derive(Serialize)]
struct Exif {
    primary: HashMap<String, String>,
    thumbnail: HashMap<String, String>,
}

fn map_image_error(error: ImageError) -> std::io::Error {
    match error {
        image::ImageError::IoError(e) => e,
        image::ImageError::Unsupported(e) => io::Error::new(io::ErrorKind::InvalidData, e),
        image::ImageError::Decoding(e) => io::Error::new(io::ErrorKind::InvalidData, e),
        _ => io::Error::new(io::ErrorKind::Other, error),
    }
}

fn map_usvg_error(error: resvg::usvg::Error) -> std::io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn map_gif_error(error: gif::DecodingError) -> std::io::Error {
    match error {
        gif::DecodingError::Format(error) => io::Error::new(io::ErrorKind::InvalidData, error),
        gif::DecodingError::Io(error) => error,
    }
}

fn map_pixel_format(color_type: image::ColorType) -> Option<String> {
    let pixel_format = match color_type {
        image::ColorType::L8 => "L8",
        image::ColorType::La8 => "LA8",
        image::ColorType::Rgb8 => "RGB8",
        image::ColorType::Rgba8 => "RGBA8",
        image::ColorType::L16 => "L16",
        image::ColorType::La16 => "LA16",
        image::ColorType::Rgb16 => "RGB16",
        image::ColorType::Rgba16 => "RGBA16",
        image::ColorType::Rgb32F => "RGB32F",
        image::ColorType::Rgba32F => "RGBA32F",
        _ => return None,
    }
    .to_string();
    Some(pixel_format)
}

fn find_font<'a>(fontdb: &fontdb::Database, fonts: &[&'a str]) -> Option<&'a str> {
    for font in fonts {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(font)],
            ..fontdb::Query::default()
        };
        if fontdb.query(&query).is_some() {
            return Some(font);
        }
    }
    None
}

/// Returns initialized font database and name of default font
fn initialize_fontdb() -> Result<(Arc<fontdb::Database>, String), io::Error> {
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();

    let serif_fonts = [
        "DejaVu Serif",
        "Liberation Serif",
        "Droid Serif",
        "FreeSerif",
        "Noto Serif",
    ];
    let sans_fonts = [
        "DejaVu Sans",
        "Liberation Sans",
        "Droid Sans",
        "FreeSans",
        "Noto Sans",
    ];
    let mono_fonts = [
        "DejaVu Sans Mono",
        "Liberation Mono",
        "Droid Mono",
        "FreeMono",
        "Noto Mono",
    ];
    let serif_family = find_font(&fontdb, &serif_fonts).ok_or(io::Error::new(
        io::ErrorKind::Other,
        "Unable to find serif font",
    ))?;
    let default_font = serif_family.to_string();
    fontdb.set_serif_family(serif_family);
    let sans_family = find_font(&fontdb, &sans_fonts).ok_or(io::Error::new(
        io::ErrorKind::Other,
        "Unable to find sans font",
    ))?;
    fontdb.set_sans_serif_family(sans_family);
    fontdb.set_cursive_family(sans_family);
    fontdb.set_fantasy_family(sans_family);
    fontdb.set_monospace_family(find_font(&fontdb, &mono_fonts).ok_or(io::Error::new(
        io::ErrorKind::Other,
        "Unable to find mono font",
    ))?);

    Ok((Arc::new(fontdb), default_font))
}

#[derive(Default)]
struct GlobalContext {
    tesseract: Option<TessBaseApi>,
    fontdb: Option<(Arc<fontdb::Database>, String)>,
    nsfw_model: Option<nsfw::Model>,
}

fn new_size_and_scale(width: f32, height: f32, scale: f32) -> Result<(Size, f32), String> {
    let size =
        Size::from_wh(width, height).ok_or(format!("Unable to create size {width}x{height}"))?;
    Ok((size, scale))
}

fn normalize_size(size: Size, min: f32, max: f32) -> Result<(Size, f32), String> {
    let mut width = size.width();
    let mut height = size.height();

    let swap = width < height;
    if swap {
        core::mem::swap(&mut width, &mut height);
    }
    if width > max {
        height = max * height / width;
        width = max;
    }
    if height < min {
        width = min * width / height;
        height = min;
    }
    if swap {
        core::mem::swap(&mut width, &mut height);
    }

    let scale = width / size.width();
    new_size_and_scale(width, height, scale)
}

fn check_svg(data: &[u8]) -> Option<Vec<String>> {
    let mut buf = Vec::<u8>::new();
    let mut slice = data;
    if slice.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(slice);
        if decoder.read_to_end(&mut buf).is_ok() {
            slice = &buf;
        }
    }

    let mut reader = quick_xml::reader::Reader::from_reader(slice);
    let mut is_svg = false;
    let mut result = Vec::new();

    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(err) => {
                result.push("SVG_ERRORS".to_string());
                warn!("{err}");
                break;
            }
        };
        match event {
            quick_xml::events::Event::Start(bytes_start) => {
                let name = bytes_start.name().0;
                match name.as_bytes() {
                    b"svg" => is_svg = true,
                    b"script" => {
                        result.push("SVG_JS".to_string());
                        break;
                    }
                    _ => {}
                }
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        };
    }
    if is_svg {
        Some(result)
    } else {
        None
    }
}

fn is_gif(data: &[u8]) -> bool {
    data.starts_with(b"GIF89a")
}

macro_rules! invalid_data_error {
    ($error:expr) => {
        io::Error::new(io::ErrorKind::InvalidData, $error)
    };
}

fn verify_image_size(width: u16, height: u16) -> Result<(), io::Error> {
    let width = usize::from(width);
    let height = usize::from(height);
    let tmp = width
        .checked_mul(4)
        .ok_or(invalid_data_error!("Overflow"))?;
    tmp.checked_mul(height)
        .ok_or(invalid_data_error!("Overflow"))?;
    Ok(())
}

fn load_gif(data: &[u8]) -> Result<DynamicImage, io::Error> {
    let mut decode_options = gif::DecodeOptions::new();
    decode_options.check_frame_consistency(false);
    decode_options.set_color_output(gif::ColorOutput::RGBA);
    decode_options.skip_frame_decoding(true);

    let cursor = Cursor::new(&data);
    let mut decoder = decode_options.read_info(cursor).map_err(map_gif_error)?;
    println!("{}x{}", decoder.width(), decoder.height());

    let mut width = decoder.width();
    let mut height = decoder.height();

    while let Some(frame) = decoder.next_frame_info().map_err(map_gif_error)? {
        let right = frame
            .left
            .checked_add(frame.width)
            .ok_or(invalid_data_error!(
                "Image size overflows vector maximum size"
            ))?;
        if right > width {
            width = right;
        }
        let bottom = frame
            .top
            .checked_add(frame.height)
            .ok_or(invalid_data_error!(
                "Image size overflows vector maximum size"
            ))?;
        if bottom > height {
            height = bottom;
        }
    }

    let mut decode_options = gif::DecodeOptions::new();
    decode_options.check_frame_consistency(false);
    decode_options.set_color_output(gif::ColorOutput::RGBA);
    let cursor = Cursor::new(data);
    let mut decoder = decode_options.read_info(cursor).map_err(map_gif_error)?;

    verify_image_size(width, height)?; // prevent panic in next line
    let mut result = RgbaImage::new(width.into(), height.into());

    while let Some(frame) = decoder.read_next_frame().map_err(map_gif_error)? {
        let image = RgbaImage::from_raw(
            frame.width.into(),
            frame.height.into(),
            frame.buffer.to_vec(),
        )
        .ok_or(invalid_data_error!("RgbaImage::from_raw failed"))?;
        result
            .copy_from(&image, frame.left.into(), frame.top.into())
            .map_err(map_image_error)?;
    }
    Ok(result.into())
}

impl ImageInfo {
    pub fn load(
        path: PathBuf,
        request_ocr: bool,
        global_context: &mut GlobalContext,
    ) -> Result<ImageInfo, std::io::Error> {
        let mut file = std::fs::File::open(path)?;
        let mut data = Vec::<u8>::new();
        file.read_to_end(&mut data)?;

        let width;
        let height;
        let format;
        let pixel_format;
        let image;

        let skip_exif;
        let mut detect_nsfw = false;
        let mut symbols = Vec::new();

        if let Some(mut svg_symbols) = check_svg(&data) {
            symbols.append(&mut svg_symbols);
            let (fontdb, default_font) = if let Some(pair) = &global_context.fontdb {
                pair
            } else {
                global_context.fontdb = Some(initialize_fontdb()?);
                global_context.fontdb.as_ref().unwrap()
            };

            let opt = Options {
                font_family: default_font.to_string(),
                fontdb: fontdb.clone(),
                ..Default::default()
            };

            let tree = usvg::Tree::from_data(&data, &opt).map_err(map_usvg_error)?;
            let pixmap_size = tree.size().to_int_size();
            width = pixmap_size.width();
            height = pixmap_size.height();
            let (size, scale) = normalize_size(tree.size(), 50_f32, 5000_f32)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            let pixmap_size = size.to_int_size();

            let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
                .ok_or(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Unable to allocate new tiny_skia::Pixmap",
                ))?;
            debug!("SVG tree loaded.");
            resvg::render(
                &tree,
                tiny_skia::Transform::from_scale(scale, scale),
                &mut pixmap.as_mut(),
            );
            let data = pixmap.encode_png()?;
            image = image::load_from_memory_with_format(&data, image::ImageFormat::Png)
                .map_err(map_image_error)?;

            format = "svg".to_string();
            pixel_format = None;
            skip_exif = true;
        } else if is_gif(&data) {
            format = "gif".to_string();
            skip_exif = true;
            image = load_gif(&data)?;
            width = image.width();
            height = image.height();
            pixel_format = Some("RGBA8".to_string());
        } else {
            skip_exif = false;
            let image_format = image::guess_format(&data).map_err(map_image_error)?;
            debug!("Recognized image format: {image_format:?}. Loading image");
            image = image::load_from_memory_with_format(&data, image_format)
                .map_err(map_image_error)?;
            width = image.width();
            height = image.height();
            let color_type = image.color();

            pixel_format = map_pixel_format(color_type);

            format = match image_format {
                image::ImageFormat::Png => {
                    detect_nsfw = true;
                    "png"
                }
                image::ImageFormat::Jpeg => {
                    detect_nsfw = true;
                    "jpeg"
                }
                image::ImageFormat::Gif => "gif",
                image::ImageFormat::WebP => {
                    detect_nsfw = true;
                    "webp"
                }
                image::ImageFormat::Pnm => "pnm",
                image::ImageFormat::Tiff => "tiff",
                image::ImageFormat::Tga => "tga",
                image::ImageFormat::Dds => "dds",
                image::ImageFormat::Bmp => "bmp",
                image::ImageFormat::Ico => "ico",
                image::ImageFormat::Hdr => "hdr",
                image::ImageFormat::OpenExr => "openexr",
                image::ImageFormat::Farbfeld => "farbfeld",
                image::ImageFormat::Avif => "avif",
                image::ImageFormat::Qoi => "qoi",
                _ => "Unknown",
            }
            .to_string();
        }

        let exif = if skip_exif {
            None
        } else {
            let exifreader = exif::Reader::new();
            let mut cursor = Cursor::new(&data);
            match exifreader.read_from_container(&mut cursor) {
                Ok(exif) => {
                    let mut primary = HashMap::<String, String>::new();
                    let mut thumbnail = HashMap::<String, String>::new();
                    for f in exif.fields() {
                        let map_ref = match f.ifd_num {
                            In::PRIMARY => &mut primary,
                            In::THUMBNAIL => &mut thumbnail,
                            _ => unreachable!(),
                        };

                        let key = (|map_ref: &mut HashMap<String, String>| {
                            let mut index = 1;
                            let mut tag_name = f.tag.to_string();
                            if tag_name.starts_with("Tag(") {
                                let context = match f.tag.0 {
                                    exif::Context::Tiff => "Tiff",
                                    exif::Context::Exif => "Exif",
                                    exif::Context::Gps => "Gps",
                                    exif::Context::Interop => "Interlop",
                                    _ => "Unknown",
                                };
                                tag_name = format!("Tag_{}_{}", context, f.tag.1);
                            }
                            loop {
                                let key = match index {
                                    1 => &tag_name,
                                    _ => &format!("{}_{}", tag_name, index),
                                };
                                if !map_ref.contains_key(key) {
                                    return key.to_string();
                                }
                                index += 1;
                            }
                        })(map_ref);
                        let value = f.display_value().with_unit(&exif).to_string();

                        map_ref.insert(key, value);
                    }
                    Some(Exif { primary, thumbnail })
                }
                Err(_) => None,
            }
        };

        let mut ocr = None;

        // Initialize Pdfium and Tesseract APIs if necessary
        if request_ocr {
            let tesseract = if let Some(tesseract) = &global_context.tesseract {
                tesseract
            } else {
                global_context.tesseract = Some(
                    TessBaseApi::new("eng", TessPageSegMode::PSM_AUTO, Some(Dpi(150))).map_err(
                        |e| {
                            error!("failed to initialize Tesseract API: {e}");
                            std::io::Error::new(std::io::ErrorKind::Other, e)
                        },
                    )?,
                );
                trace!("successfully initialized Tesseract library API");
                global_context.tesseract.as_ref().unwrap()
            };

            ocr = match perform_ocr(&image, tesseract) {
                Ok(text) => {
                    if text.is_empty() {
                        None
                    } else {
                        Some(text)
                    }
                }
                _ => None,
            }
        }

        // QR Code processing
        let qrcode = {
            let img = image.to_luma8();
            let mut img = rqrr::PreparedImage::prepare(img);
            let grids = img.detect_grids();
            let mut meta = vec![];
            let mut data = String::new();
            for grid in grids.iter() {
                if let Ok((m, content)) = grid.decode() {
                    if !content.is_empty() {
                        data += &(content + "\n");
                        meta.push(QrMeta {
                            size: m.version.to_size(),
                            ecc_level: m.ecc_level,
                            mask: m.mask,
                        });
                    }
                }
            }
            if data.is_empty() {
                None
            } else {
                Some(QrCode { qrcode: meta, data })
            }
        };

        // NSFW detection
        let (nsfw_predictions, nsfw_verdict) = if detect_nsfw {
            let nsfw_model = if let Some(nsfw_model) = &global_context.nsfw_model {
                nsfw_model
            } else {
                let file = File::open("nsfw.onnx").map_err(|e| {
                    error!("failed to load NSFW model data: {e}");
                    std::io::Error::new(std::io::ErrorKind::Other, e)
                })?;
                global_context.nsfw_model = Some(nsfw::create_model(file).map_err(|_| {
                    error!("failed to initialize NSFW model data");
                    std::io::Error::new(std::io::ErrorKind::Other, "NSFW init error")
                })?);
                trace!("successfully initialized NSFW model");
                global_context.nsfw_model.as_ref().unwrap()
            };

            if let Ok(nsfw_res) = nsfw::examine(nsfw_model, &image.to_rgba8()) {
                let mut predictions = HashMap::new();
                let mut verdict = "Unknown".to_string();
                for p in nsfw_res {
                    predictions.insert(
                        p.metric.to_string(),
                        (p.score as f64 * 1000000.0).round() / 1000000.0,
                    );
                    if p.score > 0.5 {
                        verdict = p.metric.to_string();
                    }
                }
                (Some(predictions), Some(verdict))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let gray = image.grayscale().to_luma8();
        let laplacian = laplacian_filter(&gray);
        let laplacian_variance = variance(laplacian.as_raw());

        let metadata = ImageInfoMetadata {
            format,
            pixel_format,
            width,
            height,
            exif,
            nsfw_predictions,
            nsfw_verdict,
            laplacian_variance,
        };

        Ok(ImageInfo {
            metadata,
            ocr,
            qrcode,
            symbols,
        })
    }
}

fn perform_ocr(image: &DynamicImage, tesseract: &TessBaseApi) -> Result<String, std::io::Error> {
    tesseract.set_rgba_image(&image.to_rgba8()).map_err(|e| {
        info!("failed to pass an image to Tesseract");
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;

    tesseract.recognize().map_err(|e| {
        info!("text recognition step has failed");
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;

    let text = tesseract.get_text().map_err(|e| {
        info!("failed to obtain recognized text");
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;

    Ok(text.trim().to_string())
}

fn mean(data: &[i16]) -> Option<f64> {
    if data.is_empty() {
        return None;
    }
    let sum = data.iter().map(|v| f64::from(*v)).sum::<f64>();
    let count = f64::from(u32::try_from(data.len()).ok()?);
    Some(sum / count)
}

fn variance(data: &[i16]) -> Option<f64> {
    let data_mean = mean(data)?;
    let count = f64::from(u32::try_from(data.len()).ok()?);
    let variance = data
        .iter()
        .map(|value| {
            let diff = data_mean - f64::from(*value);
            diff * diff
        })
        .sum::<f64>()
        / count;
    Some(variance)
}

#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use tempfile::NamedTempFile;

#[cfg(test)]
fn create_tempfile(hex_string: &str) -> Result<NamedTempFile, std::io::Error> {
    if hex_string.len() % 2 != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid hex size",
        ));
    }
    let mut file = NamedTempFile::new()?;
    let mut data = Vec::<u8>::new();
    for i in (0..hex_string.len()).step_by(2) {
        let hex = &hex_string[i..i + 2];
        let byte = u8::from_str_radix(hex, 16)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        data.push(byte);
    }
    file.write_all(&data)?;
    file.flush()?;
    Ok(file)
}

#[test]
fn test_png_rgb8() -> Result<(), std::io::Error> {
    let hex = "89504e470d0a1a0a0000000d49484452000000030000000208020000001216f1\
                    4d00000185694343504943432070726f66696c65000028917d913d48c3501485\
                    4f53a5a5541cec20e290a1ea62172de258ab50840aa15668d5c1e4a57fd0a425\
                    497171145c0b0efe2c561d5c9c7575701504c11f10670727451729f1bea4d022\
                    c60b8ff771de3d87f7ee0384569569665f02d074cbc8a492622ebf2a065ee143\
                    1021c4312133b33e27496978d6d73d7553ddc5789677df9f35a0164c06f844e2\
                    04ab1b16f106f1cca655e7bc4f1c616559253e279e34e882c48f5c575c7ee35c\
                    7258e09911239b99278e108ba51e567a98950d8d384e1c55359df2859ccb2ae7\
                    2dce5ab5c13af7e42f0c17f49565aed31a450a8b588204110a1aa8a00a0b31da\
                    75524c64e83ce9e11f71fc12b9147255c0c8b1801a34c88e1ffc0f7ecfd62c4e\
                    4fb949e124d0ff62db1f6340601768376dfbfbd8b6db2780ff19b8d2bbfe5a0b\
                    98fd24bdd9d5a247c0e0367071ddd5943de07207187eaacb86ec487e5a42b108\
                    bc9fd137e581a15b20b4e6cead738ed307204bb34adf000787c07889b2d73dde\
                    1dec9ddbbf3d9df9fd00b02972bf62b0db80000000097048597300002e230000\
                    2e230178a53f760000000774494d4507e801080b1a392d6aaf22000000197445\
                    5874436f6d6d656e74004372656174656420776974682047494d5057810e1700\
                    0000164944415408d763fcffff3f03030303030313030c000036060301970ad1\
                    640000000049454e44ae426082";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "png");
    assert_eq!(image_info.pixel_format, Some("RGB8".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, Some("Neutral".to_string()));
    Ok(())
}

#[test]
fn test_png_l8() -> Result<(), std::io::Error> {
    let hex = "89504e470d0a1a0a0000000d4948445200000003000000020800000000b81f39\
                    c6000000097048597300002e2300002e230178a53f760000000774494d4507e8\
                    01080b1b0ffbcb0bfa0000001974455874436f6d6d656e740043726561746564\
                    20776974682047494d5057810e17000000104944415408d763fccfc0c0c4c0c0\
                    000007110103fa329a690000000049454e44ae426082";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "png");
    assert_eq!(image_info.pixel_format, Some("L8".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, Some("Neutral".to_string()));
    Ok(())
}

#[test]
fn test_png_rgba8() -> Result<(), std::io::Error> {
    let hex = "89504e470d0a1a0a0000000d49484452000000030000000208060000009d7466\
                    1a00000185694343504943432070726f66696c65000028917d913d48c3501485\
                    4f53a5a5541cec20e290a1ea62172de258ab50840aa15668d5c1e4a57fd0a425\
                    497171145c0b0efe2c561d5c9c7575701504c11f10670727451729f1bea4d022\
                    c60b8ff771de3d87f7ee0384569569665f02d074cbc8a492622ebf2a065ee143\
                    1021c4312133b33e27496978d6d73d7553ddc5789677df9f35a0164c06f844e2\
                    04ab1b16f106f1cca655e7bc4f1c616559253e279e34e882c48f5c575c7ee35c\
                    7258e09911239b99278e108ba51e567a98950d8d384e1c55359df2859ccb2ae7\
                    2dce5ab5c13af7e42f0c17f49565aed31a450a8b588204110a1aa8a00a0b31da\
                    75524c64e83ce9e11f71fc12b9147255c0c8b1801a34c88e1ffc0f7ecfd62c4e\
                    4fb949e124d0ff62db1f6340601768376dfbfbd8b6db2780ff19b8d2bbfe5a0b\
                    98fd24bdd9d5a247c0e0367071ddd5943de07207187eaacb86ec487e5a42b108\
                    bc9fd137e581a15b20b4e6cead738ed307204bb34adf000787c07889b2d73dde\
                    1dec9ddbbf3d9df9fd00b02972bf62b0db80000000097048597300002e230000\
                    2e230178a53f760000000774494d4507e801080b1b20501a36a3000000197445\
                    5874436f6d6d656e74004372656174656420776974682047494d5057810e1700\
                    0000134944415408d763fcffffff7f062860624002005df004008fdedfd70000\
                    000049454e44ae426082";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "png");
    assert_eq!(image_info.pixel_format, Some("RGBA8".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, Some("Neutral".to_string()));
    Ok(())
}

#[test]
fn test_png_la8() -> Result<(), std::io::Error> {
    let hex = "89504e470d0a1a0a0000000d4948445200000003000000020804000000377dae\
                    91000000097048597300002e2300002e230178a53f760000000774494d4507e8\
                    01080b1b2d2eab4a1e0000001974455874436f6d6d656e740043726561746564\
                    20776974682047494d5057810e17000000164944415408d763fcff9f81818181\
                    898181818181010019110202dce52ae20000000049454e44ae426082";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "png");
    assert_eq!(image_info.pixel_format, Some("LA8".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, Some("Neutral".to_string()));
    Ok(())
}

#[test]
fn test_png_rgb16() -> Result<(), std::io::Error> {
    let hex = "89504e470d0a1a0a0000000d494844520000000300000002100200000042862d\
                    0e00000185694343504943432070726f66696c65000028917d913d48c3501485\
                    4f53a5a5541cec20e290a1ea62172de258ab50840aa15668d5c1e4a57fd0a425\
                    497171145c0b0efe2c561d5c9c7575701504c11f10670727451729f1bea4d022\
                    c60b8ff771de3d87f7ee0384569569665f02d074cbc8a492622ebf2a065ee143\
                    1021c4312133b33e27496978d6d73d7553ddc5789677df9f35a0164c06f844e2\
                    04ab1b16f106f1cca655e7bc4f1c616559253e279e34e882c48f5c575c7ee35c\
                    7258e09911239b99278e108ba51e567a98950d8d384e1c55359df2859ccb2ae7\
                    2dce5ab5c13af7e42f0c17f49565aed31a450a8b588204110a1aa8a00a0b31da\
                    75524c64e83ce9e11f71fc12b9147255c0c8b1801a34c88e1ffc0f7ecfd62c4e\
                    4fb949e124d0ff62db1f6340601768376dfbfbd8b6db2780ff19b8d2bbfe5a0b\
                    98fd24bdd9d5a247c0e0367071ddd5943de07207187eaacb86ec487e5a42b108\
                    bc9fd137e581a15b20b4e6cead738ed307204bb34adf000787c07889b2d73dde\
                    1dec9ddbbf3d9df9fd00b02972bf62b0db80000000097048597300002e230000\
                    2e230178a53f760000000774494d4507e801080b1b37d3c9b364000000197445\
                    5874436f6d6d656e74004372656174656420776974682047494d5057810e1700\
                    0000164944415408d763fcffffffffffff1990001303060000cea305fe25cefb\
                    360000000049454e44ae426082";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "png");
    assert_eq!(image_info.pixel_format, Some("RGB16".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, Some("Neutral".to_string()));
    Ok(())
}

#[test]
fn test_png_l16() -> Result<(), std::io::Error> {
    let hex = "89504e470d0a1a0a0000000d4948445200000003000000021000000000e88fe5\
                    85000000097048597300002e2300002e230178a53f760000000774494d4507e8\
                    01080b1c05545f74230000001974455874436f6d6d656e740043726561746564\
                    20776974682047494d5057810e17000000164944415408d763fcff9f81818181\
                    898181818181010019110202dce52ae20000000049454e44ae426082";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "png");
    assert_eq!(image_info.pixel_format, Some("L16".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, Some("Neutral".to_string()));
    Ok(())
}

#[test]
fn test_png_rgba16() -> Result<(), std::io::Error> {
    let hex = "89504e470d0a1a0a0000000d4948445200000003000000021006000000cde4ba\
                    5900000185694343504943432070726f66696c65000028917d913d48c3501485\
                    4f53a5a5541cec20e290a1ea62172de258ab50840aa15668d5c1e4a57fd0a425\
                    497171145c0b0efe2c561d5c9c7575701504c11f10670727451729f1bea4d022\
                    c60b8ff771de3d87f7ee0384569569665f02d074cbc8a492622ebf2a065ee143\
                    1021c4312133b33e27496978d6d73d7553ddc5789677df9f35a0164c06f844e2\
                    04ab1b16f106f1cca655e7bc4f1c616559253e279e34e882c48f5c575c7ee35c\
                    7258e09911239b99278e108ba51e567a98950d8d384e1c55359df2859ccb2ae7\
                    2dce5ab5c13af7e42f0c17f49565aed31a450a8b588204110a1aa8a00a0b31da\
                    75524c64e83ce9e11f71fc12b9147255c0c8b1801a34c88e1ffc0f7ecfd62c4e\
                    4fb949e124d0ff62db1f6340601768376dfbfbd8b6db2780ff19b8d2bbfe5a0b\
                    98fd24bdd9d5a247c0e0367071ddd5943de07207187eaacb86ec487e5a42b108\
                    bc9fd137e581a15b20b4e6cead738ed307204bb34adf000787c07889b2d73dde\
                    1dec9ddbbf3d9df9fd00b02972bf62b0db80000000097048597300002e230000\
                    2e230178a53f760000000774494d4507e801080b1c0d5a84fc11000000197445\
                    5874436f6d6d656e74004372656174656420776974682047494d5057810e1700\
                    0000124944415408d763fc0f050c688089010700006b3907fcaad9bd6c000000\
                    0049454e44ae426082";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "png");
    assert_eq!(image_info.pixel_format, Some("RGBA16".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, Some("Neutral".to_string()));
    Ok(())
}

#[test]
fn test_png_la16() -> Result<(), std::io::Error> {
    let hex = "89504e470d0a1a0a0000000d494844520000000300000002100400000067ed72\
                    d2000000097048597300002e2300002e230178a53f760000000774494d4507e8\
                    01080b1c16d0e135fd0000001974455874436f6d6d656e740043726561746564\
                    20776974682047494d5057810e17000000134944415408d763fcffffff7f0628\
                    60624002005df004008fdedfd70000000049454e44ae426082";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "png");
    assert_eq!(image_info.pixel_format, Some("LA16".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, Some("Neutral".to_string()));
    Ok(())
}

#[test]
fn test_bmp() -> Result<(), std::io::Error> {
    let hex = "424d9a000000000000008a0000007c0000000300000002000000010010000300\
                    000010000000232e0000232e0000000000000000000000f80000e00700001f00\
                    0000000000004247527300000000000000000000000000000000000000000000\
                    0000000000000000000000000000000000000000000000000000020000000000\
                    00000000000000000000ffffffffffff0000ffffffffffff0000";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "bmp");
    assert_eq!(image_info.pixel_format, Some("RGB8".to_string()));
    assert!(image_info.exif.is_none());
    assert_eq!(image_info.nsfw_verdict, None);
    Ok(())
}

#[test]
fn test_ico() -> Result<(), std::io::Error> {
    let hex = "0000010001000302020001000100400000001600000028000000030000000400\
                    000001000100000000000000000000000000000000000000000000000000ffff\
                    ff000000000000000000000000000000000000000000";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "ico");
    assert_eq!(image_info.pixel_format, Some("RGBA8".to_string()));
    assert!(image_info.exif.is_none());
    Ok(())
}

#[test]
fn test_gif() -> Result<(), std::io::Error> {
    let hex = "47494638396103000200800000ffffffffffff21fe1143726561746564207769\
                    74682047494d50002c0000000003000200000202845f003b";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "gif");
    assert_eq!(image_info.pixel_format, Some("RGBA8".to_string()));
    assert!(image_info.exif.is_none());
    Ok(())
}

#[test]
fn test_gif_2() -> Result<(), std::io::Error> {
    let hex = "47494638396100000000f797000000000d00001600001c00002200002600\
                    002a00002e00003200003500003800003b00003d00004000004200004500\
                    004b00004d00005100005500005600005800005a00005c00005d00006000\
                    006200006300006500006600006900006a00006c00006d00006e00007000\
                    007200007500007700007800007c00007d00007e00007f00008000008400\
                    008500008700008800008a00008b00008d00008e00008f00009000009100\
                    009300009400009600009700009800009900009a00009b00009c00009d00\
                    009e00009f0000a10000a20000a30000a40000a50000a60000a70000a900\
                    00aa0000ac0000ad0000af0000b10000b20000b30000b40000b60000b700\
                    00b80000b90000bb0000bc0000bd0000be0000bf0000c00000c10000c200\
                    00c40000c50000c60000c70000c80000ca0000cb0000cc0000cd0000ce00\
                    00cf0000d00000d10000d20000d30000d40000d50000d80000d90000da00\
                    00db0000dc0000dd0000de0000df0000e10000e20000e30000e40000e500\
                    00e70000e80000e90000ea0000eb0000ec0000ed0000ee0000ef0000f000\
                    00f10000f20000f30000f40000f50000f60000f70000f80000f90000fa00\
                    00fb0000fc0000fd0000fe0000ff0000ffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\
                    ff21f904000200000021fe27474946206564697465642077697468206874\
                    7470733a2f2f657a6769662e636f6d2f7370656564002cc8003200640032\
                    000008ff002d592223a5a0c18307e7085cc850601c001021de694851e080\
                    88004c20dcb8314ec58f201bb6c04892a49490961e629c88d2d2c5923063\
                    3a694993e1c8982651aa8cc812e54b9c4001ccac59f36650002743ee9448\
                    f3e751984389b6341a3429c8a5007a8674fa946454a960054acaa994a456\
                    904e99845dbb762c46ab1fb19efd9896ad5da26e23c2ad28b72946b57703\
                    87cc0b712fc5be2deb0a5e4c9130529d66fd4604ccb8b225c7861b22f6f9\
                    d7b265cc90574a8648d9b360d0654527ee6c7a31eaab91574f6eed9a2c6c\
                    d59c67d30efc3a6eecdca477f3b6ed1bf756d6c2d9f6e6fbfbb8eee46b1d\
                    11676e1c2d72e814ff186a04a951a24076ca348e5141607a434658487a69\
                    243b38f68a5df5524464c688079c2092a05144f7fafb85f11546d115f175\
                    d1df73ff01d8950545ec31608107ba97a0823139e08213680c0212815d19\
                    589162130a3406175b7c3186196de031c82321b6e8e28b30c628e38c34d6\
                    68e38d38e6a8e38e3cf6e8e38f400629e490441669e4914826a9e4924c36\
                    e9e493504629e59454ca18100021f90400020000002c0000000064003200\
                    0008ff002d091c48b0a0c18308132a5cc8b0a1c38710234a9c48b1a2c58b\
                    18336adcc8b1a3c78f20438a1c49b2a4c9932853aa5cc9b2a5cb973063ca\
                    9c49b3a6cd9b3873eadcc9b3a7cf9f40830a1d4ab4a8d1a348932a5dcab4\
                    a9453500a24a9d4ab5aad5a9280c0eb8cab52a8c9150bb8aed9ab5e0d6b1\
                    5dbf8a0c8bb6add4b204cfbaf50a76ee5cb803e5da95aaf6e0988c7ea408\
                    1e4c78700aaa500a2b962246eb54138b230f2e83708e809352a84a9aa897\
                    894648150060d6cc79aae78c5fa28e9eba5962678d25549bccccbab4d4d3\
                    171f05905d92b6d4d6115f63f42375f56fdb51715bc4537c3669d7a633f2\
                    69defb79f0e8180b5127e93b2a7088c22f4a3d32c09bbb75f0d831b6283f\
                    b23b80ef0fc35f4c2dda796de8b73f3fa85ffdfef5fc1a91c19f79fea127\
                    1564922de686424418e71d727b01d060429404040021f90400020000002c\
                    64000000640032000008ff002d091c48b0a0c18308132a5cc8b0a1c38710\
                    234a9c48b1a2c58b18336adcc8b1a3c78f20438a1c49b2a4c9932853aa5c\
                    c9b2a5cb973063ca9c49b3a6cd9b3873eadcc9b3a7cf9f40830a1d4ab4a8\
                    d1a348932a5d8af3d11e3661b67421a326cea0494c0b46f233d050161302\
                    00881d3b56c007256d16195443b6addbb770c5a208e9a8cd900c0100584a\
                    546440dcb70268b021c8f6afe1bf733df2e9e197ec1b0787e18e201cb972\
                    dbc41b03b9809bd7725b2d943d5bc68c711215029505dc28d307d1234581\
                    f07cd911a16d21827ea4e8decd7b778ab6507a0b97224663a316703b2c41\
                    435685208594ea104100e0c444296d25993ce4c16d01235c058ec36da1bd\
                    21232b65ae672fe928845b19840a8a3d70a82476b2e545527ad136006883\
                    62f960d27d63e5175217fd9d819058600cb89e48855047d6160989958683\
                    f88de4435b352824561718160861586321808887001c11a258067ae4445b\
                    512c24560390d8f72048139025c089280240858d1982d4475b333044d617\
                    2411c8624858b435869164edc023484a02d022473ab4150894641df04422\
                    54dee851772552c2655b0408a18747555ea9d1248d894542434afc15c217\
                    8c6cd426488bb465434301010021f90400020000002cc800000064003200\
                    0008ff002d091c48b0a0c18308132a5cc8b0a1c38710234a9c48b1a2c58b\
                    18336adcc8b1a3c78f20438a1c49b2a4c9932853aa5cc9b2a5cb973063ca\
                    9c49b3a6cd9b3873eadcc9b3a7cf9f40830a1d4ab4a8d1a348932a5d2a13\
                    4e1a315bbe905113279024a60e0168ddba35c0051f62fe28bcc2b5acd92e\
                    0807985dcb562b0c8d6dcb629802e820d9b85cd11e548b17efdb8c7db9fa\
                    b01b58ab5e837c0baffd8b5131803d07f50cb9803703913e691d2fd698c7\
                    10a3478800e10943e443d9140b0dadc151b6071b440b13033021a5b6eddb\
                    b86d972159e88b0aad6d1ac6297ba7a16c263aff40a1249cb871aec8930e\
                    e75a9ce171a5d3b7568f0d1dbb73ebdda57f38e7be353ad2ec5ab72bbc2e\
                    9efaf3f2dedd8387df5efb7bade68fa207a03e21fbf3e3ad171e80f29187\
                    5f7cf6cd77607de9dd07407e2905040021f90400020000002c0000320064\
                    0032000008ff002d091c48b0a0c18308074a01c090a1a4841007349c48b1\
                    e2442210336adc4870e1c4871c094ab448b222c69028535af2d81064ca91\
                    25630238a9b26646960e6bc204c0c4a6cf9f057102708972674fa0486d0a\
                    251ad268d2a72997ea9c7814aad59b149972747ab52b42a92ab97a1dab30\
                    ebd48655c98e05fb92aa5ab56c8bba7dbbd66cd8b974bbc66d8a37afd5bd\
                    5bfbfa7d0a78a3d8c184edb6458bf8af62b98c91ae59d4b8b0c6c33e430c\
                    903166b0e58c3b4d48194dbab469d2650c8668e8f9315f993261a866edf7\
                    7344d8b167336cfdf12c6e92b20b3631b0bbb6ebc0bf811f7cf4266de3e7\
                    d0a34b9f4ebdbaf5ebd8b36bdfcebdbbf7efe0c38b1b1f4fbebcf9f3e8d3\
                    ab5fcfbebdfbf7f0e3cb9f4fbfbefdfb1b03020021f90400020000002c64\
                    003200640032000008ff002d915900a0a0c1830586f4b1c4b0a1c387101d\
                    4a39084052c48b182f16a22824e3434339288a043063a1478f130f5a3cc9\
                    12e21f8a485a36e45323c04883018a34920931a5c1953c5bfaa1682428c3\
                    425224dc2c48818f51863e0b027d9a9110c51f542d5192a363c0cd03769e\
                    46ad98d5a3228a36ca324c34c5c1c805848c8e9daaf6a1248a22ea327434\
                    a58048167229d2d5dbb0c241048419fee920b24ed0b98921c2a03828b222\
                    0d441f0b8efcd009c5339ceb50dca059256787702806391de26080493c21\
                    9fde2be06082489c955064147bf36c862d289ae19c85a2a2dea67f5b5a43\
                    5103a5c85628e29629fbb7240814bf4406727042e99fca1976f1a168e00f\
                    61490c0ef2f82e35bca5491c285e28a477ec1af664ddd7b1791083f9b266\
                    d466100583a1e49b7b5788640018cf3944df498a1421d27df815785a253f\
                    8cb481198e340480053954f1461f8328f2082385dc01060d5e65165872ee\
                    59528911371120c3156f2ca52300413458618c0e6de1d78e441ec4401859\
                    5507a4257f985024910610715c92072ec9501c2b8c7482013b3a10c3183b\
                    a9a5a4950c0dd2450b0a182463227bbc6186185d6c31c61a713c48e69d96\
                    1842871a78f6e9e79f80062ae8a084166ae8a18826aae8a28c36eae8a390\
                    462ae9a494566ae9a59866aae9a69c76eae9a7a0862aeaa8640604003b";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info = ImageInfo::load(tempfile.path().to_path_buf(), true, &mut global_context)?;
    assert_eq!(image_info.metadata.width, 300);
    assert_eq!(image_info.metadata.height, 100);
    assert_eq!(image_info.metadata.format, "gif");
    assert_eq!(image_info.metadata.pixel_format, Some("RGBA8".to_string()));
    assert!(image_info.metadata.exif.is_none());
    assert_eq!(image_info.ocr, Some("TEST GIF".to_string()));
    Ok(())
}

#[test]
fn test_jpeg() -> Result<(), std::io::Error> {
    let hex = "ffd8ffe000104a46494600010101012c012c0000fffe00134372656174656420\
                    776974682047494d50ffe202b04943435f50524f46494c45000101000002a06c\
                    636d73044000006d6e74725247422058595a2007e800010008000a0034002761\
                    6373704150504c00000000000000000000000000000000000000000000000000\
                    00f6d6000100000000d32d6c636d730000000000000000000000000000000000\
                    0000000000000000000000000000000000000000000000000000000000000d64\
                    6573630000012000000040637072740000016000000036777470740000019800\
                    00001463686164000001ac0000002c7258595a000001d8000000146258595a00\
                    0001ec000000146758595a000002000000001472545243000002140000002067\
                    54524300000214000000206254524300000214000000206368726d0000023400\
                    000024646d6e640000025800000024646d64640000027c000000246d6c756300\
                    000000000000010000000c656e5553000000240000001c00470049004d005000\
                    20006200750069006c0074002d0069006e002000730052004700426d6c756300\
                    000000000000010000000c656e55530000001a0000001c005000750062006c00\
                    69006300200044006f006d00610069006e000058595a20000000000000f6d600\
                    0100000000d32d736633320000000000010c42000005defffff3250000079300\
                    00fd90fffffba1fffffda2000003dc0000c06e58595a200000000000006fa000\
                    0038f50000039058595a20000000000000249f00000f840000b6c458595a2000\
                    000000000062970000b787000018d97061726100000000000300000002666600\
                    00f2a700000d59000013d000000a5b6368726d00000000000300000000a3d700\
                    00547c00004ccd0000999a0000266700000f5c6d6c7563000000000000000100\
                    00000c656e5553000000080000001c00470049004d00506d6c75630000000000\
                    0000010000000c656e5553000000080000001c0073005200470042ffdb004300\
                    0302020302020303030304030304050805050404050a070706080c0a0c0c0b0a\
                    0b0b0d0e12100d0e110e0b0b1016101113141515150c0f171816141812141514\
                    ffdb00430103040405040509050509140d0b0d14141414141414141414141414\
                    1414141414141414141414141414141414141414141414141414141414141414\
                    1414141414ffc20011080002000303011100021101031101ffc4001400010000\
                    0000000000000000000000000008ffc400140101000000000000000000000000\
                    00000000ffda000c03010002100310000001549fffc400141001000000000000\
                    00000000000000000000ffda00080101000105027fffc4001411010000000000\
                    0000000000000000000000ffda0008010301013f017fffc40014110100000000\
                    000000000000000000000000ffda0008010201013f017fffc400141001000000\
                    00000000000000000000000000ffda0008010100063f027fffc4001410010000\
                    0000000000000000000000000000ffda0008010100013f217fffda000c030100\
                    020003000000109fffc40014110100000000000000000000000000000000ffda\
                    0008010301013f107fffc40014110100000000000000000000000000000000ff\
                    da0008010201013f107fffc40014100100000000000000000000000000000000\
                    ffda0008010100013f107fffd9";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "jpeg");
    assert_eq!(image_info.pixel_format, Some("RGB8".to_string()));
    assert!(image_info.exif.is_none());
    Ok(())
}

#[test]
fn test_webp() -> Result<(), std::io::Error> {
    let hex = "52494646e002000057454250565038580a000000200000000200000100004943\
                    4350a0020000000002a06c636d73044000006d6e74725247422058595a2007e8\
                    00010008000a00340027616373704150504c0000000000000000000000000000\
                    000000000000000000000000f6d6000100000000d32d6c636d73000000000000\
                    0000000000000000000000000000000000000000000000000000000000000000\
                    0000000000000000000d64657363000001200000004063707274000001600000\
                    003677747074000001980000001463686164000001ac0000002c7258595a0000\
                    01d8000000146258595a000001ec000000146758595a00000200000000147254\
                    5243000002140000002067545243000002140000002062545243000002140000\
                    00206368726d0000023400000024646d6e640000025800000024646d64640000\
                    027c000000246d6c756300000000000000010000000c656e5553000000240000\
                    001c00470049004d00500020006200750069006c0074002d0069006e00200073\
                    0052004700426d6c756300000000000000010000000c656e55530000001a0000\
                    001c005000750062006c0069006300200044006f006d00610069006e00005859\
                    5a20000000000000f6d6000100000000d32d736633320000000000010c420000\
                    05defffff325000007930000fd90fffffba1fffffda2000003dc0000c06e5859\
                    5a200000000000006fa0000038f50000039058595a20000000000000249f0000\
                    0f840000b6c458595a2000000000000062970000b787000018d9706172610000\
                    000000030000000266660000f2a700000d59000013d000000a5b6368726d0000\
                    0000000300000000a3d70000547c00004ccd0000999a0000266700000f5c6d6c\
                    756300000000000000010000000c656e5553000000080000001c00470049004d\
                    00506d6c756300000000000000010000000c656e5553000000080000001c0073\
                    005200470042565038201a0000003001009d012a0300020000a01227a4000370\
                    00fefedd78000000";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "webp");
    assert_eq!(image_info.pixel_format, Some("RGB8".to_string()));
    assert!(image_info.exif.is_none());
    Ok(())
}

#[test]
fn test_pnm() -> Result<(), std::io::Error> {
    let hex = "50360a2320437265617465642062792047494d502076657273696f6e20322e31\
                    302e333420504e4d20706c75672d696e0a3320320a3235350affffffffffffff\
                    ffffffffffffffffffffff";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "pnm");
    assert_eq!(image_info.pixel_format, Some("RGB8".to_string()));
    assert!(image_info.exif.is_none());
    Ok(())
}

#[test]
fn test_tiff() -> Result<(), std::io::Error> {
    let hex = "49492a000c000000f8fff8ff1200000103000100000003000000010103000100\
                    0000020000000201030003000000fa0000000301030001000000058000000601\
                    030001000000020000000e010200120000000601000011010400010000000800\
                    0000120103000100000001000000150103000100000003000000160103000100\
                    0000800000001701040001000000040000001a01050001000000ea0000001b01\
                    050001000000f20000001c01030001000000010000001d01020005000000b803\
                    000028010300010000000200000053010300030000000001000073870700a002\
                    000018010000000000002c010000010000002c01000001000000080008000800\
                    0100010001004372656174656420776974682047494d5000000002a06c636d73\
                    044000006d6e74725247422058595a2007e800010008000a0034002761637370\
                    4150504c0000000000000000000000000000000000000000000000000000f6d6\
                    000100000000d32d6c636d730000000000000000000000000000000000000000\
                    0000000000000000000000000000000000000000000000000000000d64657363\
                    0000012000000040637072740000016000000036777470740000019800000014\
                    63686164000001ac0000002c7258595a000001d8000000146258595a000001ec\
                    000000146758595a000002000000001472545243000002140000002067545243\
                    00000214000000206254524300000214000000206368726d0000023400000024\
                    646d6e640000025800000024646d64640000027c000000246d6c756300000000\
                    000000010000000c656e5553000000240000001c00470049004d005000200062\
                    00750069006c0074002d0069006e002000730052004700426d6c756300000000\
                    000000010000000c656e55530000001a0000001c005000750062006c00690063\
                    00200044006f006d00610069006e000058595a20000000000000f6d600010000\
                    0000d32d736633320000000000010c42000005defffff325000007930000fd90\
                    fffffba1fffffda2000003dc0000c06e58595a200000000000006fa0000038f5\
                    0000039058595a20000000000000249f00000f840000b6c458595a2000000000\
                    000062970000b787000018d9706172610000000000030000000266660000f2a7\
                    00000d59000013d000000a5b6368726d00000000000300000000a3d70000547c\
                    00004ccd0000999a0000266700000f5c6d6c756300000000000000010000000c\
                    656e5553000000080000001c00470049004d00506d6c75630000000000000001\
                    0000000c656e5553000000080000001c007300520047004254c5826f00";

    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "tiff");
    assert_eq!(image_info.pixel_format, Some("RGB8".to_string()));
    assert!(image_info.exif.is_some());

    let exif = image_info.exif.unwrap();
    let primary = &exif.primary;
    assert_eq!(primary.len(), 18);
    let thumbanil = &exif.thumbnail;
    assert_eq!(thumbanil.len(), 0);

    assert_eq!(primary.get("BitsPerSample").unwrap(), "8, 8, 8");
    assert_eq!(primary.get("Compression").unwrap(), "PackBits");
    assert_eq!(
        primary.get("ImageDescription").unwrap(),
        "\"Created with GIMP\""
    );
    assert_eq!(primary.get("ImageLength").unwrap(), "2 pixels");
    assert_eq!(primary.get("ImageWidth").unwrap(), "3 pixels");
    assert_eq!(
        primary.get("Orientation").unwrap(),
        "row 0 at top and column 0 at left"
    );
    assert_eq!(primary.get("PhotometricInterpretation").unwrap(), "RGB");
    assert_eq!(primary.get("PlanarConfiguration").unwrap(), "chunky");
    assert_eq!(primary.get("ResolutionUnit").unwrap(), "inch");
    assert_eq!(primary.get("RowsPerStrip").unwrap(), "128");
    assert_eq!(primary.get("SamplesPerPixel").unwrap(), "3");
    assert_eq!(primary.get("StripByteCounts").unwrap(), "4");
    assert_eq!(primary.get("StripOffsets").unwrap(), "8");
    assert_eq!(primary.get("Tag_Tiff_285").unwrap(), "\"T\\xc5\\x82o\"");
    assert_eq!(primary.get("Tag_Tiff_339").unwrap(), "1, 1, 1");
    let tag_tiff_34675 = "0x\
                                000002a06c636d73044000006d6e74725247422058595a2007e800010008000a\
                                00340027616373704150504c0000000000000000000000000000000000000000\
                                000000000000f6d6000100000000d32d6c636d73000000000000000000000000\
                                0000000000000000000000000000000000000000000000000000000000000000\
                                0000000d64657363000001200000004063707274000001600000003677747074\
                                000001980000001463686164000001ac0000002c7258595a000001d800000014\
                                6258595a000001ec000000146758595a00000200000000147254524300000214\
                                000000206754524300000214000000206254524300000214000000206368726d\
                                0000023400000024646d6e640000025800000024646d64640000027c00000024\
                                6d6c756300000000000000010000000c656e5553000000240000001c00470049\
                                004d00500020006200750069006c0074002d0069006e00200073005200470042\
                                6d6c756300000000000000010000000c656e55530000001a0000001c00500075\
                                0062006c0069006300200044006f006d00610069006e000058595a2000000000\
                                0000f6d6000100000000d32d736633320000000000010c42000005defffff325\
                                000007930000fd90fffffba1fffffda2000003dc0000c06e58595a2000000000\
                                00006fa0000038f50000039058595a20000000000000249f00000f840000b6c4\
                                58595a2000000000000062970000b787000018d9706172610000000000030000\
                                000266660000f2a700000d59000013d000000a5b6368726d0000000000030000\
                                0000a3d70000547c00004ccd0000999a0000266700000f5c6d6c756300000000\
                                000000010000000c656e5553000000080000001c00470049004d00506d6c7563\
                                00000000000000010000000c656e5553000000080000001c0073005200470042";
    assert_eq!(primary.get("Tag_Tiff_34675").unwrap(), tag_tiff_34675);
    assert_eq!(primary.get("XResolution").unwrap(), "300 pixels per inch");
    assert_eq!(primary.get("YResolution").unwrap(), "300 pixels per inch");

    Ok(())
}

#[test]
fn test_hdr() -> Result<(), std::io::Error> {
    let hex = "233f52414449414e43450a534f4654574152453d4745474c0a464f524d41543d\
                    33322d6269745f726c655f726762650a0a2d592032202b5820330a8080808180\
                    80808180808081808080818080808180808081";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "hdr");
    assert_eq!(image_info.pixel_format, Some("RGB32F".to_string()));
    assert!(image_info.exif.is_none());
    Ok(())
}

#[test]
fn test_openexr() -> Result<(), std::io::Error> {
    let hex = "762f3101020000006368616e6e656c730063686c697374003700000042000200\
                    0000000000000100000001000000470002000000000000000100000001000000\
                    520002000000000000000100000001000000006368726f6d6174696369746965\
                    73006368726f6d6174696369746965730020000000f4d6233f17f7a83e199a99\
                    3ed299193f239a193ea1bf753d371aa03eb072a83e636f6d7072657373696f6e\
                    00636f6d7072657373696f6e0001000000036461746157696e646f7700626f78\
                    3269001000000000000000000000000200000001000000646973706c61795769\
                    6e646f7700626f7832690010000000000000000000000002000000010000006c\
                    696e654f72646572006c696e654f72646572000100000000706978656c417370\
                    656374526174696f00666c6f617400040000000000803f73637265656e57696e\
                    646f7743656e746572007632660008000000000000000000000073637265656e\
                    57696e646f77576964746800666c6f617400040000000000803f008301000000\
                    000000000000000f000000785e63602002ec772408013fc511c0";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info =
        ImageInfo::load(tempfile.path().to_path_buf(), false, &mut global_context)?.metadata;
    assert_eq!(image_info.width, 3);
    assert_eq!(image_info.height, 2);
    assert_eq!(image_info.format, "openexr");
    assert_eq!(image_info.pixel_format, Some("RGB32F".to_string()));
    assert!(image_info.exif.is_none());
    Ok(())
}

#[test]
fn test_svg() -> Result<(), std::io::Error> {
    let hex = "3c3f786d6c2076657273696f6e3d22312e302220656e636f64696e673d22\
                    5554462d3822207374616e64616c6f6e653d226e6f223f3e0a3c212d2d20\
                    43726561746564207769746820496e6b73636170652028687474703a2f2f\
                    7777772e696e6b73636170652e6f72672f29202d2d3e0a0a3c7376670a20\
                    202077696474683d22333030220a2020206865696768743d22313030220a\
                    20202076696577426f783d223020302037392e3337343939392032362e34\
                    3538333333220a20202076657273696f6e3d22312e31220a20202069643d\
                    2273766735220a202020696e6b73636170653a76657273696f6e3d22312e\
                    322e322028623061383438363534312c20323032322d31322d303129220a\
                    202020736f6469706f64693a646f636e616d653d22727973756e656b2e73\
                    7667220a202020786d6c6e733a696e6b73636170653d22687474703a2f2f\
                    7777772e696e6b73636170652e6f72672f6e616d657370616365732f696e\
                    6b7363617065220a202020786d6c6e733a736f6469706f64693d22687474\
                    703a2f2f736f6469706f64692e736f75726365666f7267652e6e65742f44\
                    54442f736f6469706f64692d302e647464220a202020786d6c6e733d2268\
                    7474703a2f2f7777772e77332e6f72672f323030302f737667220a202020\
                    786d6c6e733a7376673d22687474703a2f2f7777772e77332e6f72672f32\
                    3030302f737667223e0a20203c736f6469706f64693a6e616d6564766965\
                    770a202020202069643d226e616d65647669657737220a20202020207061\
                    6765636f6c6f723d2223666666666666220a2020202020626f7264657263\
                    6f6c6f723d2223303030303030220a2020202020626f726465726f706163\
                    6974793d22302e3235220a2020202020696e6b73636170653a73686f7770\
                    616765736861646f773d2232220a2020202020696e6b73636170653a7061\
                    67656f7061636974793d22302e30220a2020202020696e6b73636170653a\
                    70616765636865636b6572626f6172643d2230220a2020202020696e6b73\
                    636170653a6465736b636f6c6f723d2223643164316431220a2020202020\
                    696e6b73636170653a646f63756d656e742d756e6974733d226d6d220a20\
                    2020202073686f77677269643d2266616c7365220a2020202020696e6b73\
                    636170653a7a6f6f6d3d22372e333139323438220a2020202020696e6b73\
                    636170653a63783d2234332e373838363532220a2020202020696e6b7363\
                    6170653a63793d223130372e3331393737220a2020202020696e6b736361\
                    70653a77696e646f772d77696474683d2233383430220a2020202020696e\
                    6b73636170653a77696e646f772d6865696768743d2232303735220a2020\
                    202020696e6b73636170653a77696e646f772d783d2230220a2020202020\
                    696e6b73636170653a77696e646f772d793d2230220a2020202020696e6b\
                    73636170653a77696e646f772d6d6178696d697a65643d2231220a202020\
                    2020696e6b73636170653a63757272656e742d6c617965723d226c617965\
                    723122202f3e0a20203c646566730a202020202069643d22646566733222\
                    3e0a202020203c726563740a20202020202020783d2232362e3433343632\
                    34220a20202020202020793d2233332e3634373732220a20202020202020\
                    77696474683d2231302e3931333034220a20202020202020686569676874\
                    3d2232302e383431393135220a2020202020202069643d22726563743237\
                    3422202f3e0a20203c2f646566733e0a20203c670a2020202020696e6b73\
                    636170653a6c6162656c3d22576172737477612031220a2020202020696e\
                    6b73636170653a67726f75706d6f64653d226c61796572220a2020202020\
                    69643d226c6179657231223e0a202020203c726563740a20202020202020\
                    7374796c653d2266696c6c3a236666303030303b7374726f6b652d776964\
                    74683a302e343538323731220a2020202020202069643d22726563743330\
                    31220a2020202020202077696474683d2237392e343737343933220a2020\
                    20202020206865696768743d2232362e353131393637220a202020202020\
                    20783d22302e303333353839343731220a20202020202020793d22302e30\
                    313231343135393922202f3e0a202020203c746578740a20202020202020\
                    786d6c3a73706163653d227072657365727665220a202020202020207374\
                    796c653d22666f6e742d73697a653a392e3134363670783b66696c6c3a23\
                    3030303030303b7374726f6b652d77696474683a302e3736323231363b2d\
                    696e6b73636170652d666f6e742d73706563696669636174696f6e3a2773\
                    657269667878782c204e6f726d616c273b666f6e742d66616d696c793a73\
                    657269667878783b666f6e742d7765696768743a6e6f726d616c3b666f6e\
                    742d7374796c653a6e6f726d616c3b666f6e742d737472657463683a6e6f\
                    726d616c3b666f6e742d76617269616e743a6e6f726d616c3b666f6e742d\
                    76617269616e742d6c69676174757265733a6e6f726d616c3b666f6e742d\
                    76617269616e742d636170733a6e6f726d616c3b666f6e742d7661726961\
                    6e742d6e756d657269633a6e6f726d616c3b666f6e742d76617269616e74\
                    2d656173742d617369616e3a6e6f726d616c220a20202020202020783d22\
                    312e35383734383934220a20202020202020793d22392e38373539343332\
                    220a2020202020202069643d2274657874343039220a2020202020202074\
                    72616e73666f726d3d227363616c6528312e303832303831332c302e3932\
                    34313434393729223e3c747370616e0a202020202020202020736f646970\
                    6f64693a726f6c653d226c696e65220a2020202020202020207374796c65\
                    3d2266696c6c3a233030303030303b7374726f6b652d77696474683a302e\
                    3736323231363b2d696e6b73636170652d666f6e742d7370656369666963\
                    6174696f6e3a2773657269667878782c204e6f726d616c273b666f6e742d\
                    66616d696c793a73657269667878783b666f6e742d7765696768743a6e6f\
                    726d616c3b666f6e742d7374796c653a6e6f726d616c3b666f6e742d7374\
                    72657463683a6e6f726d616c3b666f6e742d76617269616e743a6e6f726d\
                    616c3b666f6e742d73697a653a392e313436353939333770783b666f6e74\
                    2d76617269616e742d6c69676174757265733a6e6f726d616c3b666f6e74\
                    2d76617269616e742d636170733a6e6f726d616c3b666f6e742d76617269\
                    616e742d6e756d657269633a6e6f726d616c3b666f6e742d76617269616e\
                    742d656173742d617369616e3a6e6f726d616c220a202020202020202020\
                    783d22312e35383734383934220a202020202020202020793d22392e3837\
                    3539343332220a20202020202020202069643d22747370616e323730223e\
                    5356472053616d706c653c2f747370616e3e3c2f746578743e0a20202020\
                    3c746578740a20202020202020786d6c3a73706163653d22707265736572\
                    7665220a202020202020207472616e73666f726d3d227363616c6528302e\
                    323634353833333329220a2020202020202069643d227465787432373222\
                    0a202020202020207374796c653d2277686974652d73706163653a707265\
                    3b73686170652d696e736964653a75726c282372656374323734293b6469\
                    73706c61793a696e6c696e653b66696c6c3a2330303030303022202f3e0a\
                    20203c2f673e0a3c2f7376673e0a";
    let tempfile = create_tempfile(hex)?;
    let mut global_context = GlobalContext::default();
    let image_info = ImageInfo::load(tempfile.path().to_path_buf(), true, &mut global_context)?;
    assert_eq!(image_info.metadata.width, 300);
    assert_eq!(image_info.metadata.height, 100);
    assert_eq!(image_info.metadata.format, "svg");
    assert_eq!(image_info.metadata.pixel_format, None);
    assert!(image_info.metadata.exif.is_none());
    assert_eq!(image_info.ocr, Some("SVG Sample".to_string()));
    Ok(())
}
