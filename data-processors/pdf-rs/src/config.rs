//! Facilities for reading runtime configuration values
use crate::PdfBackendError;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use image::ImageFormat;
use serde::{Deserialize, Deserializer};
use std::ops::Deref;
use tracing::warn;

/// Worker backend configuration.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// The path to the objects store.
    pub objects_path: String,

    /// Output path.
    pub output_path: String,

    /// Maximum allowed PDF document size in bytes.
    pub max_processed_size: u64,

    /// Maximum number of objects to process.
    pub max_objects: u32,

    /// Whether to produce rendered document pages as child objects.
    pub render_pages: bool,

    /// Optional width of rendered document page in pixels.
    pub render_page_width: Option<i32>,

    /// Optional height of rendered document page in pixels.
    pub render_page_height: Option<i32>,

    /// Whether to save objects of Image type as child objects.
    pub save_image_objects: bool,

    /// Maximum traversal depth for container objects, which could have another containers inside.
    pub max_object_depth: u8,

    /// Maximum number of pages to process.
    pub max_pages: u32,

    /// Maximum number of bookmarks to process.
    pub max_bookmarks: u32,

    /// In which cases optical character recognition should be performed. This could be `"Never"`,
    /// `"IfNoDocumentTextAvailable"` and `"Always"`.
    pub ocr_mode: OcrMode,

    /// Output format to save image objects and rendered document pages. This could be `"png"`,
    /// `"jpg"` (or `"jpeg"`), `"webp"`, `"tiff"` and `"bmp"`.
    pub output_image_format: OutputImageFormat,

    /// Whether to use totally random file names for child objects (default), or add suffixes,
    /// indexes and prefixes to produced filenames to make debugging/development more convenient.
    #[serde(default = "Config::default_random_filenames")]
    pub random_filenames: bool,

    /// Maximum number of annotations to process.
    pub max_annotations: u32,

    /// Maximum number of attachments to process.
    pub max_attachments: u16,

    /// Maximum size of attachment to extract.
    pub max_attachment_size: usize,

    /// Maximum number of fonts to process on each page.
    pub max_fonts_per_page: u16,

    /// Maximum number of cryptographic signatures to process.
    pub max_signatures: u16,

    /// Maximum signature size to extract.
    pub max_signature_size: usize,

    /// Maximum number of links to process.
    pub max_links: u32,

    /// Maximum number of children objects to create (processing halts if reached)
    pub max_children: usize,

    /// Single object limit (the part is skipped if size is exceeded)
    pub max_child_output_size: u64,
}

impl Config {
    /// Constructs `Config` from a `toml` file and environment variables
    pub fn new() -> Result<Self, PdfBackendError> {
        let config: Self = Figment::new()
            .merge(Toml::file("backend.toml"))
            .merge(Env::prefixed("BACKEND__").split("__"))
            .extract()?;

        macro_rules! disallow_max_value_of_type {
            ($type:ty; $parent:ident.$var:ident) => {
                if $parent.$var == <$type>::MAX {
                    Err(PdfBackendError::ConfigParameterValue {
                        parameter: stringify!($var).into(),
                        message: format!(
                            "value is too large (must be strictly below {})",
                            <$type>::MAX
                        )
                        .into(),
                    })?
                }
            };
        }

        disallow_max_value_of_type!(u64; config.max_processed_size);
        disallow_max_value_of_type!(usize; config.max_signature_size);
        disallow_max_value_of_type!(usize; config.max_attachment_size);

        if let Some(width) = config.render_page_width {
            if width <= 0 {
                return Err(PdfBackendError::ConfigParameterValue {
                    parameter: "render_page_width".into(),
                    message: "value should be positive".into(),
                });
            }
        }
        if let Some(height) = config.render_page_height {
            if height <= 0 {
                return Err(PdfBackendError::ConfigParameterValue {
                    parameter: "render_page_height".into(),
                    message: "value should be positive".into(),
                });
            }
        }

        Ok(config)
    }

    /// Returns a default value for `random_filenames` parameter
    fn default_random_filenames() -> bool {
        true
    }
}

/// A wrapper for deserialization of `ImageFormat`
#[derive(Debug)]
pub struct OutputImageFormat(ImageFormat);

impl Deref for OutputImageFormat {
    type Target = ImageFormat;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// When to perform optical character recognition.
#[derive(Debug, Deserialize, PartialEq)]
pub enum OcrMode {
    /// OCR should never be performed.
    Never,

    /// OCR should be performed only if there is no text already present in a document.
    IfNoDocumentTextAvailable,

    /// OCR should always be performed.
    Always,
}

impl<'de> Deserialize<'de> for OutputImageFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let original_string = String::deserialize(deserializer)?;
        let inner = match original_string.to_lowercase().as_str() {
            "png" => ImageFormat::Png,
            "jpeg" | "jpg" => ImageFormat::Jpeg,
            "webp" => ImageFormat::WebP,
            "tiff" => ImageFormat::Tiff,
            "bmp" => ImageFormat::Bmp,
            _ => {
                warn!(
                    "Unrecognized image format {original_string:?}, \
                    fallback to ImageFormat::Png"
                );
                ImageFormat::Png
            }
        };
        Ok(OutputImageFormat(inner))
    }
}
