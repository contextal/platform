use image::{ImageBuffer, Rgba};
use std::{
    ffi::{CStr, CString},
    ptr,
};
use thiserror::Error;

#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
#[allow(clippy::upper_case_acronyms)]
mod raw {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

/// Publicly re-export modes for page layout analysis.
pub use raw::TessPageSegMode;

#[derive(Error, Debug)]
pub enum RawApiError {
    /// An error variant to signal to create/allocate Tesseract API instance
    #[error("failed to create an API instance")]
    CreatedApiIsNull,

    /// An error variant to signal inability to initialize Tesseract API
    #[error("failed to initialize an API instance")]
    ApiInitFailed,

    /// An error variant to signal presence of '\0' character in a string which is a subject of
    /// conversion to null-terminated string. A wrapper for
    /// [`std::ffi::NulError`](https://doc.rust-lang.org/std/ffi/struct.NulError.html)
    #[error("an input string contains a nul byte")]
    NulError(#[from] std::ffi::NulError),

    /// An error variant to signal an failed attempt to set a variable
    #[error("failed to set a variable '{key}' to value '{value}'")]
    SetVariableFailed { key: String, value: String },

    /// An error variant to signal a failure during a text recognition
    #[error("failed to recognize an image")]
    RecognitionFailed,

    /// Wrapper for [`std::str::Utf8Error`](https://doc.rust-lang.org/std/str/struct.Utf8Error.html)
    #[error("invalid UTF-8 byte sequence")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// Wrapper for
    /// [`std::num::TryFromIntError`](https://doc.rust-lang.org/std/num/struct.TryFromIntError.html)
    #[error("lossless integer conversion has failed")]
    TryFromIntError(#[from] std::num::TryFromIntError),

    /// An error variant to signal an integer overflow
    #[error("mathematical operation causes integer overflow")]
    IntegerOverflow,

    /// Provided image has zero area (i.e. at least one of image dimensions is 0 pixels in size).
    #[error("image has zero area")]
    ZeroAreaImage,

    /// Confidence reported by Tesseract is out of expected bounds.
    #[error("unexpected confidence value: {0}")]
    UnexpectedConfidenceValue(i32),
}

pub struct Dpi(pub u16);

pub struct TessBaseApi {
    inner: *mut raw::TessBaseAPI,
}

impl TessBaseApi {
    /// Composite function to allocate new Tesseract API instance, initialize it with specified
    /// language (languages), switch to specified page segmentation mode and use optionally
    /// provided image DPI.
    ///
    /// Performing each step manually could give you higher granularity if anything goes wrong.
    pub fn new(
        lang: &str,
        page_segmentation_mode: TessPageSegMode,
        dpi: Option<Dpi>,
    ) -> Result<Self, RawApiError> {
        let api = Self::create()?;
        api.init(lang)?;
        api.set_page_segmentation_mode(page_segmentation_mode);
        if let Some(dpi) = dpi {
            api.set_variable("user_defined_dpi", &dpi.0.to_string())?;
        }

        Ok(api)
    }

    /// Creates a new [`TessBaseApi`] instance.
    pub fn create() -> Result<Self, RawApiError> {
        let api = unsafe { raw::TessBaseAPICreate() };
        if api.is_null() {
            return Err(RawApiError::CreatedApiIsNull);
        }

        Ok(TessBaseApi { inner: api })
    }

    /// Initializes [`TessBaseApi`] with a specified language string.
    ///
    /// The language is an `ISO 639-3` string, like "eng" for English.
    /// The language may be a string of the form [~]<lang>[+[~]<lang>]* indicating that multiple
    /// languages are to be loaded. E.g. "hin+eng" will load Hindi and English.
    ///
    /// It is considered safe to call `init` multiple times on the same instance to change
    /// language.
    pub fn init(&self, lang: &str) -> Result<(), RawApiError> {
        let c_lang = CString::new(lang)?;

        match unsafe { raw::TessBaseAPIInit3(self.inner, ptr::null(), c_lang.as_ptr()) } {
            0 => Ok(()),
            _ => Err(RawApiError::ApiInitFailed),
        }
    }

    /// Sets a mode for page layout analysis.
    ///
    /// The following modes are available:
    /// PSM_OSD_ONLY: Orientation and script detection only.
    /// PSM_AUTO_OSD: Automatic page segmentation with orientation and script detection (OSD).
    /// PSM_AUTO_ONLY: Automatic page segmentation, but no OSD, or OCR.
    /// PSM_AUTO: Fully automatic page segmentation, but no OSD.
    /// PSM_SINGLE_COLUMN: Assume a single column of text of variable sizes.
    /// PSM_SINGLE_BLOCK_VERT_TEXT: Assume a single uniform block of vertically aligned text.
    /// PSM_SINGLE_BLOCK: Assume a single uniform block of text. [Default mode]
    /// PSM_SINGLE_LINE: Treat the image as a single text line.
    /// PSM_SINGLE_WORD: Treat the image as a single word.
    /// PSM_CIRCLE_WORD: Treat the image as a single word in a circle.
    /// PSM_SINGLE_CHAR: Treat the image as a single character.
    /// PSM_SPARSE_TEXT: Find as much text as possible in no particular order.
    /// PSM_SPARSE_TEXT_OSD: Sparse text with orientation and script detection.
    /// PSM_RAW_LINE: Treat the image as a single text line, bypassing hacks that are Tesseract-specific.
    pub fn set_page_segmentation_mode(&self, mode: raw::TessPageSegMode) {
        unsafe { raw::TessBaseAPISetPageSegMode(self.inner, mode) };
    }

    /// Sets a variable to influence Tesseract behaviour.
    ///
    /// Use `tesseract --print-parameters` command to see all the possible variables/parameters and
    /// their default values.
    pub fn set_variable(&self, key: &str, value: &str) -> Result<(), RawApiError> {
        let c_key = CString::new(key)?;
        let c_value = CString::new(value)?;

        match unsafe { raw::TessBaseAPISetVariable(self.inner, c_key.as_ptr(), c_value.as_ptr()) } {
            1 => Ok(()),
            _ => Err(RawApiError::SetVariableFailed {
                key: key.into(),
                value: value.into(),
            }),
        }
    }

    /// Specify an RGBA image for further recognition.
    pub fn set_rgba_image(&self, img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<(), RawApiError> {
        // Tesseract/Leptonica won't tolerate an image of zero area, so it is better to provide a
        // meaningful error while it is still possible.
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return Err(RawApiError::ZeroAreaImage);
        }

        const RGBA_BYTES_PER_PIXEL: u8 = 4;
        unsafe {
            raw::TessBaseAPISetImage(
                self.inner,
                img.as_ptr(),
                img.width().try_into()?,
                img.height().try_into()?,
                RGBA_BYTES_PER_PIXEL.into(),
                img.width()
                    .checked_mul(RGBA_BYTES_PER_PIXEL.into())
                    .ok_or(RawApiError::IntegerOverflow)?
                    .try_into()?,
            )
        };

        Ok(())
    }

    /// Perform a text recognition on an image, previously specified via [`set_rgba_image`].
    ///
    /// It is not necessary to call this function explicitly, and text recognition process would be
    /// performed implicitly on attempt to get recognition results. But if anything goes wrong it
    /// might be helpful to know which exact step has failed with higher granularity.
    pub fn recognize(&self) -> Result<(), RawApiError> {
        match unsafe { raw::TessBaseAPIRecognize(self.inner, ptr::null_mut()) } {
            0 => Ok(()),
            _ => Err(RawApiError::RecognitionFailed),
        }
    }

    /// Returns a recognized text.
    ///
    /// If the text has not been recognized yet via explicit call to [`recognize`], this will
    /// trigger recognition implicitly.
    pub fn get_text(&self) -> Result<String, RawApiError> {
        let c_text = unsafe { raw::TessBaseAPIGetUTF8Text(self.inner) };
        let text = unsafe { CStr::from_ptr(c_text) }.to_str()?;
        let text = String::from(text.trim_end_matches('\n'));
        unsafe { raw::TessDeleteText(c_text) };

        Ok(text)
    }

    /// Returns a vector of all word confidences. Confidence is a value between 0 and 100.
    ///
    /// The number of confidences should correspond to the number of space-delimited words in
    /// `get_text` result.
    pub fn get_all_word_confidences(&self) -> Result<Vec<u8>, RawApiError> {
        let array_ptr = unsafe { raw::TessBaseAPIAllWordConfidences(self.inner) };

        let confidences = (0..)
            .map(|i| match unsafe { array_ptr.add(i).read() } {
                v @ 0..=100 => Ok(Some(v as _)),
                -1 => Ok(None), // array of confidences is terminated by `-1`
                v => Err(RawApiError::UnexpectedConfidenceValue(v)),
            })
            .take_while(|v| matches!(v, Ok(Some(_))))
            .map(|v| match v {
                Ok(Some(v)) => Ok(v),
                Ok(None) => unreachable!(),
                Err(e) => Err(e),
            })
            .collect();

        unsafe { raw::TessDeleteIntArray(array_ptr) };

        confidences
    }
}

impl Drop for TessBaseApi {
    fn drop(&mut self) {
        unsafe {
            raw::TessBaseAPIEnd(self.inner);
            raw::TessBaseAPIDelete(self.inner);
        };
    }
}
