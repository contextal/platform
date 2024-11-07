use crate::decomp::CompressContainerReader;
use crate::*;
use ctxutils::{cmp::*, io::*, win32::GUID};
use std::io::{self, Read};

#[derive(Debug, Default)]
/// Version-independent VBA Project info (`dir` stream)
///
/// All the records in the `dir` stream are parsed and made available here
/// conveniently grouped
pub struct Project {
    /// Version-independent information
    pub info: ProjectInfo,
    /// External references
    pub references: Vec<Reference>,
    /// The modules in the project
    pub modules: Vec<Module>,
}

#[derive(Debug, Default)]
/// Version-independent information of the VBA project (`PROJECTINFORMATION` record)
///
/// # Notes
/// The parser is intentionally loose (i.e. non compliant) in a few different aspects:
/// 1. the specs mandate a strict order in the record sequence, however since Office
///    doesn't honor this rule, neither does this parser
/// 2. Office appears to disregard some of these fields (e.g. the `PROJECTCODEPAGE`
///    record) to a point where it becomes hard to decide what's important and what is
///    not; therefore this implementation extracts all the fields it finds and makes
///    them available for convenience, statistical analysis, IoCs, etc...
/// 3. although the presence and type of each record is strictly defined in the specs,
///    Office, as per the previous point, does survive certain missing or incorrectly
///    formatted types; therefore everything in here is wrapped in an [`Option`] which
///    is set to [`None`] if the relevant record is missing or ill formatted
pub struct ProjectInfo {
    /// The platform for which the VBA project is created
    ///
    /// | Value      | Meaning                       |
    /// |------------|-------------------------------|
    /// | 0x00000000 | For 16-bit Windows Platforms. |
    /// | 0x00000001 | For 32-bit Windows Platforms. |
    /// | 0x00000002 | For Macintosh Platforms.      |
    /// | 0x00000003 | For 64-bit Windows Platforms. |
    ///
    pub sys_kind: Option<u32>,
    /// The Office Model version used by the project
    pub compat_version: Option<u32>,
    /// The project language (`LCID`)
    pub lcid: Option<u32>,
    /// The language (`LCID`) of the *Automation Server*
    pub lcid_invoke: Option<u32>,
    /// The project code page
    pub codepage: Option<u16>,
    /// The name of the project
    pub name: Option<String>,
    /// The project description (code page variant)
    pub docstring: Option<String>,
    /// The project description (UTF-16 variant)
    pub docstring_unicode: Option<String>,
    /// The path to the project Help file (variant 1)
    pub help1: Option<String>,
    /// The path to the project Help file (variant 2)
    pub help2: Option<String>,
    /// The Help topic identifier in the Help file
    pub help_context: Option<u32>,
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
    pub lib_flags: Option<u32>,
    /// The major version of the project
    pub version_major: Option<u32>,
    /// The minor version of the project
    pub version_minor: Option<u16>,
    /// The compilation constants for the project (code page variant)
    pub constants: Option<String>,
    /// The compilation constants for the project (UTF-16 variant)
    pub constants_unicode: Option<String>,
    /// *Documented* as: "MUST be ignored on read. MUST be 0xFFFF on write."
    pub cookie: Option<u16>,
}

