//! # A library to parse the *Office VBA File Format Structure*
//!
//! Vba provides functionality to parse and decode *Visual Basic for Applications* (VBA) structures
//! and to retrieve the modules source code
//!
//! # The VBA project
//! VBA structures, data and code are arranged inside *Projects*
//!
//! The project consists fundamentally of:
//! * *Project* info: global data like versioning info, external references, constants, the list of
//!   Modules, etc
//! * *Modules*: VBA code units (plus a bunch of local metadata)
//! * *Forms*: Office Forms used by the Project - not covered here and entirely documented under the
//!   [forms] module
//!
//! ## *PerformanceCache* and other monsters
//! Despite the relatively simple structure depicted above, nothing is ever straight forward when
//! it comes to MS formats; in fact it turns out there are different routes to the same destinations
//!
//! Office documents in fact keep Project and Module info in two distinct forms: version-dependent
//! and version-independent. Similarly the VBA code is stored both in its original "source" form -
//! i.e. in textual form - as well as in its compiled binary form - known as *P-Code*
//!
//! While the version-independent structures are "thoroughly" documented in
//! [[MS-OVBA]](https://docs.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/575462ba-bf67-4190-9fac-c275523c75fc),
//! the version-dependent path is paved with several different undocumented blobs going under the
//! generic name of `PerformanceCache`
//!
//! ### The version-independent form
//! This comes in the guise of a bunch of different structures allocated inside the `PROJECT` and
//! `VBA/dir` streams for the Project and Modules info and the second half of the `VBA/${MODULE_NAME}`
//! stream for the Module source code
//!
//! ### The version-dependent form
//! This consists of a set of *PerformanceCache* blobs which appear to be, more or less, rough memory
//! dumps from the VB compiler output
//!
//! The Project *PerformanceCache* is located inside the `VBA/_VBA_PROJECT` while that of the Modules
//! can be found in the first half of the `VBA/${MODULE_NAME}` stream and contains both the Module
//! info and the "compiled" source code
//!
//! ### When in doubt, choose both
//! While the canonical, version-independent, form is the most interoperable and the one guaranteeing
//! the most compatibility, it is often not the one used by Office which, for performance reasons,
//! might choose the cached version instead
//!
//! The rule of thumb, according to the specs, should be that a comparison is performed between the
//! the VBA version in the Office suite which opens the document and the VBA version of the document
//! creator - as declared inside `VBA/_VBA_PROJECT` (see [VbaProject::vba_version])
//!
//! In the case of a match, the version-dependent path is taken: i.e. the `VBA/dir` is skipped
//! entirely and so is the bottom half of the module streams
//!
//! In case of a mismatch, then the canonical path is followed
//!
//! In practice, given that the two sides can diverge wildly (as in the case of *VBA stomping*),
//! from an analytical perspective it's hard to choose the *right* path unless it is known for
//! sure which version of Office we're protecting/researching against
//!
//! Moreover, as usual, MS is the first transgressor of their own rules: older versions of Office
//! tend to blindly load the cached blobs disregarding the versioning (this includes the case when
//! version 0xFFFF is specified, which is the designed way to disable loading of *PerformanceCache*s)
//! or to load *SRP*s (yet another form of cached content) when pursuing a version-independent path
//!
//! ## A multi-path interface
//! Due to the dual nature of VBA content, this module interface is split accordingly:
//! canonical (version-independent) and cached (version-dependent) functions are made available
//! to retrieve the Project and Module info as well as the Module VBA code
//!
//! In extreme summary, the provided interface looks like this:
//! ```text
//!                 [2]-------------------+    [5]-------------------+
//!                 | Version-independent |    | Version-independent |
//!            +--> |   Project info      |--->|     Module info     |
//!            |    +---------------------+    +---------------------+       [7]-----------+
//!            |              |                           |             +--->| Source code |
//!            |              v                           v             |    +-------------+
//!            |     [4]--------------+           [7]-------------+     |
//!            |     |  ProjectTrait  |           |  ModuleTrait  |---->+
//!            |     | ProjectGeneric |           | ModuleGeneric |     |
//! [1]---+    |     +----------------+           +---------------+     |    [8]-----------------+
//! | Vba |--->+              ^                           ^             +--->| Decompiled P-code |
//! +-----+    |              |                           |                  +-------------------+
//!            |    [3]-------------------+    [6]-------------------+
//!            +--> |  Version-dependent  |--->|  Version-dependent  |
//!            |    |    Project info     |    |     Module info     |
//!            |    +---------------------+    +---------------------+
//!            |
//!            |    [10]-----------+
//!            +--> | Office forms |
//!                 +--------------+
//!
//! ```
//! 1. The main interface for VBA parsing: [`Vba`] is the entry interface for anything VBA - code
//!    examples can be found here
//! 2. Version-independent VBA Project info: [`Project`] - use [`Vba::project()`]
//! 3. Version-dependent VBA Project info: [`ProjectPC`] - use [`Vba::project_pc()`]
//! 4. Convenience interfaces for projects:
//!    - a shared trait: [`ProjectTrait`]
//!    - a unifying enum: [`ProjectGeneric`] - see [`Project::as_gen()`] / [`ProjectPC::as_gen()`]
//! 5. Version-independent VBA Module metadata: [`Module`] - obtained via the [`Project::modules()`]
//!    iterator (or its convenience shortcut [`Vba::modules()`])
//! 6. Version-dependent VBA Module metadata: [`ModulePC`] - obtained via the [`ProjectPC::modules()`]
//!    iterator (or its convenience shortcut [`Vba::modules_pc()`])
//! 7. Convenience interfaces for module info and VBA code:
//!    - a shared trait: [`ModuleTrait`]
//!    - a unifying enum: [`ModuleGeneric`] - see [`Module::as_gen()`] / [`ModulePC::as_gen()`]
//! 8. An interface for retrieving the version-independent module source code - refer to [`Vba::get_code_stream()`]
//! 9. An interface for source code decompilation - refer to [`Vba::get_decompiler()`]
//! 10. Read the [forms module documentation](forms)
//!
//! # Notes and final warnings
//! The core of the implementation is strictly based upon [MS-OVBA] however a lot of effort was
//! put into researching undocumented and semi-documented "features" and in testing the actual
//! behavior of different Office products
//!
//! The version-dependent interface is entirely based on the reverse engineering of relatively
//! recent Office versions and is therefore mostly tested with VB7 content
//!
//! The situation with the P-Code decompiler in particular is quite complicated: the VBA parser
//! and interpreter are extremely loose, lack even elementary bound checking, tend to behave
//! weirdly on unorthodox input and crash - see [`ModuleDecompiler`] for the gory details
//!
//! In short if you're parsing something that's been produced by Office itself, the results
//! should be alright, both within Office and with the decompiler; however, if the data have been
//! touched by an evil hand, then all bets are off. Always take the results with a grain of salt

