use config::Config;
use libc::{setlocale, LC_ALL};
use nt_time::{error::OffsetDateTimeRangeError, FileTime};
use raw::RARHeaderDataEx;
use scopeguard::ScopeGuard;
use serde::{ser::Error, Serialize, Serializer};
use std::{
    cell::Cell,
    ffi::{c_char, c_int, c_long, c_uint, OsString},
    fmt::Write,
    fs::Permissions,
    ops::Div,
    os::raw::c_void,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    ptr,
    rc::{Rc, Weak},
    slice,
    sync::Once,
    time::Duration,
};
use thiserror::Error;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time};
use tracing::{error, info, trace, warn};
use widestring::{U32CString, WideCString};

#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
#[allow(clippy::upper_case_acronyms)]
mod raw {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
pub mod config;

#[derive(Error, Debug)]
pub enum RarBackendError {
    /// OsString contains invalid UTF-8 byte sequence.
    #[error("invalid UTF-8 byte sequence: {0:?}")]
    Utf8(OsString),

    /// Configuration parameter value is out of bounds.
    #[error("config parameter `{parameter}` value is out of bounds: {message}")]
    ConfigParameterValue {
        parameter: &'static str,
        message: String,
    },

    /// Wrapper for [`Figment::Error`](https://docs.rs/figment/latest/figment/struct.Error.html)
    #[error("config deserialization: {0:?}")]
    ConfigDeserialization(#[from] figment::Error),

    /// Wrapper for
    /// [`serde_json::Error`](https://docs.rs/serde_json/latest/serde_json/struct.Error.html)
    #[error("json serialization/deserialization: {0:?}")]
    SerdeJson(#[from] serde_json::Error),

    /// Wrapper for
    /// [`tempfile::PathPersistError`](https://docs.rs/tempfile/latest/tempfile/struct.PathPersistError.html)
    #[error("failed to persist a temporary file: {0}")]
    PathPersist(#[from] tempfile::PathPersistError),

    /// Wrapper for [`std::io::Error`](https://doc.rust-lang.org/std/io/struct.Error.html)
    #[error("IO error: {0:?}")]
    IO(#[from] std::io::Error),

    /// Provided path contains Nul symbol and it can't be converted to WideCString.
    #[error("file path contains Nul symbol and it can't be converted to wide string")]
    PathContainsNul {
        source: widestring::error::ContainsNul<u32>,
    },

    /// Provided password contains Nul symbol and it can't be converted to WideCString.
    #[error("password contains Nul symbol and it can't be converted to wide string")]
    PasswordContainsNul {
        source: widestring::error::ContainsNul<u32>,
    },

    /// Out of memory.
    #[error("out of memory")]
    OutOfMemory,

    /// Error classified as "unknown" by `libunrar`.
    ///
    /// This error code is used by `libunrar` when the extraction is interrupted from a callback
    /// function.
    #[error("Unknown error (interruption from a callback?) during extraction of {entry:?}")]
    UnknownWhenExtracting {
        entry: String,
        extracted_so_far: u64,
    },

    /// Broken archive header.
    #[error("broken archive header")]
    ArchiveHeaderBroken,

    /// Broken entry header.
    #[error("broken entry header")]
    EntryHeaderBroken,

    /// File is not a RAR archive.
    #[error("file is not a RAR archive")]
    NotRarArchive,

    /// Unknown encryption algorithm of archive headers.
    #[error("unknown encryption algorithm of archive headers")]
    ArchiveHeadersEncryptionUnknown,

    /// File open error.
    #[error("failed to open a file: {0:?}")]
    FileOpen(PathBuf),

    /// Invalid password (returned only for archives in RAR 5.0 format).
    #[error("invalid password")]
    InvalidPassword,

    /// Missing password.
    #[error("missing password")]
    MissingPassword,

    /// Archive comment doesn't fit into provided buffer.
    #[error("archive comment truncated")]
    ArchiveCommentTruncated(String),

    /// Broken archive comment.
    #[error("broken archive comment")]
    ArchiveCommentBroken,

    /// Unexpected result code while opening an archive.
    #[error("unexpected result code while attempting to open an archive: {0}")]
    UnexpectedResultCodeOpenArchive(u32),

    /// Unexpected result code while accessing archive comment.
    #[error("unexpected result code while accessing archive comment: {0}")]
    UnexpectedResultCodeArchiveComment(u32),

    /// Unexpected result code while skipping a data block.
    #[error("unexpected result code while skipping a data block: {0}")]
    UnexpectedResultCodeSkipData(u32),

    /// Unexpected result code while reading archive entry header.
    #[error("unexpected result code while reading archive entry header: {0}")]
    UnexpectedResultCodeReadEntryHeader(u32),

    /// Unexpected result code while extracting archive entry.
    #[error("unexpected result code while extracting archive entry: {0}")]
    UnexpectedResultCodeExtractEntry(u32),

    /// Wrapper for
    /// [`widestring::error::Utf32Error`](https://docs.rs/widestring/latest/widestring/error/struct.Utf32Error.html)
    #[error("invalid UTF-32 input")]
    Utf32Error(#[from] widestring::error::Utf32Error),

    /// Read error.
    #[error("read error")]
    Read,

    /// Failed to read a file.
    #[error("read failed while extracting {entry:?}")]
    ReadWhileExtracting { entry: String },

    /// Volume open error.
    #[error("failed to open a next volume")]
    VolumeOpen,

    /// Volume open error while extracting an entry.
    #[error("failed to open a next volume while extracting {entry:?}")]
    VolumeOpenWhileExtracting { entry: String },

    /// Checksum mismatch.
    #[error("checksum mismatch")]
    ChecksumMismatch,

    /// Checksum mismatch while extracting an entry.
    #[error("checksum mismatch while extracting {entry:?}")]
    ChecksumMismatchWhileExtracting { entry: String },

    /// Archive handler has been taken.
    /// This could happen on attempt to extract the same archive entry more than once.
    #[error("archive handler has been taken")]
    ArchiveHandlerHasBeenTaken,

    /// Archive handler reference is no longer upgradable.
    ///
    /// This could happen if `ArchiveIterator` has "moved on" to the next archive entry, or if it
    /// has been dropped.
    #[error("archive handler is no longer accessible")]
    ArchiveHandlerIsNoLongerAccessible,

    /// Archive handler is not accessible via iterator's shared reference.
    ///
    /// This should never happen.
    #[error("archive handler is not accessible via iterator's shared reference")]
    ArchiveHandlerIsNotAccessibleFromIterator,

    /// Entry cursor is in a wrong state.
    ///
    /// I.e. in before-header-state, when before-data-state is expected, or vice-versa.
    #[error("entry cursor is in a wrong state")]
    EntryCursorState,

    /// Unknown archive format while extracting an entry.
    #[error("unknown archive format while extracting {entry:?}")]
    UnknownArchiveFormat { entry: String },

    /// Failed to create a file.
    #[error("failed to create a file {destination:?} while extracting {entry:?}")]
    Create { entry: String, destination: PathBuf },

    /// Failed to close a file.
    #[error("failed to close a file {destination:?} while extracting {entry:?}")]
    Close { entry: String, destination: PathBuf },

    /// Write failed.
    #[error("failed to write a file {destination:?} while extracting {entry:?}")]
    Write { entry: String, destination: PathBuf },

    /// Referenced file is not available.
    #[error("referenced file is not available while extracting {entry:?} to file {destination:?}")]
    ReferenceIsNotAvailable { entry: String, destination: PathBuf },
}

/// An entity representing an archive. It is created when an archive is opened, and it is used to
/// perform various operations on the archive. And to hold the data supplied-to and obtained-from
/// `libunrar`.
#[derive(Debug)]
pub struct ArchiveHandler {
    /// Opaque `libunrar` archive handler. It is used as an argument in most of `libunrar` function
    /// calls.
    opaque: *mut c_void,

    /// Pointer to a data which we might need to process callback requests from `libunrar` (like
    /// password for decryption, and limits and counters to make decision whether to interrupt an
    /// extraction in the middle of entry extraction process).
    callback_data: *mut CallbackData,

    /// A structure to pass arguments to `libunrar` when opening an archive, and also to hold the
    /// archive attributes received from `libunrar` when archive has been opened.
    attributes: Box<raw::RAROpenArchiveDataEx>,

    /// Represents current state of an archive.
    /// Which is either before next entry header block, or before next data block.
    /// It allows us to know when data block has not been consumed (extracted) and has to be
    /// explicitly skipped.
    state: Cell<EntryCursorState>,

    /// Archive comment in wide-string form.
    comment: Vec<u32>,
}

/// Container for data which is necessary to process callback calls from `libunrar`.
///
/// First we provide `libunrar` with the data when we open an archive, and then `libunrar` makes it
/// accessible to us via one of callback function arguments.
#[derive(Debug)]
pub struct CallbackData {
    /// Wide-string representation of a password, if it has been provided.
    /// The value is supplied to `libunrar` when necessary (decryption of archive headers,
    /// decryption of encrypted entries).
    password: Option<U32CString>,

    /// Max decompressed entry size.
    max_decompressed_entry_size: u64,

    /// How much data has been extracted so far for a current archive entry.
    decompressed_entry_size_so_far: u64,
}

/// An entity which represents current kind-of-location within the archive.
/// It could be either before-entry-header-block-or-end-of-archive and before-entry-data-block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EntryCursorState {
    /// Before archive entry header block (or end of archive).
    BeforeHeader,

    /// Before archive entry data block.
    BeforeData,
}

/// The size of a pre-allocated buffer which is passed to `libunrar` to copy archive comment into
/// it. If the buffer is not big enough the comment will be truncated.
const MAX_COMMENT_BUFFER_SIZE_CHARS: usize = 16384;

/// The size of a pre-allocated buffer which is passed to `libunrar` to copy "redirection
/// destination" into it. Only unusual kinds of files, like Unix and Windows symlinks, hardlinks,
/// Windows junctions, etc have "redirection destination" value.
const MAX_REDIR_BUFFER_SIZE_CHARS: usize = 2048;

/// Synchronization primitive used to initialize locale.
static LOCALE: Once = Once::new();

impl ArchiveHandler {
    /// Attempts to open an archive. In case of success it provides archive handler necessary to
    /// perform all the other operations with the archive.
    pub fn new(
        path: &Path,
        password: Option<&str>,
        config: &Config,
    ) -> Result<ArchiveHandler, RarBackendError> {
        // Setting UTF-8 localte is necessary to make wide-character/Unicode conversion libc
        // functions to work.
        // These functions are used by `libunrar` under the hood.
        LOCALE.call_once(|| {
            if unsafe { setlocale(LC_ALL, b"en_US.UTF-8\0".as_ptr() as *const c_char) }.is_null() {
                panic!("failed to set a locale");
            }
        });

        let wide_path = WideCString::from_os_str(path)
            .map_err(|source| RarBackendError::PathContainsNul { source })?;

        let password = match &password {
            Some(v) => Some(
                WideCString::from_str(v)
                    .map_err(|source| RarBackendError::PasswordContainsNul { source })?,
            ),
            None => None,
        };
        let callback_data = scopeguard::guard(
            Box::into_raw(Box::new(CallbackData {
                password,
                max_decompressed_entry_size: config.max_child_output_size,
                decompressed_entry_size_so_far: 0,
            })),
            |callback_data_ptr| unsafe {
                let _ = Box::from_raw(callback_data_ptr);
            },
        );

        let mut comment_buffer = vec![0u32; MAX_COMMENT_BUFFER_SIZE_CHARS];

        let mut archive_data = raw::RAROpenArchiveDataEx {
            ArcNameW: wide_path.as_ptr() as _,
            CmtBufW: comment_buffer.as_ptr() as _,
            CmtBufSize: comment_buffer.len() as _,
            OpenMode: raw::RAR_OM_EXTRACT,
            OpFlags: raw::ROADOF_KEEPBROKEN,
            Callback: Some(Self::callback),
            UserData: *callback_data as _,
            ..Default::default()
        };

        let opaque = unsafe { raw::RAROpenArchiveEx(&mut archive_data as _) };
        match archive_data.OpenResult {
            raw::ERAR_SUCCESS => {
                trace!("open archive operation succeeded");
                Ok(())
            }
            raw::ERAR_NO_MEMORY => Err(RarBackendError::OutOfMemory),
            raw::ERAR_BAD_ARCHIVE => Err(RarBackendError::NotRarArchive),
            raw::ERAR_BAD_DATA => Err(RarBackendError::ArchiveHeaderBroken),
            raw::ERAR_UNKNOWN_FORMAT => Err(RarBackendError::ArchiveHeadersEncryptionUnknown),
            raw::ERAR_EOPEN => Err(RarBackendError::FileOpen(path.to_path_buf())),
            raw::ERAR_MISSING_PASSWORD => Err(RarBackendError::MissingPassword),
            raw::ERAR_BAD_PASSWORD => Err(RarBackendError::InvalidPassword),
            other => Err(RarBackendError::UnexpectedResultCodeOpenArchive(other)),
        }?;

        comment_buffer.truncate(archive_data.CmtSize as usize);
        comment_buffer.shrink_to_fit();

        Ok(ArchiveHandler {
            opaque,
            state: Cell::new(EntryCursorState::BeforeHeader),
            attributes: Box::new(archive_data),
            comment: comment_buffer,
            callback_data: ScopeGuard::into_inner(callback_data), // disarms ScopeGuard
        })
    }

    /// Callback function invoked by `libunrar` on various occasions (see `CallbackMessage`).
    ///
    /// The `msg` argument represents a reason of invocation, `user_data` is the pointer we
    /// supplied to `libunrar` when we were opening the archive. And the rest of parameters change
    /// their meaning depending on the message.
    #[no_mangle]
    pub extern "C" fn callback(
        msg: c_uint,
        user_data: c_long,
        param1: c_long,
        param2: c_long,
    ) -> c_int {
        let message: CallbackMessage = msg.into();
        match message {
            CallbackMessage::NeedPasswordWide => {
                trace!("callback message `{message:?}`");
                let callback_data = scopeguard::guard(
                    unsafe { Box::from_raw(user_data as *mut CallbackData) },
                    |boxed_callback_data| {
                        Box::into_raw(boxed_callback_data);
                    },
                );

                if let Some(ref password) = callback_data.password {
                    let buffer_ptr = param1 as *mut u32;
                    let buffer_size_chars = param2 as usize;
                    let password_with_nul = password.as_slice_with_nul();

                    if password_with_nul.len() <= buffer_size_chars {
                        let buffer: &mut [u32] = unsafe {
                            slice::from_raw_parts_mut(buffer_ptr.cast(), buffer_size_chars)
                        };

                        buffer[..(password_with_nul.len())].copy_from_slice(password_with_nul);
                        trace!("provided a password to `libunrar` via callback");

                        1
                    } else {
                        warn!(
                            "can't supply `libunrar` with a password, as it is longer ({}) than the \
                            buffer to hold it ({buffer_size_chars}) => canceling the operation",
                            password_with_nul.len()
                        );

                        -1
                    }
                } else {
                    trace!("no password available to provide => canceling the operation");

                    -1
                }
            }
            CallbackMessage::NeedPassword => {
                trace!("callback message `{message:?}`: skipping");

                0
            }
            CallbackMessage::ChangeVolume | CallbackMessage::ChangeVolumeWide => {
                trace!("callback message `{message:?}`: canceling the operation");

                -1
            }
            CallbackMessage::ProcessData => {
                let mut callback_data = scopeguard::guard(
                    unsafe { Box::from_raw(user_data as *mut CallbackData) },
                    |boxed_callback_data| {
                        Box::into_raw(boxed_callback_data);
                    },
                );
                let decompressed_block_size = param2 as u64;
                callback_data.decompressed_entry_size_so_far = callback_data
                    .decompressed_entry_size_so_far
                    .saturating_add(decompressed_block_size);

                if callback_data.decompressed_entry_size_so_far
                    > callback_data.max_decompressed_entry_size
                {
                    info!(
                        "callback message `{message:?}`: extracted entry \
                        data ({}) is over the max decompressed entry \
                        size ({}) => interrupting",
                        callback_data.decompressed_entry_size_so_far,
                        callback_data.max_decompressed_entry_size
                    );

                    -1
                } else {
                    trace!("callback message `{message:?}`: within the limit => continuing");

                    0
                }
            }
            CallbackMessage::Unknown(_) => {
                error!("skipping unknown callback message: {message:?}");

                0
            }
        }
    }

    /// Attempts to supply an archive comment, which has been read into the buffer when we were
    /// opening the archive.
    pub fn comment(&self) -> Result<Option<String>, RarBackendError> {
        match self.attributes.CmtState {
            0 => {
                trace!("archive has no comment");
                Ok(None)
            }
            1 => {
                trace!("archive comment extraction succeeded");
                let comment = WideCString::from_vec_truncate(&*self.comment).to_string()?;
                Ok(Some(comment))
            }
            raw::ERAR_SMALL_BUF => {
                let comment = WideCString::from_vec_truncate(&*self.comment).to_string()?;
                Err(RarBackendError::ArchiveCommentTruncated(comment))
            }
            raw::ERAR_NO_MEMORY => Err(RarBackendError::OutOfMemory),
            raw::ERAR_BAD_DATA => Err(RarBackendError::ArchiveCommentBroken),
            other => Err(RarBackendError::UnexpectedResultCodeArchiveComment(other)),
        }
    }

    /// Returns true if archive flags indicate presence of the comment.
    pub fn has_comment(&self) -> bool {
        self.attributes.Flags & raw::ROADF_COMMENT != 0
    }

    /// Returns true if the archive has encrypted headers. So it is not possible to read the
    /// comment and get any information about archive entries without a password.
    ///
    /// RAR archives can have encrypted data without encrypted headers.
    pub fn has_encrypted_headers(&self) -> bool {
        self.attributes.Flags & raw::ROADF_ENCHEADERS != 0
    }

    /// Returns true if the archive has a recovery record.
    pub fn has_recovery_record(&self) -> bool {
        self.attributes.Flags & raw::ROADF_RECOVERY != 0
    }

    /// Returns true if the archive has lock (locked) attribute set.
    pub fn is_locked(&self) -> bool {
        self.attributes.Flags & raw::ROADF_LOCK != 0
    }

    /// Returns true if it is a solid archive.
    pub fn is_solid(&self) -> bool {
        self.attributes.Flags & raw::ROADF_SOLID != 0
    }

    /// Returns true if the archive follows new volume naming scheme. I.e. `volname.partN.rar`.
    pub fn is_new_numbering_scheme(&self) -> bool {
        self.attributes.Flags & raw::ROADF_NEWNUMBERING != 0
    }

    /// Returns true if the archive is a first volume in a multivolume archive (set only by RAR 3.0
    /// and later).
    pub fn is_multivolume_start(&self) -> bool {
        self.attributes.Flags & raw::ROADF_FIRSTVOLUME != 0
    }

    /// Returns true is the archive is a first or any subsequent volume in a multivolume archive.
    pub fn is_multivolume_part(&self) -> bool {
        self.attributes.Flags & (raw::ROADF_FIRSTVOLUME | raw::ROADF_VOLUME) != 0
    }

    /// Returns true if the "authenticity information is present" in the archive (the flag is
    /// obsolete in RAR).
    pub fn is_signed(&self) -> bool {
        self.attributes.Flags & raw::ROADF_SIGNED != 0
    }
}

impl Drop for ArchiveHandler {
    fn drop(&mut self) {
        trace!("dropping an archive handler");
        match unsafe { raw::RARCloseArchive(self.opaque) } as u32 {
            raw::ERAR_SUCCESS => trace!("successfully closed an archive"),
            raw::ERAR_ECLOSE => warn!("generic error while closing an archive"),
            other => error!("unexpected return value when closing an archive: {other}"),
        }

        let _ = unsafe { Box::from_raw(self.callback_data) };
    }
}

impl IntoIterator for ArchiveHandler {
    type Item = Result<EntryHandler, RarBackendError>;

    type IntoIter = ArchiveIterator;

    fn into_iter(self) -> Self::IntoIter {
        let inner = Rc::new(self);
        Self::IntoIter {
            inner,
            shared: Default::default(),
        }
    }
}

/// An iterator to go over RAR archive entries.
#[derive(Debug)]
pub struct ArchiveIterator {
    /// Archive handler.
    inner: Rc<ArchiveHandler>,

    /// Weak reference to archive handler to share with the most recent `EntryHandler` entity
    /// produced via `next()`.
    ///
    /// Archive handler accessibility from `EntryHandler` allows it to perform entry extraction
    /// operation. While (weak) share of weak reference allows us to invalidate the previous share
    /// on each subsequent `next()` invocation. It is when `self.shared` gets recreated.
    shared: Rc<Weak<ArchiveHandler>>,
}

impl ArchiveIterator {
    /// Skips data block if it has not been consumed (extracted). This operation is performed
    /// before iterating to the next archive entry.
    fn skip_unconsumed_data(&mut self) -> Result<(), RarBackendError> {
        match self.inner.state.get() {
            EntryCursorState::BeforeData => {
                match {
                    let result = unsafe {
                        raw::RARProcessFileW(
                            self.inner.opaque,
                            raw::RAR_SKIP as i32,
                            ptr::null_mut(),
                            ptr::null_mut(),
                        )
                    } as u32;
                    self.inner.state.set(EntryCursorState::BeforeHeader);
                    result
                } {
                    raw::ERAR_SUCCESS => {
                        trace!("archive entry data block has been skipped successfully");
                        Ok(())
                    }
                    raw::ERAR_BAD_DATA => Err(RarBackendError::ChecksumMismatch),
                    raw::ERAR_EOPEN => Err(RarBackendError::VolumeOpen),
                    raw::ERAR_EREAD => Err(RarBackendError::Read),
                    raw::ERAR_NO_MEMORY => Err(RarBackendError::OutOfMemory),
                    other => Err(RarBackendError::UnexpectedResultCodeSkipData(other)),
                }
            }
            EntryCursorState::BeforeHeader => Ok(()),
        }
    }
}

/// An entity representing an archive entry. It could be created by `ArchiveIterator` on `next()`
/// invocation. It holds the data supplied-to and obtained-from `libunrar`.
#[derive(Debug)]
#[non_exhaustive]
pub struct EntryHandler {
    /// A structure which holds archive entry attributes provided by `libunrar`.
    header: RARHeaderDataEx,

    /// Single-use archive handler shared by `ArchiveIterator`.
    ///
    /// Its purpose is to allow to perform an entry extraction if requested.
    ///
    /// Weak reference to a (weak) reference becomes un-upgradable as soon as related
    /// `ArchiveIterator` processes to a next archive entry via `next()`.
    inner: Option<Weak<Weak<ArchiveHandler>>>,
}

impl Iterator for ArchiveIterator {
    type Item = Result<EntryHandler, RarBackendError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Err(e) = self.skip_unconsumed_data() {
            match e {
                RarBackendError::ChecksumMismatch => {
                    warn!("checksum mismatch while skipping archive entry data block => ignoring");
                }
                _ => {
                    error!("error while skipping archive entry data block: {e}");
                    return Some(Err(e));
                }
            }
        }

        // Recreate the shared reference to make previous EntryHandler's Weak reference no longer
        // upgradable:
        self.shared = Rc::new(Rc::downgrade(&self.inner));

        let redir_name = scopeguard::guard(
            Box::into_raw(Box::new([0u32; MAX_REDIR_BUFFER_SIZE_CHARS])),
            |name_ptr| unsafe {
                let _ = Box::from_raw(name_ptr);
            },
        );

        let mut header_data = RARHeaderDataEx {
            RedirName: *redir_name as _,
            RedirNameSize: MAX_REDIR_BUFFER_SIZE_CHARS as _,
            ..Default::default()
        };

        match {
            let result =
                unsafe { raw::RARReadHeaderEx(self.inner.opaque, &mut header_data as *mut _) };
            self.inner.state.set(EntryCursorState::BeforeData);
            result as u32
        } {
            raw::ERAR_SUCCESS => {
                trace!("entry header has been read successfully");
                Some(Ok(header_data))
            }
            raw::ERAR_END_ARCHIVE => {
                info!("reached the end of archive on attempt to read next entry header");
                None
            }
            raw::ERAR_BAD_DATA => Some(Err(RarBackendError::EntryHeaderBroken)),
            raw::ERAR_EOPEN => Some(Err(RarBackendError::VolumeOpen)),
            raw::ERAR_MISSING_PASSWORD => Some(Err(RarBackendError::MissingPassword)),
            other => Some(Err(RarBackendError::UnexpectedResultCodeReadEntryHeader(
                other,
            ))),
        }
        .map(|result| {
            result.map(|header| {
                ScopeGuard::into_inner(redir_name); // disarm ScopeGuard, raw pointer will be freed
                                                    // in EntryHandler's Drop implementation

                let mut callback_data =
                    unsafe { Box::from_raw(self.inner.attributes.UserData as *mut CallbackData) };
                callback_data.decompressed_entry_size_so_far = 0;
                Box::into_raw(callback_data);

                EntryHandler {
                    header,
                    inner: Some(Rc::downgrade(&self.shared)),
                }
            })
        })
    }
}

impl EntryHandler {
    /// Attempts to extract archive entry into a specified file path.
    pub fn extract_to_file(&mut self, path: &Path) -> Result<(), RarBackendError> {
        let inner = self
            .inner
            .take()
            .ok_or(RarBackendError::ArchiveHandlerHasBeenTaken)?;

        let mut wide_path = WideCString::from_os_str(path)
            .map_err(|source| RarBackendError::PathContainsNul { source })?;

        let inner = match inner.upgrade() {
            Some(v) => match v.upgrade() {
                Some(v) => v,
                None => Err(RarBackendError::ArchiveHandlerIsNotAccessibleFromIterator)?,
            },
            None => Err(RarBackendError::ArchiveHandlerIsNoLongerAccessible)?,
        };

        if inner.state.get() != EntryCursorState::BeforeData {
            return Err(RarBackendError::EntryCursorState);
        }

        match {
            let result = unsafe {
                raw::RARProcessFileW(
                    inner.opaque,
                    raw::RAR_EXTRACT as _,
                    ptr::null_mut(),
                    wide_path.as_mut_ptr(),
                )
            };
            inner.state.set(EntryCursorState::BeforeHeader);
            result as u32
        } {
            raw::ERAR_SUCCESS => {
                trace!(
                    "archive entry `{}` has been extracted successfully",
                    self.filename()
                );
                std::fs::set_permissions(path, Permissions::from_mode(0o600))?;
                Ok(())
            }
            raw::ERAR_BAD_DATA => Err(RarBackendError::ChecksumMismatchWhileExtracting {
                entry: self.filename(),
            }),
            raw::ERAR_UNKNOWN_FORMAT => Err(RarBackendError::UnknownArchiveFormat {
                entry: self.filename(),
            }),
            raw::ERAR_EOPEN => Err(RarBackendError::VolumeOpenWhileExtracting {
                entry: self.filename(),
            }),
            raw::ERAR_ECREATE => Err(RarBackendError::Create {
                entry: self.filename(),
                destination: path.to_owned(),
            }),
            raw::ERAR_ECLOSE => Err(RarBackendError::Close {
                entry: self.filename(),
                destination: path.to_owned(),
            }),
            raw::ERAR_EREAD => Err(RarBackendError::ReadWhileExtracting {
                entry: self.filename(),
            }),
            raw::ERAR_EWRITE => Err(RarBackendError::Write {
                entry: self.filename(),
                destination: path.to_owned(),
            }),
            raw::ERAR_NO_MEMORY => Err(RarBackendError::OutOfMemory),
            raw::ERAR_EREFERENCE => Err(RarBackendError::ReferenceIsNotAvailable {
                entry: self.filename(),
                destination: path.to_owned(),
            }),
            raw::ERAR_BAD_PASSWORD => Err(RarBackendError::InvalidPassword),
            raw::ERAR_MISSING_PASSWORD => Err(RarBackendError::MissingPassword),
            raw::ERAR_UNKNOWN => {
                // This error code is not documented in `libunrar`. Looks like it is returned at
                // least when entry extraction is interrupted from a callback function.
                Err(RarBackendError::UnknownWhenExtracting {
                    entry: self.filename(),
                    extracted_so_far: {
                        let callback_data = unsafe {
                            Box::from_raw(inner.attributes.UserData as *mut CallbackData)
                        };
                        let extracted_so_far = callback_data.decompressed_entry_size_so_far;
                        Box::into_raw(callback_data);
                        extracted_so_far
                    },
                })
            }
            other => Err(RarBackendError::UnexpectedResultCodeExtractEntry(other)),
        }
    }

    /// Provides entry path-and-filename.
    pub fn filename(&self) -> String {
        WideCString::from_vec_truncate(self.header.FileNameW).to_string_lossy()
    }

    /// Provides entry's file attributes.
    /// The meaning of attributes depends on the operating system which added a file to archive.
    pub fn file_attr(&self) -> FileAttributes {
        self.header.FileAttr.into()
    }

    /// Provides unpacked file CRC32 checksum according to archive entry headers.
    pub fn file_crc32(&self) -> Crc32 {
        self.header.FileCRC.into()
    }

    /// File's modification timestamp according to archive entry headers.
    pub fn file_time(&self) -> MsDosDateTimeRaw {
        self.header.FileTime.into()
    }

    /// Returns true if the entry is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.header.Flags & raw::RHDF_ENCRYPTED != 0
    }

    /// Returns true if the entry is split between multiple RAR volumes.
    pub fn is_split_between_volumes(&self) -> bool {
        self.header.Flags & (raw::RHDF_SPLITBEFORE | raw::RHDF_SPLITAFTER) != 0
    }

    /// Returns true if previous files data is used/necessary for extraction.
    pub fn is_solid(&self) -> bool {
        self.header.Flags & raw::RHDF_SOLID != 0
    }

    /// Returns true if the entry is of directory type.
    pub fn is_directory(&self) -> bool {
        self.header.Flags & raw::RHDF_DIRECTORY != 0
    }

    /// Return compression method used for the archive entry.
    pub fn compression_method(&self) -> CompressionMethod {
        self.header.Method.into()
    }

    /// Returns uncompressed data size in bytes when it is available.
    pub fn uncompressed_size(&self) -> Option<u64> {
        const UNDOCUMENTED_MAGIC_VALUE_FOR_UNKNOWN_SIZE: u64 = 0x7fffffff7fffffff;

        match ((self.header.UnpSizeHigh as u64) << 32) | self.header.UnpSize as u64 {
            UNDOCUMENTED_MAGIC_VALUE_FOR_UNKNOWN_SIZE => None,
            other => Some(other),
        }
    }

    /// Returns compressed data size in bytes.
    pub fn compressed_size(&self) -> u64 {
        ((self.header.PackSizeHigh as u64) << 32) | self.header.PackSize as u64
    }

    /// Returns operating system which created/added current entry to an archive.
    pub fn creation_os(&self) -> OperatingSystem {
        self.header.HostOS.into()
    }

    /// RAR version necessary to entry extraction, in a string form as "MAJOR.MINOR".
    pub fn version_to_extract(&self) -> String {
        let major = self.header.UnpVer / 10;
        let minor = self.header.UnpVer % 10;
        format!("{major}.{minor}")
    }

    /// Returns entry's compression dictionary size in bytes.
    pub fn dict_size(&self) -> u64 {
        self.header.DictSize as u64 * 1024
    }

    /// Returns type of hash function used to protect file data integrity.
    /// Can be:
    /// - no checksum or unknown hash function type,
    /// - CRC32 or
    /// - BLAKE2sp
    pub fn hash_type(&self) -> HashType {
        self.header.HashType.into()
    }

    /// Returns Blake2sp hash value if entry's hash type is Blake2sp.
    pub fn blake2_hash(&self) -> Option<Blake2spHash> {
        if self.hash_type() == HashType::Blake2sp {
            Some(self.header.Hash.into())
        } else {
            None
        }
    }

    /// Returns type of file system redirection.
    ///
    /// It is expected to be one of the following options:
    /// - No redirection, usual file,
    /// - Unix symbolic link,
    /// - Windows symbolic link,
    /// - Windows junction,
    /// - Hard link,
    /// - File reference saved with `-oi` switch.
    pub fn redir_type(&self) -> RedirType {
        self.header.RedirType.into()
    }

    /// Returns file system redirection target name, such as target of symbolic link or file
    /// reference.
    pub fn redir_name(&self) -> Option<String> {
        let redir_name = unsafe {
            WideCString::from_ptr_truncate(self.header.RedirName, self.header.RedirNameSize as _)
        }
        .to_string_lossy();

        match redir_name.is_empty() {
            true => None,
            false => Some(redir_name),
        }
    }

    /// Optional file modification time (see also `file_time`).
    pub fn mtime_raw(&self) -> Option<WindowsFiletimeRaw> {
        let ticks = ((self.header.MtimeHigh as u64) << 32) | self.header.MtimeLow as u64;

        match ticks == 0 {
            true => None,
            false => Some(ticks.into()),
        }
    }

    /// Optional file creation (?) time.
    pub fn ctime_raw(&self) -> Option<WindowsFiletimeRaw> {
        let ticks = ((self.header.CtimeHigh as u64) << 32) | self.header.CtimeLow as u64;

        match ticks == 0 {
            true => None,
            false => Some(ticks.into()),
        }
    }

    /// Optional file last access time.
    pub fn atime_raw(&self) -> Option<WindowsFiletimeRaw> {
        let ticks = ((self.header.AtimeHigh as u64) << 32) | self.header.AtimeLow as u64;

        match ticks == 0 {
            true => None,
            false => Some(ticks.into()),
        }
    }

    /// Entry compression ratio if it could be meaningfully calculated.
    pub fn compress_ratio(&self) -> Option<f32> {
        match self.uncompressed_size() {
            Some(uncompressed_size) => {
                match (uncompressed_size as f32).div(self.compressed_size() as f32) {
                    v if v.is_normal() => Some(v),
                    _ => None,
                }
            }
            None => None,
        }
    }
}

impl Drop for EntryHandler {
    fn drop(&mut self) {
        trace!("dropping an entry handler");
        let _ = unsafe { Box::from_raw(self.header.RedirName) };
    }
}

/// RAR compression methods which could be reported by `libunrar`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionMethod {
    Store,
    Fastest,
    Fast,
    Normal,
    Good,
    Best,
    Unknown(u32),
}

impl From<u32> for CompressionMethod {
    fn from(value: u32) -> Self {
        match value {
            0x30 => Self::Store,
            0x31 => Self::Fastest,
            0x32 => Self::Fast,
            0x33 => Self::Normal,
            0x34 => Self::Good,
            0x35 => Self::Best,
            _ => Self::Unknown(value),
        }
    }
}

/// Operating systems which could be reported by `libunrar`.
#[derive(Debug, Serialize)]
pub enum OperatingSystem {
    MsDos,
    Os2,
    Windows,
    Unix,
    Unknown(u32),
}

impl From<u32> for OperatingSystem {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::MsDos,
            1 => Self::Os2,
            2 => Self::Windows,
            3 => Self::Unix,
            _ => Self::Unknown(value),
        }
    }
}

