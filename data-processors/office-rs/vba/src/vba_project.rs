use crate::*;
use std::collections::HashMap;
use std::io::{self, Read, Seek};
use tracing::debug;

use ctxutils::io::*;
mod consts;
use consts::*;
pub mod pcode;

/// Versioning info of the Project (`_VBA_PROJECT` stream header)
#[derive(Debug)]
pub struct VbaProject {
    /// Magic value (0x61cc)
    pub rsvd1: u16,
    /// The version of VBA used to create the VBA project
    pub vba_version: u16,
    /// Should be 0 (not validated by Office)
    pub rsvd2: u8,
    /// Undefined (apparently same as [`ProjectInfo::sys_kind`](crate::ProjectInfo::sys_kind))
    pub rsvd3: u16,
}

impl VbaProject {
    pub(crate) fn new<R: Read + Seek>(f: &mut R) -> Result<Self, io::Error> {
        let rsvd1 = rdu16le(f)?;
        let vba_version = rdu16le(f)?;
        let rsvd2 = rdu8(f)?;
        let rsvd3 = rdu16le(f)?;
        Ok(Self {
            rsvd1,
            vba_version,
            rsvd2,
            rsvd3,
        })
    }

    fn is_5x(&self) -> bool {
        (0x50..=0x5f).contains(&self.vba_version)
    }
}

/// Version-dependent Project info (`_VBA_PROJECT` stream, *PerformanceCache* blob)
///
/// # Important note
/// This blob is not documented, is varies slightly across VBA versions and archs, but
/// it's nonetheless often the first source of data utilized by Office
///
/// Despite the huge time spent reverse engineering Office behavior, the amount of guesswork
/// employed in here remains remarkably vast
///
/// This info and interface are provided here for IoCs, generic digging and exploration of
/// a fascinating alien creature
#[derive(Debug, Default)]
pub struct ProjectPC {
    /// The platform for which the VBA project is created
    ///
    /// | Value      | Meaning                       |
    /// |------------|-------------------------------|
    /// | 0x00000000 | For 16-bit Windows Platforms. |
    /// | 0x00000001 | For 32-bit Windows Platforms. |
    /// | 0x00000002 | For Macintosh Platforms.      |
    /// | 0x00000003 | For 64-bit Windows Platforms. |
    ///
    pub sys_kind: u32,
    /// The project language (`LCID`)
    pub lcid: u32,
    /// The language (`LCID`) of the *Automation Server*
    pub lcid_invoke: u32,
    /// The project code page
    pub codepage: u16,
    /// The index in the [`string_table`](Self::string_table) of the project name
    pub name_idx: u16,
    /// Unknown string
    pub unused_string: Option<String>,
    /// The project description
    pub docstring: Option<String>,
    /// The path to the project Help file
    pub help_file: Option<String>,
    /// The Help topic identifier in the Help file
    pub help_context: u32,
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
    pub lib_flags: u32,
    /// The major version of the project
    pub version_major: u32,
    /// The minor version of the project
    pub version_minor: u16,
    /// The compilation constants for the project (as [`string_table`](Self::string_table)
    /// index + value pairs)
    pub constants: Vec<(u16, i16)>,
    /// External references
    pub references: Vec<dir::Reference>,
    /// *Documented* as: "MUST be ignored on read. MUST be 0xFFFF on write."
    pub cookie: u16,
    modules_data: Vec<ModuleData>,
    /// Contains all the indexed object names and string found in the project its modules
    ///
    /// Note: indexes are left shifted by one - prefer
    /// [`get_string_from_table()`](Self::get_string_from_table()) for lookup
    pub string_table: HashMap<u16, String>,
    is_5x: bool,
}