#![warn(missing_docs)]
#![allow(clippy::collapsible_else_if)]

pub mod decomp;
mod dir;
pub mod forms;
mod vba_project;

#[cfg(not(test))]
use ctxole::{Ole, OleEntry};
use ctxutils::win32::GUID;
#[cfg(test)]
pub(crate) use mockedole::{Ole, OleEntry};
use std::io::{self, BufRead, BufReader, Read, Seek};

use decomp::CompressContainerReader;
pub use dir::{Module, Project, ProjectInfo, Reference};
pub use vba_project::pcode::ModuleDecompiler;
pub use vba_project::{ModulePC, ProjectPC, VbaProject};

/// The parser for *Office VBA File Format Structure* (VBA)
///
/// # Examples
/// ```no_run
/// use ctxole::Ole;
/// use vba::*;
/// use std::fs::File;
/// use std::io::BufReader;
///
/// let f = File::open("MyDocument.doc").unwrap();
/// let ole = Ole::new(BufReader::new(f)).unwrap();
/// let vba = Vba::new(&ole, "Macros").unwrap();
/// let module = vba.modules()
///     .unwrap()
///     .find(|m| m.main_name() == Some("ThisDocument"))
///     .unwrap();
/// let mut stream = vba.get_code_stream(&module).unwrap();
/// let mut code: Vec<u8> = Vec::new();
/// std::io::copy(&mut stream, &mut code);
/// ```
pub struct Vba<'a, R: Read + Seek> {
    ole: &'a Ole<R>,
    rootdir: String,
    /// VBA version info
    pub vba_project: VbaProject,
    project: Result<Project, io::Error>,
    project_pc: Result<ProjectPC, io::Error>,
    forms: Vec<String>,
    cp: u16,
}