/// Possible hash methods/types which could be reported by `libunrar`.
#[derive(Debug, Serialize, PartialEq)]
pub enum HashType {
    NoneOrUnknown,
    CRC32,
    Blake2sp,
    Unexpected(u32),
}

impl From<u32> for HashType {
    fn from(value: u32) -> Self {
        match value {
            v if v == raw::RAR_HASH_NONE => Self::NoneOrUnknown,
            v if v == raw::RAR_HASH_CRC32 => Self::CRC32,
            v if v == raw::RAR_HASH_BLAKE2 => Self::Blake2sp,
            _ => Self::Unexpected(value),
        }
    }
}

/// Types of "redirection" which could be reported for archive entry by `libunrar`.
#[derive(Debug, Serialize, PartialEq)]
pub enum RedirType {
    None,
    UnixSymlink,
    WindowsSymlink,
    WindowsJunction,
    HardLink,
    FileReference,
    Unknown(u32),
}

impl From<u32> for RedirType {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::UnixSymlink,
            2 => Self::WindowsSymlink,
            3 => Self::WindowsJunction,
            4 => Self::HardLink,
            5 => Self::FileReference,
            _ => Self::Unknown(value),
        }
    }
}

/// Types of messages which could be passed by `libunrar` into a callback function.
#[derive(Debug, PartialEq)]
#[repr(u32)]
pub enum CallbackMessage {
    ChangeVolume,
    ChangeVolumeWide,
    NeedPassword,
    NeedPasswordWide,
    ProcessData,
    Unknown(u32),
}

