use serde::Serialize;
#[allow(unused_imports)]
use std::io::{BufRead, BufReader, Read, Seek};

const MH_MAGIC: u32 = 0xfeedface;
const MH_CIGAM: u32 = 0xcefaedfe;
const MH_MAGIC_64: u32 = 0xfeedfacf;
const MH_CIGAM_64: u32 = 0xcffaedfe;

fn rdu32<R: Read>(r: &mut R, le: bool) -> Result<u32, std::io::Error> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    match le {
        true => Ok(u32::from_le_bytes(buf)),
        false => Ok(u32::from_be_bytes(buf)),
    }
}

fn rdu64<R: Read>(r: &mut R, le: bool) -> Result<u64, std::io::Error> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    match le {
        true => Ok(u64::from_le_bytes(buf)),
        false => Ok(u64::from_be_bytes(buf)),
    }
}

/// Mach-O header
#[derive(Serialize)]
pub struct MachOHeader {
    /// Magic number
    pub magic: u32,
    /// CPU type
    pub cputype: u32,
    /// Description of CPU type (not an official field)
    pub cputypestr: &'static str,
    /// CPU subtype
    pub cpusubtype: u32,
    /// File type
    pub filetype: u32,
    /// Description of file type (not an official field)
    pub filetypestr: &'static str,
    /// Number of load commands
    pub ncmds: u32,
    /// Size of all load commands
    pub sizeofcmds: u32,
    /// Flags
    pub flags: u32,
    /// Description of flags
    pub flagsvec: Vec<&'static str>,
}

fn mh_cputype(cputype: u32) -> &'static str {
    match cputype {
        1 => "VAX",
        6 => "MC680x0",
        7 => "I386",
        8 => "MIPS",
        10 => "MC98000",
        11 => "HPPA",
        12 => "ARM",
        13 => "MC88000",
        14 => "SPARC",
        15 => "I860",
        16 => "ALPHA",
        18 => "POWERPC",
        0x01000012 => "POWERPC64",
        0x01000007 => "X86_64",
        0x0100000c => "ARM64",
        0x0200000c => "ARM64_32",
        0xffffffff => "ANY",
        _ => "*** UNKNOWN ***",
    }
}

fn mh_filetype(ftype: u32) -> &'static str {
    match ftype {
        0x1 => "OBJECT",
        0x2 => "EXECUTE",
        0x3 => "FVMLIB",
        0x4 => "CORE",
        0x5 => "PRELOAD",
        0x6 => "DYLIB",
        0x7 => "DYLINKER",
        0x8 => "BUNDLE",
        0x9 => "DYLIB_STUB",
        0xa => "DSYM",
        0xb => "KEXT_BUNDLE",
        0xc => "FILESET",
        0xd => "GPU_EXECUTE",
        0xe => "GPU_DYLIB",
        _ => "*** UNKNOWN ***",
    }
}

fn mh_flags(flags: u32) -> Vec<&'static str> {
    let mut f: Vec<&'static str> = vec![];

    if flags & 0x1 > 0 {
        f.push("NOUNDEFS");
    }
    if flags & 0x2 > 0 {
        f.push("INCRLINK");
    }
    if flags & 0x4 > 0 {
        f.push("DYLDLINK");
    }
    if flags & 0x8 > 0 {
        f.push("BINDATLOAD");
    }
    if flags & 0x10 > 0 {
        f.push("PREBOUND");
    }
    if flags & 0x20 > 0 {
        f.push("SPLIT_SEGS");
    }
    if flags & 0x40 > 0 {
        f.push("LAZY_INIT");
    }
    if flags & 0x80 > 0 {
        f.push("TWOLEVEL");
    }
    if flags & 0x100 > 0 {
        f.push("FORCE_FLAT");
    }
    if flags & 0x200 > 0 {
        f.push("NOMULTIDEFS");
    }
    if flags & 0x400 > 0 {
        f.push("NOFIXPREBINDING");
    }
    if flags & 0x800 > 0 {
        f.push("PREBINDABLE");
    }
    if flags & 0x1000 > 0 {
        f.push("ALLMODSBOUND");
    }
    if flags & 0x2000 > 0 {
        f.push("SUBSECTIONS_VIA_SYMBOLS");
    }
    if flags & 0x4000 > 0 {
        f.push("CANONICAL");
    }
    if flags & 0x8000 > 0 {
        f.push("WEAK_DEFINES");
    }
    if flags & 0x10000 > 0 {
        f.push("BINDS_TO_WEAK");
    }
    if flags & 0x20000 > 0 {
        f.push("ALLOW_STACK_EXECUTION");
    }
    if flags & 0x40000 > 0 {
        f.push("ROOT_SAFE");
    }
    if flags & 0x80000 > 0 {
        f.push("SETUID_SAFE");
    }
    if flags & 0x100000 > 0 {
        f.push("NO_REEXPORTED_DYLIBS");
    }
    if flags & 0x200000 > 0 {
        f.push("PIE");
    }
    if flags & 0x400000 > 0 {
        f.push("DEAD_STRIPPABLE_DYLIB");
    }
    if flags & 0x800000 > 0 {
        f.push("HAS_TLV_DESCRIPTORS");
    }
    if flags & 0x1000000 > 0 {
        f.push("NO_HEAP_EXECUTION");
    }
    if flags & 0x2000000 > 0 {
        f.push("APP_EXTENSION_SAFE");
    }
    if flags & 0x4000000 > 0 {
        f.push("NLIST_OUTOFSYNC_WITH_DYLDINFO");
    }
    if flags & 0x8000000 > 0 {
        f.push("SIM_SUPPORT");
    }
    if flags & 0x80000000 > 0 {
        f.push("DYLIB_IN_CACHE");
    }
    f
}