impl ProjectPC {
    pub(crate) fn new<R: Read + Seek>(
        f: &mut R,
        vba_project: &VbaProject,
    ) -> Result<Self, io::Error> {
        if vba_project.vba_version == 0xffff {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PerformanceCache is disabled on this document",
            ));
        }
        let mut ret = Self {
            is_5x: vba_project.is_5x(),
            ..Self::default()
        };
        f.seek(io::SeekFrom::Current(1))?; // always 0xff
        ret.lcid = rdu32le(f)?;
        ret.lcid_invoke = rdu32le(f)?;
        ret.codepage = rdu16le(f)?;
        ret.sys_kind = rdu32le(f)?;
        f.seek(io::SeekFrom::Current(4 + 4))?; // always [0, 0x10000]
        let cnt = rdu16le(f)?;
        f.seek(io::SeekFrom::Current(2))?; // always 2
        ret.references = Vec::with_capacity(cnt.into());
        for _ in 0..cnt {
            ret.references.push(dir::Reference {
                name: None, // Note: Names are filled in later in this function
                name_unicode: None,
                value: Self::cached_ref(f)?,
            });
        }
        let cnt = rdu16le(f)?;
        let mut mflags: Vec<u16> = Vec::with_capacity(cnt.into());
        for _ in 0..cnt {
            mflags.push(rdu16le(f)?);
        }
        let cnt = rdu16le(f)?;
        ret.constants = Vec::with_capacity(cnt.into());
        for _ in 0..cnt {
            let idx = rdu16le(f)?;
            let val = rdi16le(f)?;
            ret.constants.push((idx, val));
        }
        ret.name_idx = rdu16le(f)?;
        ret.unused_string = Self::maybe_get_string(f)?; // FIXME: always 0xffff; is this a string at all?
        ret.docstring = Self::maybe_get_string(f)?;
        ret.help_file = Self::maybe_get_string(f)?;
        ret.help_context = rdu32le(f)?;
        let _unk = rdu16le(f)?; // mostly 0xffff, sometimes 0
        ret.lib_flags = rdu16le(f)?.into(); // Note: this should technically be u32 but whatever
        ret.version_major = rdu32le(f)?;
        ret.version_minor = rdu16le(f)?;
        // TABLE UNK1
        // This list contains either 0xffff or an ordinal
        // Altering the low values usually results in an infloop loading the document
        for i in 0..42 {
            let v = rdu16le(f)?;
            debug!("UNK1 {} {:04x}", i, v);
        }
        ret.cookie = rdu16le(f)?;

        let cnt = rdu16le(f)?;
        ret.modules_data = Vec::with_capacity(cnt.into());
        if mflags.len() != usize::from(cnt) {
            mflags.clear();
        }
        for i in 0..usize::from(cnt) {
            ret.modules_data.push(ModuleData::new(
                f,
                ret.is_5x,
                *mflags.get(i).unwrap_or(&0),
                ret.codepage,
            )?);
        }

        let _some_size = rdu32le(f)?; // mostly 0xffffffff, rarely 2-aligned 0x2xx or 3xxx
        f.seek(io::SeekFrom::Current(2))?; // always 0x101
        let cnt = rdu32le(f)?;
        // The first portion of this table contains:
        // - 0xffffffff
        // - one of the another_index_and_flags values found in the modules
        //   values are typically 0x2xx, all multiples of 8
        // The second portion of this table contains:
        // - 0xffffffff
        // - a seemingly random id
        // - a low value 1..=1e8
        /*
        debug!("--- TBL ---");
        for i in 0..(cnt/4) {
            debug!("{:#x} -> {:#x}", i, rdu32le(f)?);
        }
         */
        f.seek(io::SeekFrom::Current(i64::from(cnt)))?;

        f.seek(io::SeekFrom::Current(3 * 2))?; // always [0x80, 0x00, 0x00]
        let total_strings = rdu16le(f)?;
        let stored_strings = rdu16le(f)?;
        let static_strings = rdu16le(f)?;
        let keyword_strings = static_strings + stored_strings - total_strings;
        let user_strings = total_strings - static_strings;
        f.seek(io::SeekFrom::Current(4))?; // a strange apparently random 16bit value left-shifted by 2
        ret.string_table = HashMap::with_capacity(stored_strings.into());
        for _ in 0..keyword_strings {
            let (ord, s) = Self::get_keyword_string(f, ret.codepage)?;
            ret.string_table.insert(ord, s);
        }
        for i in 0..(user_strings) {
            let s = Self::get_user_string(f, ret.codepage)?;
            ret.string_table.insert(static_strings + i + 1, s);
        }
        f.seek(io::SeekFrom::Current(5))?; // 0x02u8 0xffffu16, 0x0101u16
        let mut len = rdu32le(f)?;
        /* Note:
         * on recent 64-bit Office's it's possible to overflow the heap by ~5 bytes by
         * setting the len to some lower value (although the heap layout matters a lot)
         */
        let mut name_cnt = 0;
        for i in 0.. {
            if len == 0 {
                break;
            }
            let idx = rdu16le(f)?; // the string index
            let is_system = (idx & 1) == 0; // system or user
            let t = rdu16le(f)?; // the appearance order
            let s = rdu16le(f)?; // another index? this is unique or 0 or -1
            if idx == 0xffff {
                debug!("{}: Absent", i);
            } else {
                if s == 0xffff {
                    if let Some(reference) = ret.references.get_mut(usize::from(t)) {
                        if let Some(name) = ret.string_table.get(&(idx >> 1)) {
                            reference.name = Some(name.to_owned());
                            reference.name_unicode = Some(name.to_owned());
                        }
                    }
                }
                debug!(
                    "{}: {:?}: {} ord:{} / {:#x}",
                    i,
                    ret.get_string_from_table(idx),
                    is_system,
                    t,
                    s
                );
            }
            name_cnt += 1;
            len -= 3 * 2;
        }
        let _cnt1 = rdu16le(f)?; // Note: always smaller that _cnt2, also why u16 vs u32?
        let _cnt2 = rdu32le(f)?; // the number of entries in the name table, allocated, refs, or otherwise
        debug_assert!(u32::from(_cnt1) < _cnt2);
        debug_assert_eq!(name_cnt, _cnt2);
        let _unk = rdu16le(f)?; // always 1
        let tail_len = rdu32le(f)?;
        let mut buf = vec![0u8; tail_len.try_into().unwrap()];
        f.read_exact(&mut buf)?;
        // a confusing array of 16bit values, mostly zero
        let tail: Vec<u16> = buf
            .chunks_exact(2)
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect();
        debug!("tail {:04x?}", tail);
        debug!("Strings: {:x?}", ret.string_table);
        debug!("vba_project: {:x} bytes parsed", f.stream_position()?);
        Ok(ret)
    }

    fn cached_ref<R: Read + Seek>(f: &mut R) -> Result<ReferenceValue, io::Error> {
        let name = Self::get_string(f)?;
        if name.starts_with("*\\C") || name.starts_with("*\\D") {
            let relative = Self::get_string(f)?;
            let version_major = rdu32le(f)?;
            let version_minor = rdu16le(f)?;
            f.seek(io::SeekFrom::Current(6))?;
            return Ok(ReferenceValue::Project(ReferenceProject {
                absolute: Some(name),
                relative: Some(relative),
                version_major,
                version_minor,
            }));
        }
        f.seek(io::SeekFrom::Current(10))?;
        let sub = rdu16le(f)?;
        if sub == 0 {
            return Ok(ReferenceValue::Registered(ReferenceRegistered {
                libid: Some(name),
            }));
        }
        if sub == 1 {
            let libid = Self::get_string(f)?;
            f.seek(io::SeekFrom::Current(10))?;
            let cookie = rdu32le(f)?;
            let guid = GUID::from_le_stream(f)?;
            let sub = rdu16le(f)?;
            if sub == 0 {
                return Ok(ReferenceValue::Control(ReferenceControl {
                    original: ReferenceOriginal {
                        libid_original: Some(name),
                    },
                    twiddled: None,
                    record_name: None,
                    record_name_unicode: None,
                    libid: Some(libid),
                    guid,
                    cookie,
                }));
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Reference {} has unknown sub 1/{}", name, sub),
            ));
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Reference {} has unknown sub {}", name, sub),
        ))
    }

    /// Returns an index based string from the project string table
    ///
    /// Mainly for internal use, exported for low level operations
    ///
    /// Note: the passed idx is automatically shifted to match that of the [`string_table`](Self::string_table)
    ///
    /// See also [`get_string_from_table_or_builtin()`](Self::get_string_from_table_or_builtin)
    pub fn get_string_from_table(&self, idx: u16) -> Option<&str> {
        self.string_table.get(&(idx >> 1)).map(|s| s.as_str())
    }

    /// Returns an index based string from the project string table complemented with the VB builtin keywords
    ///
    /// Mainly for internal use, exported for low level operations
    ///
    /// See [`get_string_from_table()`](Self::get_string_from_table_or_builtin) for internal details
    pub fn get_string_from_table_or_builtin(&self, idx: u16) -> Option<&str> {
        self.get_string_from_table(idx).or_else(|| {
            let idx = idx >> 1;
            if idx >= 1 {
                if self.is_5x {
                    BUILTIN_IDS_V5X.get(usize::from(idx) - 1).copied()
                } else {
                    BUILTIN_IDS.get(usize::from(idx) - 1).copied()
                }
            } else {
                None
            }
        })
    }

    /// Returns an iterator over the constants compilation arguments
    pub fn get_constants(&self) -> impl Iterator<Item = (&str, i16)> {
        self.constants
            .iter()
            .filter_map(|(idx, val)| self.get_string_from_table(*idx).map(|s| (s, *val)))
    }

    fn get_keyword_string<R: Read>(f: &mut R, cp: u16) -> Result<(u16, String), io::Error> {
        let id_len = rdu8(f)?;
        let id_type = rdu8(f)?;
        if id_len != 0 || id_type != 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Not a keyword"));
        }

        let ord = rdu16le(f)?;
        let id_len = rdu8(f)?;
        let id_type = rdu8(f)?;
        let s = if id_type & 0x80 != 0 {
            let u1 = rdu16le(f)?;
            let u2 = rdu16le(f)?;
            let u3 = rdu16le(f)?;
            let mut buf = vec![0u8; id_len.into()];
            f.read_exact(&mut buf)?;
            let s = utf8dec_rs::decode_win_str(&buf, cp);
            debug!(
                "KID \"{}\" ({:02x}) - ({:04x} - {:04x}, {:04x}, {:04x})",
                s, id_type, ord, u1, u2, u3
            );
            s
        } else {
            let mut buf = vec![0u8; id_len.into()];
            f.read_exact(&mut buf)?;
            let s = utf8dec_rs::decode_win_str(&buf, cp);
            debug!("KID \"{}\" ({:04x}) - ({:04x})", s, id_type, ord);
            s
        };
        Ok((ord + 1, s))
    }

    fn get_user_string<R: Read>(f: &mut R, cp: u16) -> Result<String, io::Error> {
        let id_len = rdu8(f)?;
        let id_type = rdu8(f)?;

        if id_len == 0 && id_type == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Keyword found"));
        }

        Ok(if id_type & 0x80 != 0 {
            let u1 = rdu16le(f)?;
            let u2 = rdu16le(f)?;
            let u3 = rdu16le(f)?;
            let mut buf = vec![0u8; id_len.into()];
            f.read_exact(&mut buf)?;
            let u4 = rdu16le(f)?;
            let u5 = rdu16le(f)?;
            let s = utf8dec_rs::decode_win_str(&buf, cp);
            debug!(
                "ID \"{}\" ({:04x}) - ({:04x}, {:04x}, {:04x}, {:04x}, {:04x})",
                s,
                id_type & !0x80,
                u1,
                u2,
                u3,
                u4,
                u5
            );
            s
        } else {
            let mut buf = vec![0u8; id_len.into()];
            f.read_exact(&mut buf)?;
            let u4 = rdu16le(f)?;
            let u5 = rdu16le(f)?;
            let s = utf8dec_rs::decode_win_str(&buf, cp);
            debug!("ID \"{}\" ({:04x}) - ({:04x}, {:04x})", s, id_type, u4, u5);
            s
        })
    }

    fn get_sized_string<R: Read>(f: &mut R, len: u16) -> Result<String, io::Error> {
        let mut sb: Vec<u8> = vec![0u8; len.into()];
        f.read_exact(&mut sb)?;
        Ok(utf8dec_rs::decode_utf16le_str(&sb))
    }

    fn get_string<R: Read>(f: &mut R) -> Result<String, io::Error> {
        let len = rdu16le(f)?;
        Self::get_sized_string(f, len)
    }

    fn maybe_get_string<R: Read>(f: &mut R) -> Result<Option<String>, io::Error> {
        let len = rdu16le(f)?;
        if len == 0xffff {
            Ok(None)
        } else {
            Ok(Some(Self::get_sized_string(f, len)?))
        }
    }

    fn maybe_get_cp_string<R: Read>(f: &mut R, cp: u16) -> Result<Option<String>, io::Error> {
        let len = rdu16le(f)?;
        if len == 0xffff {
            Ok(None)
        } else {
            let mut sb: Vec<u8> = vec![0u8; len.into()];
            f.read_exact(&mut sb)?;
            Ok(Some(utf8dec_rs::decode_win_str(&sb, cp)))
        }
    }

    /// Upcast to generic project
    pub fn as_gen(&self) -> ProjectGeneric<'_> {
        ProjectGeneric::VD(self)
    }

    /// Version-dependent list of project *Modules*
    pub fn modules(&self) -> impl Iterator<Item = ModulePC<'_>> {
        self.modules_data.iter().map(|m| ModulePC {
            parent: self,
            data: m,
        })
    }
}