impl Project {
    pub(crate) fn new<R: Read>(f: &mut R, size: u64) -> Result<Self, io::Error> {
        let mut ret = Self::default();
        let mut f = CompressContainerReader::new(f, size)?;

        // In theory this is a list of records and subrecords in strict order,
        // however office doesn't care much about that and appears to simply
        // treat this stream as an array of k/v pairs

        let mut record_id = rdu16le(&mut f)?;
        loop {
            match record_id {
                0x0001 /* PROJECTSYSKIND */ =>
                    set_u32_or_skip(&mut f, &mut ret.info.sys_kind)?,
                0x004a /* PROJECTCOMPATVERSION */ =>
                    set_u32_or_skip(&mut f, &mut ret.info.compat_version)?,
                0x0002 /* PROJECTLCID */ =>
                    set_u32_or_skip(&mut f, &mut ret.info.lcid)?,
                0x0014 /* PROJECTLCIDINVOKE */ =>
                    set_u32_or_skip(&mut f, &mut ret.info.lcid_invoke)?,
                0x0003 /* PROJECTCODEPAGE */ =>
                    set_u16_or_skip(&mut f, &mut ret.info.codepage)?,
                0x0004 /* PROJECTNAME */ =>
                    set_string_or_skip(&mut f, &mut ret.info.name, StrEnc::CP(ret.info.codepage), 128)?,
                0x0005 /* PROJECTDOCSTRING (1/2) */ => {
                    set_string_or_skip(&mut f, &mut ret.info.docstring, StrEnc::CP(ret.info.codepage), 2000)?;
                    skip(&mut f, 2)?; // Should be 0x0040, but it should be ignored
                    record_id = 0x040;
                    continue;
                }
                0x0040 /* PROJECTDOCSTRING (2/2) */ =>
                    set_string_or_skip(&mut f, &mut ret.info.docstring_unicode, StrEnc::UTF16, 2000 * 2)?,
                0x0006 /* PROJECTHELPFILEPATH (1/2) */ => {
                    set_string_or_skip(&mut f, &mut ret.info.help1, StrEnc::CP(ret.info.codepage), 260)?;
                    skip(&mut f, 2)?; // Should be 0x003d, but it should be ignored
                    record_id = 0x03d;
                    continue;
                }
                0x003d /* PROJECTHELPFILEPATH (2/2) */ =>
                    set_string_or_skip(&mut f, &mut ret.info.help2, StrEnc::CP(ret.info.codepage), 260)?,
                0x0007 /* PROJECTHELPCONTEXT */ =>
                    set_u32_or_skip(&mut f, &mut ret.info.help_context)?,
                0x0008 /* PROJECTLIBFLAGS */ =>
                    set_u32_or_skip(&mut f, &mut ret.info.lib_flags)?,
                0x0009 /* PROJECTVERSION */ => {
                    skip(&mut f, 4)?; // Should be 0x00000004, but it should be ignored
                    let maj = rdu32le(&mut f)?;
                    let min = rdu16le(&mut f)?;
                    if ret.info.version_major.is_none() {
                        ret.info.version_major = Some(maj);
                    }
                    if ret.info.version_minor.is_none() {
                        ret.info.version_minor = Some(min);
                    }
                }
                0x000c /* PROJECTCONSTANTS (1/2) */ => {
                    set_string_or_skip(&mut f, &mut ret.info.constants, StrEnc::CP(ret.info.codepage), 1015)?;
                    skip(&mut f, 2)?; // Should be 0x003c, but it should be ignored
                    record_id = 0x03c;
                    continue;
                }
                0x003c /* PROJECTCONSTANTS (1/2) */ =>
                    set_string_or_skip(&mut f, &mut ret.info.constants_unicode, StrEnc::UTF16, 1015 * 2)?,
                0x0016 | 0x0033 | 0x000e | 0x000f =>
                    break,
                _ => skip_this(&mut f)?,
            };
            record_id = rdu16le(&mut f)?;
        }

        // REFERENCEs
        loop {
            let mut name: Option<String> = None;
            let mut name_unicode: Option<String> = None;
            if record_id == 0x0016 {
                set_string_or_skip(&mut f, &mut name, StrEnc::CP(ret.info.codepage), 2000)?;
                skip(&mut f, 2)?; // Should be 0x003e, but it should be ignored
                set_string_or_skip(&mut f, &mut name_unicode, StrEnc::UTF16, 2000 * 2)?;
                record_id = rdu16le(&mut f)?;
            }
            match record_id {
                0x0033 /* REFERENCEORIGINAL (+REFERENCECONTROL) */  => {
                    let original = ReferenceOriginal::new(&mut f, ret.info.codepage)?;
                    record_id = rdu16le(&mut f)?;
                    if record_id != 0x002f {
                        ret.references.push(
                            Reference {
                                name,
                                name_unicode,
                                value: ReferenceValue::Original(original),
                            }
                        );
                        continue;
                    } else {
                        ret.references.push(
                            Reference {
                                name,
                                name_unicode,
                                value: ReferenceValue::Control(
                                    ReferenceControl::new(&mut f, original, ret.info.codepage)?
                                ),
                            }
                        );
                    }
                }
                0x000d /* REFERENCEREGISTERED */ => {
                    ret.references.push(
                        Reference {
                            name,
                            name_unicode,
                            value: ReferenceValue::Registered(
                                ReferenceRegistered::new(&mut f, ret.info.codepage)?
                            ),
                        }
                    );
                }
                0x000e /* REFERENCEPROJECT */ => {
                    ret.references.push(
                        Reference {
                            name,
                            name_unicode,
                            value: ReferenceValue::Project(
                                ReferenceProject::new(&mut f, ret.info.codepage)?
                            ),
                        }
                    );
                }
                0x000f => break,
                _ => skip_this(&mut f)?,
            }
            record_id = rdu16le(&mut f)?;
        }

        let mut nmodules: Option<u16> = None;
        set_u16_or_skip(&mut f, &mut nmodules)?;
        if rdu16le(&mut f)? == 0x0013 {
            // PROJECTCOOKIE
            set_u16_or_skip(&mut f, &mut ret.info.cookie)?;
        } else {
            skip(&mut f, 6)?;
        }
        if let Some(nmodules) = nmodules {
            for _ in 0..nmodules {
                ret.modules.push(Module::new(&mut f, ret.info.codepage)?);
            }
        }
        // The Terminator (id 0x0010) is ignored
        Ok(ret)
    }