impl MachOHeader {
    fn new<R: Read>(mut r: R, magic: u32, le: bool, arch64: bool) -> Result<Self, std::io::Error> {
        let cputype = rdu32(&mut r, le)?;
        let cpusubtype = rdu32(&mut r, le)?;
        let filetype = rdu32(&mut r, le)?;
        let ncmds = rdu32(&mut r, le)?;
        let sizeofcmds = rdu32(&mut r, le)?;
        let flags = rdu32(&mut r, le)?;
        if arch64 {
            let _reserved = rdu32(&mut r, le)?;
        }

        Ok(Self {
            magic,
            cputype,
            cputypestr: mh_cputype(cputype),
            cpusubtype,
            filetype,
            filetypestr: mh_filetype(filetype),
            ncmds,
            sizeofcmds,
            flags,
            flagsvec: mh_flags(flags),
        })
    }
}

fn lc_type(cmd: u32) -> &'static str {
    match cmd {
        0x1 => "SEGMENT",
        0x2 => "SYMTAB",
        0x3 => "SYMSEG",
        0x4 => "THREAD",
        0x5 => "UNIXTHREAD",
        0x6 => "LOADFVMLIB",
        0x7 => "IDFVMLIB",
        0x8 => "IDENT",
        0x9 => "FVMFILE",
        0xa => "PREPAGE",
        0xb => "DYSYMTAB",
        0xc => "LOAD_DYLIB",
        0xd => "ID_DYLIB",
        0xe => "LOAD_DYLINKER",
        0xf => "ID_DYLINKER",
        0x10 => "PREBOUND_DYLIB",
        0x11 => "ROUTINES",
        0x12 => "SUB_FRAMEWORK",
        0x13 => "SUB_UMBRELLA",
        0x14 => "SUB_CLIENT",
        0x15 => "SUB_LIBRARY",
        0x16 => "TWOLEVEL_HINTS",
        0x17 => "PREBIND_CKSUM",
        0x80000018 => "LOAD_WEAK_DYLIB",
        0x19 => "SEGMENT_64",
        0x1a => "ROUTINES_64",
        0x1b => "UUID",
        0x8000001c => "RPATH",
        0x1d => "CODE_SIGNATURE",
        0x1e => "SEGMENT_SPLIT_INFO",
        0x8000001f => "REEXPORT_DYLIB",
        0x20 => "LAZY_LOAD_DYLIB",
        0x21 => "ENCRYPTION_INFO",
        0x22 => "DYLD_INFO",
        0x80000022 => "DYLD_INFO_ONLY",
        0x80000023 => "LOAD_UPWARD_DYLIB",
        0x24 => "VERSION_MIN_MACOSX",
        0x25 => "VERSION_MIN_IPHONEOS",
        0x26 => "FUNCTION_STARTS",
        0x27 => "DYLD_ENVIRONMENT",
        0x80000028 => "MAIN",
        0x29 => "DATA_IN_CODE",
        0x2A => "SOURCE_VERSION",
        0x2B => "DYLIB_CODE_SIGN_DRS",
        0x2C => "ENCRYPTION_INFO_64",
        0x2D => "LINKER_OPTION",
        0x2E => "LINKER_OPTIMIZATION_HINT",
        0x2F => "VERSION_MIN_TVOS",
        0x30 => "VERSION_MIN_WATCHOS",
        0x31 => "NOTE",
        0x32 => "BUILD_VERSION",
        0x80000033 => "DYLD_EXPORTS_TRIE",
        0x80000034 => "DYLD_CHAINED_FIXUPS",
        0x80000035 => "FILESET_ENTRY",
        0x36 => "ATOM_INFO",
        _ => "*** UNKNOWN ***",
    }
}

#[derive(Serialize)]
/// Load command structure (load commands directly follow the mach-o header)
pub struct LoadCmd {
    /// Type of load command
    pub cmd: u32,
    /// Description of load command (not an official field)
    pub cmdstr: &'static str,
    /// Total size of load command
    pub cmdsize: u32,
}

#[derive(Serialize)]
/// Segment load command structure
pub struct SegmentCmd {
    /// Segment name
    pub segname: String,
    /// Memory address of the segment
    pub vmaddr: u64,
    /// Memory size of the segment
    pub vmsize: u64,
    /// File offset of the segment
    pub fileoff: u64,
    /// Amount to map from the file
    pub filesize: u64,
    /// Maximum VM protection
    pub maxprot: u32,
    /// Initial VM protection
    pub initprot: u32,
    /// Number of sections in the segment
    pub nsects: u32,
    /// Flags
    pub flags: u32,
}