impl From<c_uint> for CallbackMessage {
    #[rustfmt::skip]
    fn from(value: c_uint) -> Self {
        use raw::UNRARCALLBACK_MESSAGES;
        match value {
            v if v == UNRARCALLBACK_MESSAGES::UCM_CHANGEVOLUME  as u32 => Self::ChangeVolume,
            v if v == UNRARCALLBACK_MESSAGES::UCM_CHANGEVOLUMEW as u32 => Self::ChangeVolumeWide,
            v if v == UNRARCALLBACK_MESSAGES::UCM_NEEDPASSWORD  as u32 => Self::NeedPassword,
            v if v == UNRARCALLBACK_MESSAGES::UCM_NEEDPASSWORDW as u32 => Self::NeedPasswordWide,
            v if v == UNRARCALLBACK_MESSAGES::UCM_PROCESSDATA   as u32 => Self::ProcessData,
            _ => Self::Unknown(value),
        }
    }
}

/// Raw MS DOS date/time value provided by `libunrar`.
#[derive(Debug, Serialize, Clone, Copy)]
pub struct MsDosDateTimeRaw(u32);

impl From<u32> for MsDosDateTimeRaw {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl TryFrom<MsDosDateTimeRaw> for PrimitiveDateTime {
    type Error = time::Error;