    /// Upcast to generic Project
    pub fn as_gen(&self) -> ProjectGeneric<'_> {
        ProjectGeneric::VI(self)
    }

    /// Version-independent list of project *Modules*
    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.iter()
    }
}

impl<'a> ProjectTrait<'a> for Project {
    fn sys_kind(&self) -> Option<u32> {
        self.info.sys_kind
    }
    fn lcid(&self) -> Option<u32> {
        self.info.lcid
    }
    fn lcid_invoke(&self) -> Option<u32> {
        self.info.lcid_invoke
    }
    fn codepage(&self) -> Option<u16> {
        self.info.codepage
    }
    fn name(&self) -> Option<&str> {
        self.info.name.as_deref()
    }
    fn docstring(&self) -> Option<&str> {
        [&self.info.docstring_unicode, &self.info.docstring]
            .into_iter()
            .flatten()
            .next()
            .map(|s| s.as_str())
    }
    fn help(&self) -> Option<&str> {
        self.info.help1.as_deref()
    }
    fn help_context(&self) -> Option<u32> {
        self.info.help_context
    }
    fn lib_flags(&self) -> Option<u32> {
        self.info.lib_flags
    }
    fn version_major(&self) -> Option<u32> {
        self.info.version_major
    }
    fn version_minor(&self) -> Option<u16> {
        self.info.version_minor
    }
    fn constants(&'a self) -> Box<dyn Iterator<Item = (&'a str, i16)> + 'a> {
        Box::new(
            [&self.info.constants_unicode, &self.info.constants]
                .into_iter()
                .flatten()
                .next()
                .map(|s| s.as_str())
                .unwrap_or("")
                .split(':')
                .flat_map(|x| x.split_once('='))
                .flat_map(|(k, v)| Some((k.trim(), v.trim().parse::<i16>().ok()?))),
        )
    }
    fn references(&self) -> &[Reference] {
        &self.references
    }
    fn cookie(&self) -> Option<u16> {
        self.info.cookie
    }
}

#[derive(Debug)]
/// External references of the VBA project (`PROJECTREFERENCES` record)
pub struct Reference {
    /// The name of the referenced VBA project or *Automation type library* (code page variant)
    pub name: Option<String>,
    /// The name of the referenced VBA project or *Automation type library* (UTF-16 variant)
    pub name_unicode: Option<String>,
    /// The content of the reference
    pub value: ReferenceValue,
}

impl ReferenceControl {
    fn new<R: Read>(
        f: &mut R,
        original: ReferenceOriginal,
        cp: Option<u16>,
    ) -> Result<Self, io::Error> {
        let mut ret = ReferenceControl {
            original,
            ..Self::default()
        };
        skip(f, 4)?; // Should be total size, but it should be ignored
        set_string_or_skip(f, &mut ret.twiddled, StrEnc::CP(cp), 2000)?;
        skip(f, 6)?; // Reserved
        if rdu16le(f)? == 0x0016 {
            set_string_or_skip(f, &mut ret.record_name, StrEnc::CP(cp), 2000)?;
            skip(f, 2)?; // Should be 0x003e, but it should be ignored
            set_string_or_skip(f, &mut ret.record_name_unicode, StrEnc::UTF16, 2000 * 2)?;
            skip(f, 6)?; // Should be 0x0030 + total size, but it should be ignored
        } else {
            // REFERENCENAME is optional, 0x0030 was already consumed
            skip(f, 4)?; // Should be total size, but it should be ignored
        }
        set_string_or_skip(f, &mut ret.libid, StrEnc::CP(cp), 2000)?;
        skip(f, 6)?; // Reserved
        let mut guidbuf = [0u8; 16];
        f.read_exact(&mut guidbuf)?;
        ret.guid = GUID::from_le_bytes(&guidbuf).unwrap();
        ret.cookie = rdu32le(f)?;
        Ok(ret)
    }
}