impl SegmentCmd {
    pub fn new<R: Read>(mut r: R, le: bool, cmd: u32) -> Result<Self, std::io::Error> {
        let mut s64 = true;

        if cmd == 0x1 {
            s64 = false;
        }

        Ok(Self {
            segname: {
                let mut buf = [0u8; 16];
                r.read_exact(&mut buf)?;
                String::from_utf8_lossy(&buf)
                    .trim_end_matches(char::from(0))
                    .to_string()
            },
            vmaddr: match s64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)? as u64,
            },
            vmsize: match s64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)? as u64,
            },
            fileoff: match s64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)? as u64,
            },
            filesize: match s64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)? as u64,
            },
            maxprot: rdu32(&mut r, le)?,
            initprot: rdu32(&mut r, le)?,
            nsects: rdu32(&mut r, le)?,
            flags: rdu32(&mut r, le)?,
        })
    }
}

#[derive(Serialize)]
/// Section structure
pub struct Section {
    /// Section name
    pub sectname: String,
    /// Segment this section goes in
    pub segname: String,
    /// Memory address of the section
    pub addr: u64,
    /// Size of the section
    pub size: u64,
    /// File offset of the section
    pub offset: u32,
    /// Section alignment
    pub align: u32,
    /// File offset of relocation entriesw
    pub reloff: u32,
    /// Number of relocation entries
    pub nreloc: u32,
    /// Flags
    pub flags: u32,
    /// Reserved
    pub reserved1: u32,
    /// Reserved
    pub reserved2: u32,
    /// Reserved
    pub reserved3: u32,
}

impl Section {
    pub fn new<R: Read>(mut r: R, le: bool, cmd: u32) -> Result<Self, std::io::Error> {
        let mut s64 = true;

        if cmd == 0x1 {
            s64 = false;
        }

        Ok(Self {
            sectname: {
                let mut buf = [0u8; 16];
                r.read_exact(&mut buf)?;
                String::from_utf8_lossy(&buf)
                    .trim_end_matches(char::from(0))
                    .to_string()
            },
            segname: {
                let mut buf = [0u8; 16];
                r.read_exact(&mut buf)?;
                String::from_utf8_lossy(&buf)
                    .trim_end_matches(char::from(0))
                    .to_string()
            },
            addr: match s64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)? as u64,
            },
            size: match s64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)? as u64,
            },
            offset: rdu32(&mut r, le)?,
            align: rdu32(&mut r, le)?,
            reloff: rdu32(&mut r, le)?,
            nreloc: rdu32(&mut r, le)?,
            flags: rdu32(&mut r, le)?,
            reserved1: rdu32(&mut r, le)?,
            reserved2: rdu32(&mut r, le)?,
            reserved3: match s64 {
                true => rdu32(&mut r, le)?,
                false => 0,
            },
        })
    }
}

#[derive(Serialize)]
pub struct MachO {
    /// The Mach-O header
    pub macho_header: MachOHeader,

    /// Load commands
    pub load_cmds: Vec<LoadCmd>,

    /// Segment load commands
    pub segment_cmds: Vec<SegmentCmd>,

    /// Sections
    pub sections: Vec<Section>,
}

impl MachO {
    pub fn new<R: Read + Seek>(mut r: R) -> Result<Self, std::io::Error> {
        let mut le = true; // we just use the flag to determine the right conversion order, not for the actual endianness
        let mut arch64 = true;

        let magic = rdu32(&mut r, le)?;
        match magic {
            MH_MAGIC => {
                arch64 = false;
            }
            MH_CIGAM => {
                arch64 = false;
                le = false;
            }
            MH_MAGIC_64 => (),
            MH_CIGAM_64 => {
                le = false;
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Not a Mach-O file: Invalid magic number",
                ))
            }
        }

        let macho_header = MachOHeader::new(&mut r, magic, le, arch64)?;

        // parse load commands
        let mut load_cmds: Vec<LoadCmd> = vec![];
        let mut segment_cmds: Vec<SegmentCmd> = vec![];
        let mut sections: Vec<Section> = vec![];
        for _ in 0..macho_header.ncmds {
            let cmd = rdu32(&mut r, le)?;
            let cmdstr = lc_type(cmd);
            let cmdsize = rdu32(&mut r, le)?;
            load_cmds.push(LoadCmd {
                cmd,
                cmdstr,
                cmdsize,
            });

            if cmd == 0x1 || cmd == 0x19 {
                let sc = SegmentCmd::new(&mut r, le, cmd)?;
                for _ in 0..sc.nsects {
                    sections.push(Section::new(&mut r, le, cmd)?);
                }
                segment_cmds.push(sc);
            } else {
                r.seek(std::io::SeekFrom::Current((cmdsize - 8) as i64))?;
            }
        }

        Ok(Self {
            macho_header,
            load_cmds,
            segment_cmds,
            sections,
        })
    }
}