impl<'a> ProjectTrait<'a> for ProjectPC {
    fn sys_kind(&self) -> Option<u32> {
        Some(self.sys_kind)
    }
    fn lcid(&self) -> Option<u32> {
        Some(self.lcid)
    }
    fn lcid_invoke(&self) -> Option<u32> {
        Some(self.lcid_invoke)
    }
    fn codepage(&self) -> Option<u16> {
        Some(self.codepage)
    }
    fn name(&self) -> Option<&str> {
        self.get_string_from_table(self.name_idx)
    }
    fn docstring(&self) -> Option<&str> {
        self.docstring.as_deref()
    }
    fn help(&self) -> Option<&str> {
        self.help_file.as_deref()
    }
    fn help_context(&self) -> Option<u32> {
        Some(self.help_context)
    }
    fn lib_flags(&self) -> Option<u32> {
        Some(self.lib_flags)
    }
    fn version_major(&self) -> Option<u32> {
        Some(self.version_major)
    }
    fn version_minor(&self) -> Option<u16> {
        Some(self.version_minor)
    }
    fn constants(&'a self) -> Box<dyn Iterator<Item = (&'a str, i16)> + 'a> {
        Box::new(self.get_constants())
    }
    fn references(&self) -> &[dir::Reference] {
        &self.references
    }
    fn cookie(&self) -> Option<u16> {
        Some(self.cookie)
    }
}