impl<'a, R: Read + Seek> Vba<'a, R> {
    /// Creates a new VBA parser
    ///
    /// # Errors
    /// * Errors from the IO layer are bubbled
    /// * Errors generated in the parser are reported with [`ErrorKind`](std::io::ErrorKind)
    ///   set to [`InvalidData`](std::io::ErrorKind#variant.InvalidData)
    ///
    pub fn new(ole: &'a Ole<R>, rootdir: &str) -> Result<Self, io::Error> {
        let rootdir = rootdir.to_owned();

        let entry = ole.get_entry_by_name(&format!("{}/VBA/_VBA_PROJECT", rootdir))?;
        let mut vba_project_stream = ole.get_stream_reader(&entry);
        let vba_project = VbaProject::new(&mut vba_project_stream)?;
        let project = match ole.get_entry_by_name(&format!("{}/VBA/dir", rootdir)) {
            Ok(entry) => {
                let mut stream = ole.get_stream_reader(&entry);
                Project::new(&mut stream, entry.size)
            }
            Err(e) => Err(e),
        };
        let cp: u16 = project
            .as_ref()
            .ok()
            .and_then(|p| p.info.codepage)
            .unwrap_or(1252);
        let project_pc = ProjectPC::new(&mut vba_project_stream, &vba_project);

        // FIXME: properly parse if we are to extract the VBA password and lock info
        let mut forms: Vec<String> = Vec::new();
        let entry = ole.get_entry_by_name(&format!("{}/PROJECT", rootdir))?;
        let stream = utf8dec_rs::UTF8DecReader::for_windows_cp(cp, ole.get_stream_reader(&entry));
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if line.starts_with('[') {
                break;
            }
            if let Some(v) = line.strip_prefix("BaseClass=") {
                forms.push(v.to_string())
            }
        }

