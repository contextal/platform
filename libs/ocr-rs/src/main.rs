//! Tesseract OCR backend
use anyhow::Context;
use backend_utils::objects::{BackendRequest, BackendResultKind, BackendResultOk};
use config::Config;
use image::{GenericImageView, ImageError};
use ocr_rs::{Dpi, RawApiError, TessBaseApi, TessPageSegMode};
use serde::Serialize;
use std::{
    cell::RefCell,
    fs::File,
    io::{BufReader, ErrorKind},
    path::PathBuf,
};
use tracing::trace;
use tracing_subscriber::prelude::*;

mod config;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::new().context("failed to construct a config")?;

    backend_utils::work_loop!(None, None, |request| { process_request(request, &config) })?;

    Ok(())
}

/// A structure to hold metadata attributes of an Image
#[derive(Debug, Serialize)]
struct ImageMetadata {
    /// Image width in pixels
    image_width: u32,
    /// Image height in pixels
    image_height: u32,
    /// Recognized text
    text: String,
    /// Apparent image format
    image_format: String,
}

/// Perform image text recognition by means of Tesseract OCR.
///
/// Image format support is provided by the [`image`](https://crates.io/crates/image) crate.
/// Supported formats are: `png`, `jpeg`, `gif`, `webp`, `pnm` (`pbm`, `pam`, `ppm`, `pgm`),
/// `tiff`, `tga`, `dds`, `bmp`, `ico`, `hdr`, `openexr`, `farbfeld` and `qoi`.
///
/// TIFF decoder from `image` crate supports baseline TIFF (no fax support), LZW and PackBits.
///
/// # Returns #
/// * Err for transient retriable errors (I/O, memory allocation, etc)
/// * Ok(BackendResultKind::ok) for success
/// * Ok(BackendResultKind::error) for permanent errors (e.g. unsupported image format, invalid
/// image file)
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> anyhow::Result<BackendResultKind> {
    thread_local! {
        // A placeholder to keep initialized API handler between function invocations.
        static STATE_CELL: RefCell<Option<TessBaseApi>> = RefCell::new(None);
    }

    let input_path = PathBuf::from(&config.objects_path).join(&request.object.object_id);
    let file = File::open(input_path).context("failed to open an input file")?;
    let file_size = file
        .metadata()
        .context("failed to read an input file metadata")?
        .len();
    if file_size > config.max_input_size_bytes as _ {
        return Ok(BackendResultKind::error(format!(
            "image file size ({file_size}) exceeds the limit ({})",
            config.max_input_size_bytes
        )));
    }

    let image_reader = image::ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .context("failed to read an image signature")?;
    let image_format = match image_reader.format() {
        Some(format) => format!("{format:?}").to_lowercase(),
        None => {
            return Ok(BackendResultKind::error(
                "unsupported/unrecognized image format".to_string(),
            ))
        }
    };
    let image = match image_reader.decode() {
        Ok(image) => {
            let (width, height) = image.dimensions();
            if width.saturating_mul(height) > config.max_input_size_pixels {
                return Ok(BackendResultKind::error(format!(
                    "image pixel count ({width} x {height}) exceeds the limit ({})",
                    config.max_input_size_pixels
                )));
            }
            image
        }
        Err(e) => {
            return match e {
                ImageError::IoError(e) => match e.kind() {
                    ErrorKind::InvalidInput | ErrorKind::InvalidData | ErrorKind::UnexpectedEof => {
                        Ok(BackendResultKind::error(format!(
                            "invalid image file: {e:?}"
                        )))
                    }
                    ErrorKind::Unsupported => Ok(BackendResultKind::error(format!(
                        "unsupported image format: {e:?}"
                    ))),
                    _ => Err(e).context("failed to decode an image"),
                },
                _ => Ok(BackendResultKind::error(format!(
                    "failed to decode an image: {e:?}"
                ))),
            };
        }
    }
    .into_rgba8();

    // Initialize (or re-initialize) Tesseract API if necessary
    STATE_CELL
        .with(|option| {
            if option.borrow().is_none() {
                *option.borrow_mut() = Some(TessBaseApi::new(
                    "eng",
                    TessPageSegMode::PSM_AUTO,
                    Some(Dpi(150)),
                )?);
                trace!("successfully initialized Tesseract API");
            }
            Ok::<_, RawApiError>(())
        })
        .context("failed to initialize Tesseract API")?;

    let text = match STATE_CELL.with(|option| {
        let borrow = option.borrow();
        let api = match borrow.as_ref() {
            Some(v) => v,
            None => unreachable!(),
        };
        api.set_rgba_image(&image)?;
        api.recognize()?;
        api.get_text()
    }) {
        Ok(v) => v,
        e @ Err(RawApiError::RecognitionFailed) => {
            trace!("dropping Tesseract API handler for reinit on the next request");
            STATE_CELL.with(|option| *option.borrow_mut() = None);
            e?
        }
        Err(e) => return Ok(BackendResultKind::error(e.to_string())),
    };

    let metadata = ImageMetadata {
        image_format,
        image_width: image.height(),
        image_height: image.width(),
        text,
    };

    let result = BackendResultKind::ok(BackendResultOk {
        symbols: vec![],
        object_metadata: match serde_json::to_value(metadata)
            .context("failed to serialize ImageMetadata")?
        {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children: vec![],
    });

    Ok(result)
}

#[cfg(test)]
mod test;