#[derive(Debug)]
/// Version-dependent VBA Module metadata
pub struct ModuleData {
    // Could be name or name unicode (likely unicode)
    pub name: Option<String>,
    // Some gibberish id, no idea what it means
    pub id: Option<String>,
    // Could be docstring or docstring_unicode
    pub docstring: Option<String>,
    // A string_table index identifying the stream name
    pub stream_index: u16,
    // A flag of sorts
    pub non_procedural: bool,
    // Could be the non-unicode name (seemingly unused)
    pub name_again: Option<String>,
    // description/docstring possibly the non unicode version or maybe the help file
    pub docstring_again: Option<String>,
    pub cookie: u16,
    // Possibly help context
    pub help_context: u32,
    // unsure what this is, it's found in the first table (possibly u16)
    pub another_index_and_flags: u32,
    // unknown
    pub unk1: u8,
    // size of the cached portion of the module stream (i.e. the offset to the source portion)
    pub offset: u32,
    // unknown, usually 0xffff
    pub unk2: u16,

    /// Module flags
    ///
    /// | Bit | Meaning                               |
    /// |-----|---------------------------------------|
    /// |   0 | Procedural                            |
    /// |   1 | Non-procedural                        |
    /// |   2 | Unknown                               |
    /// |   3 | Causes error when source is viewed    |
    /// |   4 | Causes error when source is viewed    |
    /// |   5 | Read only                             |
    /// |   6 | Unknown                               |
    /// |   7 | Unknown                               |
    /// |   8 | Unknown                               |
    /// |   9 | Unknown                               |
    /// |  10 | Private                               |
    /// |  11 | Read-only (volatile, cleared on save) |
    /// |  12 | Unknown                               |
    /// |  13 | Unknown                               |
    /// |  14 | Unknown                               |
    /// |  15 | Unknown                               |
    ///
    pub flags: u16,
}