        Ok(Self {
            ole,
            rootdir,
            vba_project,
            project,
            project_pc,
            forms,
            cp,
        })
    }

    /// Returns version-independent project info
    pub fn project(&self) -> Result<&Project, io::Error> {
        self.project
            .as_ref()
            .map_err(|e| io::Error::new(e.kind(), e.to_string()))
    }

    /// Returns version-dependent project info
    pub fn project_pc(&self) -> Result<&ProjectPC, io::Error> {
        self.project_pc
            .as_ref()
            .map_err(|e| io::Error::new(e.kind(), e.to_string()))
    }

    /// Returns a stream to [`Read`] the module source code
    pub fn get_code_stream<M: ModuleTrait>(&self, module: &M) -> Result<impl Read + '_, io::Error> {
        let (entry, offset) = self.get_module_entry_offset(module)?;
        let mut reader = self.ole.get_stream_reader(&entry);
        let offset: u64 = offset.into();
        reader.seek(io::SeekFrom::Start(offset))?;
        Ok(utf8dec_rs::UTF8DecReader::for_windows_cp(
            self.cp,
            CompressContainerReader::new(reader, entry.size - offset)?,
        ))
    }

    /// Returns a decompiler for the Module *P-Code*
    ///
    /// Refer to the [`ModuleDecompiler` docs](`ModuleDecompiler`)
    pub fn get_decompiler<M: ModuleTrait>(
        &self,
        module: &M,
    ) -> Result<vba_project::pcode::ModuleDecompiler, io::Error> {
        let (entry, offset) = self.get_module_entry_offset(module)?;
        let mut stream = self.ole.get_stream_reader(&entry);
        vba_project::pcode::ModuleDecompiler::new(
            &mut stream,
            offset,
            self.project_pc()?,
            &self.vba_project,
        )
    }

    /// Iterates over the document forms
    ///
    /// Returns a 2-item tuple with:
    /// * The form name
    /// * The corresponding `Result`-wrapped [`UserForm`](forms::UserForm)s
    pub fn forms(&self) -> impl Iterator<Item = (&str, Result<forms::UserForm<R>, io::Error>)> {
        self.forms.iter().map(|f| {
            let path = format!("{}/{}", self.rootdir, f);
            (f.as_str(), forms::UserForm::new(self.ole, f, &path))
        })
    }

    fn get_module_entry<M: ModuleTrait>(&self, module: &M) -> Result<OleEntry, io::Error> {
        module
            .stream_names()
            .iter()
            .find_map(|name| {
                self.ole
                    .get_entry_by_name(&format!("{}/VBA/{}", self.rootdir, name))
                    .ok()
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "This module doesn't have a stream name",
                )
            })
    }

    fn get_module_entry_offset<M: ModuleTrait>(
        &self,
        module: &M,
    ) -> Result<(OleEntry, u32), io::Error> {
        let offset = match module.offset() {
            Some(off) => off,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "This module doesn't have an offset",
                ))
            }
        };
        let entry = self.get_module_entry(module)?;
        if u64::from(offset) >= entry.size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "The offset of this module exceeds the containing entry size ({} >= {})",
                    offset, entry.size
                ),
            ));
        }
        Ok((entry, offset))
    }

    /// Version-independent list of project *Modules*
    pub fn modules(&'a self) -> Result<impl Iterator<Item = &'a Module>, io::Error> {
        Ok(self.project()?.modules())
    }

    /// Version-dependent list of project *Modules*
    pub fn modules_pc(&'a self) -> Result<impl Iterator<Item = ModulePC<'a>>, io::Error> {
        Ok(self.project_pc()?.modules())
    }
}

#[derive(Debug)]
/// The content of the referenced object
pub enum ReferenceValue {
    /// The identifier of the referenced *Automation type library*
    Original(ReferenceOriginal),
    /// Reference to a *twiddled type library* and its *extended type library*
    Control(ReferenceControl),
    /// Reference to an *Automation type library*
    Registered(ReferenceRegistered),
    /// Reference to an external VBA project
    Project(ReferenceProject),
}

#[derive(Debug, Default)]
/// The identifier of the referenced *Automation type library*
pub struct ReferenceOriginal {
    /// The identifier of the referenced *Automation type library*
    pub libid_original: Option<String>,
}

#[derive(Debug, Default)]
/// Reference to a *twiddled type library* and its *extended type library*
pub struct ReferenceControl {
    /// The identifier of the referenced *Automation type library*
    pub original: ReferenceOriginal,
    /// A *twiddled type* library’s identifier
    pub twiddled: Option<String>,
    /// The name of the extended type library (code page variant)
    pub record_name: Option<String>,
    /// The name of the extended type library (UTF-16 variant)
    pub record_name_unicode: Option<String>,
    /// The extended type library’s identifier.
    pub libid: Option<String>,
    /// The `GUID` of the Automation type library the extended type library was generated from
    pub guid: GUID,
    /// The extended type library’s cookie
    pub cookie: u32,
}

#[derive(Debug, Default)]
/// Reference to an *Automation type library*
pub struct ReferenceRegistered {
    /// The Automation type library’s identifier
    pub libid: Option<String>,
}

#[derive(Debug, Default)]
/// Reference to an external VBA project
pub struct ReferenceProject {
    /// Absolute path to the referenced VBA project’s identifier
    pub absolute: Option<String>,
    /// Relative path to the referenced VBA project’s identifier
    pub relative: Option<String>,
    /// The major version of the referenced VBA project
    pub version_major: u32,
    /// The minor version of the referenced VBA project
    pub version_minor: u16,
}

fn flatten_main_alt<'a>(main: Option<&'a str>, alt: Option<&'a str>) -> Vec<&'a str> {
    let mut ret = [main, alt].into_iter().flatten().collect::<Vec<&str>>();
    ret.dedup();
    ret
}

/// A common trait to retrieve Project info
pub trait ProjectTrait<'a> {
    /// The platform for which the VBA project is created
    ///
    /// | Value      | Meaning                       |
    /// |------------|-------------------------------|
    /// | 0x00000000 | For 16-bit Windows Platforms. |
    /// | 0x00000001 | For 32-bit Windows Platforms. |
    /// | 0x00000002 | For Macintosh Platforms.      |
    /// | 0x00000003 | For 64-bit Windows Platforms. |
    ///
    fn sys_kind(&self) -> Option<u32>;
    /// The project language (`LCID`)
    fn lcid(&self) -> Option<u32>;
    /// The language (`LCID`) of the *Automation Server*
    fn lcid_invoke(&self) -> Option<u32>;
    /// The project code page
    fn codepage(&self) -> Option<u16>;
    /// The name of the project
    fn name(&self) -> Option<&str>;
    /// The project description
    fn docstring(&self) -> Option<&str>;
    /// The path to the project Help file
    fn help(&self) -> Option<&str>;
    /// The Help topic identifier in the Help file
    fn help_context(&self) -> Option<u32>;
    /// The `LIBFLAGS` for the project’s *Automation type library*
    /// ```c
    /// typedef [v1_enum] enum tagLIBFLAGS
    /// {
    ///     LIBFLAG_FRESTRICTED = 0x01,
    ///     LIBFLAG_FCONTROL = 0x02,
    ///     LIBFLAG_FHIDDEN = 0x04,
    ///     LIBFLAG_FHASDISKIMAGE = 0x08
    /// } LIBFLAGS;
    /// ```
    fn lib_flags(&self) -> Option<u32>;
    /// The major version of the project
    fn version_major(&self) -> Option<u32>;
    /// The minor version of the project
    fn version_minor(&self) -> Option<u16>;
    /// The compilation constants for the project
    fn constants(&'a self) -> Box<dyn Iterator<Item = (&'a str, i16)> + 'a>;
    /// External references
    fn references(&self) -> &[Reference];
    /// *Documented* as: "MUST be ignored on read. MUST be 0xFFFF on write."
    fn cookie(&self) -> Option<u16>;
}