impl ReferenceOriginal {
    fn new<R: Read>(f: &mut R, cp: Option<u16>) -> Result<Self, io::Error> {
        let mut ret = Self::default();
        set_string_or_skip(f, &mut ret.libid_original, StrEnc::CP(cp), 2000)?;
        Ok(ret)
    }
}

impl ReferenceRegistered {
    fn new<R: Read>(f: &mut R, cp: Option<u16>) -> Result<Self, io::Error> {
        let mut ret = Self::default();
        skip(f, 4)?; // Should be total size, but it should be ignored
        set_string_or_skip(f, &mut ret.libid, StrEnc::CP(cp), 2000)?;
        skip(f, 6)?; // Reserved
        Ok(ret)
    }
}

impl ReferenceProject {
    fn new<R: Read>(f: &mut R, cp: Option<u16>) -> Result<Self, io::Error> {
        let mut ret = Self::default();
        skip(f, 4)?; // Should be total size, but it should be ignored
        set_string_or_skip(f, &mut ret.absolute, StrEnc::CP(cp), 2000)?;
        set_string_or_skip(f, &mut ret.relative, StrEnc::CP(cp), 2000)?;
        ret.version_major = rdu32le(f)?;
        ret.version_minor = rdu16le(f)?;
        Ok(ret)
    }
}

#[derive(Debug, Default)]
/// Version-independent VBA Module metadata (`PROJECTMODULES` record)
///
/// Fields can be accessed directly or through the provided [`ModuleTrait`] interface
///
/// Please review the [`ProjectInfo` notes](ProjectInfo#notes) for details about
/// the parser and field types
pub struct Module {
    /// The name of the module (code page variant)
    pub name: Option<String>,
    /// The name of the module (UTF-16 variant)
    pub name_unicode: Option<String>,
    /// The name of the Ole stream that contains the module (code page variant)
    pub stream: Option<String>,
    /// The name of the Ole stream that contains the module (UTF-16 variant)
    pub stream_unicode: Option<String>,
    /// The module description (code page variant)
    pub docstring: Option<String>,
    /// The module description (UTF-16 variant)
    pub docstring_unicode: Option<String>,
    /// Stat offset in the Ole stream
    pub offset: Option<u32>,
    /// The Help topic identifier in the Help file
    pub help_context: Option<u32>,
    /// To be ignored, usually 0xffff
    pub cookie: Option<u16>,
    /// Indicates whether this module is marked as a procedural module
    pub procedural: bool,
    /// Indicates whether this module is marked as a document, class module or designer module
    pub non_procedural: bool,
    /// Indicates that this module is marked as read-only
    pub read_only: bool,
    /// Indicates that this module is marked as private
    pub private: bool,
}

impl Module {
    fn new<R: Read>(f: &mut R, cp: Option<u16>) -> Result<Self, io::Error> {
        let mut ret = Self::default();
        let mut record_id = rdu16le(f)?;
        loop {
            match record_id {
                0x0019 /* MODULENAME */ =>
                    set_string_or_skip(f, &mut ret.name, StrEnc::CP(cp), 2000)?,
                0x0047 /* MODULENAMEUNICODE */ =>
                    set_string_or_skip(f, &mut ret.name_unicode, StrEnc::UTF16, 2000*2)?,
                0x001a /* MODULESTREAMNAME (1/2) */ => {
                    set_string_or_skip(f, &mut ret.stream, StrEnc::CP(cp), 64)?;
                    skip(f, 2)?; // Should be 0x0032, but it should be ignored
                    record_id = 0x0032;
                    continue;
                }
                0x0032 /* MODULESTREAMNAME (2/2) */  =>
                    set_string_or_skip(f, &mut ret.stream_unicode, StrEnc::UTF16, 64*2)?,
                0x001c /* MODULEDOCSTRING (1/2) */ => {
                    set_string_or_skip(f, &mut ret.docstring, StrEnc::CP(cp), 2000)?;
                    skip(f, 2)?; // Should be 0x0048, but it should be ignored
                    record_id = 0x0048;
                    continue;
                }
                0x0048 /* MODULEDOCSTRING (2/2) */  =>
                    set_string_or_skip(f, &mut ret.docstring_unicode, StrEnc::UTF16, 64*2)?,
                0x0031 /* MODULEOFFSET */ =>
                    set_u32_or_skip(f, &mut ret.offset)?,
                0x001e /* MODULEHELPCONTEXT */ =>
                set_u32_or_skip(f, &mut ret.help_context)?,
                0x002c /* MODULECOOKIE */ =>
                    set_u16_or_skip(f, &mut ret.cookie)?,
                0x0021 /* MODULETYPE (procedural module) */ => {
                    skip_this(f)?;
                    ret.procedural = true;
                }
                0x0022 /* MODULETYPE (document, class or designer module) */ => {
                    skip_this(f)?;
                    ret.non_procedural = true;
                }
                0x0025 /* MODULEREADONLY */ => {
                    skip_this(f)?;
                    ret.read_only = true;
                }
                0x0028 /* MODULEPRIVATE */ => {
                    skip_this(f)?;
                    ret.private = true;
                }
                0x002B /* Terminator */ => {
                    skip_this(f)?;
                    break;
                }
                _ => skip_this(f)?,
            }
            record_id = rdu16le(f)?;
        }
        Ok(ret)
    }