    #[rustfmt::skip]
    fn try_from(value: MsDosDateTimeRaw) -> Result<Self, Self::Error> {
        let seconds = ((value.0 & 0b0000000000000000_0000000000011111) <<  1) as u8;
        let minutes = ((value.0 & 0b0000000000000000_0000011111100000) >>  5) as u8;
        let hours   = ((value.0 & 0b0000000000000000_1111100000000000) >> 11) as u8;
        let days    = ((value.0 & 0b0000000000011111_0000000000000000) >> 16) as u8;
        let months  = ((value.0 & 0b0000000111100000_0000000000000000) >> 21) as u8;
        let years   = ((value.0 & 0b1111111000000000_0000000000000000) >> 25) as i32;

        Ok(Self::new(
            Date::from_calendar_date(1980 + years, Month::try_from(months)?, days)?,
            Time::from_hms(hours, minutes, seconds)?,
        ))
    }
}

/// A wrapper for CRC32 checksum value.
#[derive(Debug, Clone, Copy)]
pub struct Crc32(u32);

impl From<u32> for Crc32 {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl Serialize for Crc32 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:08x}", self.0))
    }
}

/// A wrapper for Blake2sp hash value.
#[derive(Debug, Clone, Copy)]
pub struct Blake2spHash([i8; 32]);