/// A common trait to retrieve Module info
///
/// # Note
/// Most info are actually stored twice (once in UTF-16, once in code page form) and while
/// they usually match, they don't have to, nor they always do (a mismatch could indicate
/// corruption or manual tampering)
///
/// The interface provides access to both in the form of a *main* and an *alt* variants
/// where `main` indicates the form preferred by Office and `alt` indicates the fallback
/// value which is used in case the `main` variant is missing
///
/// Affected fn's include:
/// * [`main_name()`](Self::main_name) vs [`alt_name()`](Self::alt_name)
/// * [`main_stream_name()`](Self::main_stream_name) vs [`alt_stream_name()`](Self::alt_stream_name)
/// * [`main_docstring()`](Self::main_docstring) vs [`alt_docstring()`](Self::alt_docstring)
pub trait ModuleTrait {
    /// The preferred name of the module
    fn main_name(&self) -> Option<&str>;
    /// The alternative name of the module
    fn alt_name(&self) -> Option<&str>;
    /// The preferred name of the Ole stream which contains the module data
    fn main_stream_name(&self) -> Option<&str>;
    /// The alternative name of the Ole stream which contains the module data
    fn alt_stream_name(&self) -> Option<&str>;
    /// The preferred project description
    fn main_docstring(&self) -> Option<&str>;
    /// The alternative project description
    fn alt_docstring(&self) -> Option<&str>;
    /// The (unique) list of module names in preference order
    fn names(&self) -> Vec<&str> {
        flatten_main_alt(self.main_name(), self.alt_name())
    }
    /// The (unique) list of module stream names in preference order
    fn stream_names(&self) -> Vec<&str> {
        flatten_main_alt(self.main_stream_name(), self.alt_stream_name())
    }
    /// The (unique) list of module docstrings in preference order
    fn docstrings(&self) -> Vec<&str> {
        flatten_main_alt(self.main_docstring(), self.alt_docstring())
    }
    /// The offset in the module stream at which the compressed source code block starts
    ///
    /// By converse, it indicates the size of the `PerformanceCache` module blob
    fn offset(&self) -> Option<u32>;
    /// The help topic identifier
    fn help_context(&self) -> Option<u32>;
    /// The module cookie
    fn cookie(&self) -> Option<u16>;
    /// Indicates whether this module is marked as a procedural module
    ///
    /// Note: this is not the inverse of [`is_non_procedural()`](Self::is_non_procedural)
    fn is_procedural(&self) -> bool;
    /// Indicates whether this module is marked as a document, class module or designer module
    ///
    /// Note: this is not the inverse of [`is_procedural()`](Self::is_procedural)
    fn is_non_procedural(&self) -> bool;
    /// Indicates that this module is marked as read-only
    fn is_read_only(&self) -> bool;
    /// Indicates that this module is marked as private
    fn is_private(&self) -> bool;
}