    /// Upcast to generic module
    pub fn as_gen(&self) -> ModuleGeneric<'_> {
        ModuleGeneric::VI(self)
    }
}

impl ModuleTrait for &Module {
    fn main_name(&self) -> Option<&str> {
        self.name_unicode.as_deref()
    }
    fn alt_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn main_stream_name(&self) -> Option<&str> {
        self.stream_unicode.as_deref()
    }
    fn alt_stream_name(&self) -> Option<&str> {
        self.stream.as_deref()
    }
    fn main_docstring(&self) -> Option<&str> {
        self.docstring_unicode.as_deref()
    }
    fn alt_docstring(&self) -> Option<&str> {
        self.docstring.as_deref()
    }
    fn offset(&self) -> Option<u32> {
        self.offset
    }
    fn help_context(&self) -> Option<u32> {
        self.help_context
    }
    fn cookie(&self) -> Option<u16> {
        self.cookie
    }
    fn is_procedural(&self) -> bool {
        self.procedural
    }
    fn is_non_procedural(&self) -> bool {
        self.non_procedural
    }
    fn is_read_only(&self) -> bool {
        self.read_only
    }
    fn is_private(&self) -> bool {
        self.private
    }
}

// Dir record parsing helpers

fn set_u32_or_skip<R: Read>(f: &mut R, p: &mut Option<u32>) -> Result<(), io::Error> {
    let len = rdu32le(f)?;
    if len == 4 && p.is_none() {
        *p = Some(rdu32le(f)?);
    } else {
        skip(f, len)?;
    }
    Ok(())
}

fn set_u16_or_skip<R: Read>(f: &mut R, p: &mut Option<u16>) -> Result<(), io::Error> {
    let len = rdu32le(f)?;
    if len == 2 && p.is_none() {
        *p = Some(rdu16le(f)?);
    } else {
        skip(f, len)?;
    }
    Ok(())
}

enum StrEnc {
    UTF16,
    CP(Option<u16>),
}

fn set_string_or_skip<R: Read>(
    f: &mut R,
    p: &mut Option<String>,
    cp: StrEnc,
    maxlen: u32,
) -> Result<(), io::Error> {
    let total_len = rdu32le(f)?;
    if p.is_none() {
        let used_len = total_len.min(umin(maxlen, usize::MAX));
        let mut buf = vec![0u8; used_len as usize]; // Cast is ok due to min/umin above
        f.read_exact(&mut buf)?;
        skip(f, total_len - used_len)?;
        *p = Some(match cp {
            StrEnc::UTF16 => utf8dec_rs::decode_utf16le_str(&buf),
            StrEnc::CP(Some(v)) => utf8dec_rs::decode_win_str(&buf, v),
            StrEnc::CP(None) => utf8dec_rs::decode_win_str(&buf, 1252),
        });
    } else {
        skip(f, total_len)?;
    }
    Ok(())
}

fn skip_this<R: Read>(f: &mut R) -> Result<(), io::Error> {
    let len = rdu32le(f)?;
    skip(f, len)?;
    Ok(())
}

fn skip<R: Read>(f: &mut R, skip_len: u32) -> Result<(), io::Error> {
    io::copy(&mut f.take(skip_len.into()), &mut io::sink())?;
    Ok(())
}