// Note: bindgen may opt for i8 or u8 based on a fair roll of dice, therefore
// both cases are handled here
impl From<[i8; 32]> for Blake2spHash {
    fn from(value: [i8; 32]) -> Self {
        Self(value)
    }
}

impl From<[u8; 32]> for Blake2spHash {
    fn from(value: [u8; 32]) -> Self {
        // mem::transmute would be quicker but the compiler should figure
        let mut as_i8 = [ 0i8; 32 ];
        for (i, v) in value.into_iter().enumerate() {
            as_i8[i] = v as i8;
        }
        Self(as_i8)
    }
}

impl Serialize for Blake2spHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.iter().try_fold(String::new(), |mut output, byte| {
            write!(output, "{byte:02x}").map_err(S::Error::custom)?;
            Ok(output)
        })?)
    }
}

/// Raw Windows Filetime value which could be provided by `libunrar`.
#[derive(Debug, Serialize, Clone, Copy)]
pub struct WindowsFiletimeRaw(u64);

impl From<u64> for WindowsFiletimeRaw {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl TryFrom<WindowsFiletimeRaw> for OffsetDateTime {
    type Error = OffsetDateTimeRangeError;

    fn try_from(value: WindowsFiletimeRaw) -> Result<Self, Self::Error> {
        Self::try_from(FileTime::NT_TIME_EPOCH.saturating_add(Duration::from_micros(value.0 / 10)))
    }
}

/// Raw value of file attributes as reported by an operating system.
/// The meaning of file attributes depends on the operating system which added a file to archive.
#[derive(Debug, Clone, Copy)]
pub struct FileAttributes(u32);

impl From<u32> for FileAttributes {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl Serialize for FileAttributes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:06o}", self.0))
    }
}

/// MS DOS datetime either converted into `OffsetDateTime`, or in a raw form as obtained from
/// `libunrar`, if conversion has failed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MsDosDateTime {
    Parsed(#[serde(serialize_with = "time::serde::timestamp::serialize")] OffsetDateTime),
    Raw(MsDosDateTimeRaw),
}

/// Windows Filetime either converted into `OffsetDateTime`, or in a raw form as obtained from
/// `libunrar`, if conversion has failed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowsFiletime {
    Parsed(#[serde(serialize_with = "time::serde::timestamp::serialize")] OffsetDateTime),
    Raw(WindowsFiletimeRaw),
}