impl ModuleData {
    fn new<R: Read + Seek>(f: &mut R, is_5x: bool, flags: u16, cp: u16) -> Result<Self, io::Error> {
        let name = ProjectPC::maybe_get_string(f)?;
        let id = if is_5x {
            ProjectPC::maybe_get_cp_string(f, cp)?
        } else {
            ProjectPC::maybe_get_string(f)?
        };
        let docstring = if is_5x {
            let cnt = rdu16le(f)?;
            f.seek(io::SeekFrom::Current(cnt.into()))?;
            None
        } else {
            ProjectPC::maybe_get_string(f)?
        };
        let index_and_flags = rdu16le(f)?;
        let non_procedural = (index_and_flags & 1) != 0; // FIXME: is this an indication of the Document module ?
        let stream_index = index_and_flags;
        let name_again = ProjectPC::maybe_get_string(f)?;
        let docstring_again = if is_5x {
            None
        } else {
            ProjectPC::maybe_get_string(f)?
        };
        let cookie = rdu16le(f)?;
        let help_context = rdu32le(f)?;
        let cnt = rdu16le(f)?;
        // FIXME: this is at most present on the document module, it seems to contain indexes or -1
        f.seek(io::SeekFrom::Current(i64::from(cnt) * 8))?;
        let another_index_and_flags = rdu32le(f)?; // Unsure, could be a size or offset too
        let unk1 = rdu8(f)?; // always 0
        let offset = rdu32le(f)?;
        let unk2 = rdu16le(f)?; // mostly 0xffff
        Ok(Self {
            name,
            id,
            docstring,
            stream_index,
            non_procedural,
            name_again,
            docstring_again,
            cookie,
            help_context,
            another_index_and_flags,
            unk1,
            offset,
            unk2,
            flags,
        })
    }
}