/// Convenience enum that wraps [`Project`] and [`ProjectPC`] and provides [`ProjectTrait`]
///
/// See [`Project::as_gen()`] and [`ProjectPC::as_gen()`]
pub enum ProjectGeneric<'a> {
    /// Version independent project
    VI(&'a Project),
    /// Version dependent project
    VD(&'a ProjectPC),
}

macro_rules! mkgenfn {
    ($self: tt, $enumname: tt, $fnname: tt) => {
        match $self {
            $enumname::VI(p) => p.$fnname(),
            $enumname::VD(p) => p.$fnname(),
        }
    };
}

impl<'a> ProjectTrait<'a> for ProjectGeneric<'a> {
    fn sys_kind(&self) -> Option<u32> {
        mkgenfn!(self, ProjectGeneric, sys_kind)
    }
    fn lcid(&self) -> Option<u32> {
        mkgenfn!(self, ProjectGeneric, lcid)
    }
    fn lcid_invoke(&self) -> Option<u32> {
        mkgenfn!(self, ProjectGeneric, lcid_invoke)
    }
    fn codepage(&self) -> Option<u16> {
        mkgenfn!(self, ProjectGeneric, codepage)
    }
    fn name(&self) -> Option<&str> {
        mkgenfn!(self, ProjectGeneric, name)
    }
    fn docstring(&self) -> Option<&str> {
        mkgenfn!(self, ProjectGeneric, docstring)
    }
    fn help(&self) -> Option<&str> {
        mkgenfn!(self, ProjectGeneric, help)
    }
    fn help_context(&self) -> Option<u32> {
        mkgenfn!(self, ProjectGeneric, help_context)
    }
    fn lib_flags(&self) -> Option<u32> {
        mkgenfn!(self, ProjectGeneric, lib_flags)
    }
    fn version_major(&self) -> Option<u32> {
        mkgenfn!(self, ProjectGeneric, version_major)
    }
    fn version_minor(&self) -> Option<u16> {
        mkgenfn!(self, ProjectGeneric, version_minor)
    }
    fn constants(&'a self) -> Box<dyn Iterator<Item = (&'a str, i16)> + 'a> {
        mkgenfn!(self, ProjectGeneric, constants)
    }
    fn references(&self) -> &[Reference] {
        mkgenfn!(self, ProjectGeneric, references)
    }
    fn cookie(&self) -> Option<u16> {
        mkgenfn!(self, ProjectGeneric, cookie)
    }
}

/// Convenience enum that wraps [`Module`] and [`ModulePC`] and provides [`ModuleTrait`]
///
/// See [`Module::as_gen()`] and [`ModulePC::as_gen()`]
pub enum ModuleGeneric<'a> {
    /// Version independent module
    VI(&'a Module),
    /// Version dependent module
    VD(ModulePC<'a>),
}

impl<'a> ModuleTrait for ModuleGeneric<'a> {
    fn main_name(&self) -> Option<&str> {
        mkgenfn!(self, ModuleGeneric, main_name)
    }
    fn alt_name(&self) -> Option<&str> {
        mkgenfn!(self, ModuleGeneric, alt_name)
    }
    fn main_stream_name(&self) -> Option<&str> {
        mkgenfn!(self, ModuleGeneric, main_stream_name)
    }
    fn alt_stream_name(&self) -> Option<&str> {
        mkgenfn!(self, ModuleGeneric, alt_stream_name)
    }
    fn main_docstring(&self) -> Option<&str> {
        mkgenfn!(self, ModuleGeneric, main_docstring)
    }
    fn alt_docstring(&self) -> Option<&str> {
        mkgenfn!(self, ModuleGeneric, alt_docstring)
    }
    fn offset(&self) -> Option<u32> {
        mkgenfn!(self, ModuleGeneric, offset)
    }
    fn help_context(&self) -> Option<u32> {
        mkgenfn!(self, ModuleGeneric, help_context)
    }
    fn cookie(&self) -> Option<u16> {
        mkgenfn!(self, ModuleGeneric, cookie)
    }
    fn is_procedural(&self) -> bool {
        mkgenfn!(self, ModuleGeneric, is_procedural)
    }
    fn is_non_procedural(&self) -> bool {
        mkgenfn!(self, ModuleGeneric, is_non_procedural)
    }
    fn is_read_only(&self) -> bool {
        mkgenfn!(self, ModuleGeneric, is_read_only)
    }
    fn is_private(&self) -> bool {
        mkgenfn!(self, ModuleGeneric, is_private)
    }
}

/// Document capable of returning its VBA contents
pub trait VbaDocument<'a, R: Read + Seek> {
    /// Returns an intertface to the Visual Basic for Applications contents
    fn vba(self) -> Option<Result<Vba<'a, R>, io::Error>>;
}

#[cfg(test)]
/// A Mocked Ole container used for unit tests
mod mockedole {
    use std::io::{self, Cursor, Read, Seek};
    use std::marker::PhantomData;

    pub struct Ole<R> {
        names: Vec<String>,
        streams: Vec<Cursor<Vec<u8>>>,
        _phantom: PhantomData<R>,
    }

    #[derive(Default)]
    pub struct OleEntry {
        n: usize,
        // pub id: u32,
        // pub objtype: u8,
        // pub name: String,
        // pub color: u8,
        // pub clsid: GUID,
        // pub state: u32,
        // pub ctime: NaiveDateTime,
        // pub mtime: NaiveDateTime,
        pub size: u64,
        // pub anomalies: Vec<String>,
    }

    impl<R: Read + Seek> Ole<R> {
        pub fn new() -> Self {
            Self {
                names: Vec::new(),
                streams: Vec::new(),
                _phantom: PhantomData,
            }
        }

        pub fn push(&mut self, name: &str, stream: Cursor<Vec<u8>>) {
            self.names.push(name.to_string());
            self.streams.push(stream);
        }

        pub fn get_entry_by_name(&self, name: &str) -> Result<OleEntry, io::Error> {
            let n = self
                .names
                .iter()
                .position(|n| n == name)
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "404"))?;
            Ok(OleEntry {
                n,
                size: self.streams[n].get_ref().len() as u64,
                ..OleEntry::default()
            })
        }

        pub fn get_stream_reader(&self, entry: &OleEntry) -> Cursor<Vec<u8>> {
            self.streams[entry.n].clone()
        }
    }
}