#[derive(Debug)]
/// A wrapper for Version-dependent VBA Module metadata
///
/// see [`data`](Self::data)
pub struct ModulePC<'a> {
    parent: &'a ProjectPC,
    /// Version-dependent VBA Module metadata
    pub data: &'a ModuleData,
}

impl<'a> ModulePC<'a> {
    /// Upcast to generic module
    pub fn as_gen(self) -> ModuleGeneric<'a> {
        ModuleGeneric::VD(self)
    }
}

impl ModuleTrait for ModulePC<'_> {
    fn main_name(&self) -> Option<&str> {
        self.data.name.as_deref()
    }
    fn alt_name(&self) -> Option<&str> {
        self.data.name_again.as_deref()
    }
    fn main_stream_name(&self) -> Option<&str> {
        self.parent.get_string_from_table(self.data.stream_index)
    }
    fn alt_stream_name(&self) -> Option<&str> {
        None
    }
    fn main_docstring(&self) -> Option<&str> {
        self.data.docstring.as_deref()
    }
    fn alt_docstring(&self) -> Option<&str> {
        self.data.docstring_again.as_deref()
    }
    fn offset(&self) -> Option<u32> {
        Some(self.data.offset)
    }
    fn help_context(&self) -> Option<u32> {
        Some(self.data.help_context)
    }
    fn cookie(&self) -> Option<u16> {
        Some(self.data.cookie)
    }
    fn is_procedural(&self) -> bool {
        self.data.flags & 0b1 != 0
    }
    fn is_non_procedural(&self) -> bool {
        self.data.flags & 0b10 != 0
    }
    fn is_read_only(&self) -> bool {
        self.data.flags & 0b100000 != 0
    }
    fn is_private(&self) -> bool {
        self.data.flags & 0b10000000000 != 0
    }
}
