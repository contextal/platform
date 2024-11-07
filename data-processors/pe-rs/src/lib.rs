use chrono::{TimeZone, Utc};
use ctxutils::io::{rdu16le, rdu32le, rdu64le, rdu8};
use serde::Serialize;
use std::cmp::min;
use std::io::{Read, Seek, SeekFrom};

const DOS_HEADER_SIGNATURE: &[u8] = b"MZ";
const PE_FILE_SIGNATURE: &[u8] = b"PE\x00\x00";

#[allow(non_snake_case)]
#[derive(Serialize)]
/// PE File Header (right after the PE signature)
pub struct PeFileHeader {
    /// The type of target machine
    pub Machine: u16,
    /// Description of target machine (not an official field)
    pub MachineStr: &'static str,
    /// The number of sections in the executable
    pub NumberOfSections: u16,
    /// The time when the file was created (EPOCH)
    pub TimeDateStamp: u32,
    /// Time in text form (not an official field)
    pub TimeDateStampString: String,
    /// The file offset of the COFF symbol table, or zero if not present
    pub PointerToSymbolTable: u32,
    /// The number of entries in the symbol table
    pub NumberOfSymbols: u32,
    /// The size of the optional header (should be zero for an object file)
    pub SizeOfOptionalHeader: u16,
    /// The flags indicating the attributes of the file
    Characteristics: u16,
    /// Description of attributes (not an official field)
    pub CharacteristicsSymbols: Vec<&'static str>,
}

fn ph_machine_type(machine: u16) -> &'static str {
    match machine {
        0x0 => "Any",
        0x184 => "Alpha AXP",
        0x284 => "Alpha 64",
        0x1d3 => "Matsushita AM33",
        0x8664 => "x64",
        0x1c0 => "ARM LE",
        0xaa64 => "ARM64 LE",
        0x1c4 => "ARM Thumb-2 LE",
        0xebc => "EFI byte code",
        0x14c => "x86",
        0x200 => "Intel Itanium",
        0x6232 => "LoongArch 32-bit",
        0x6264 => "LoongArch 64-bit",
        0x9041 => "Mitsubishi M32R LE",
        0x266 => "MIPS16",
        0x366 => "MIPS FPU",
        0x466 => "MIPS16 FPU",
        0x1f0 => "Power PC LE",
        0x1f1 => "Power PC FPU",
        0x166 => "MIPS LE",
        0x5032 => "RISC-V 32-bit",
        0x5064 => "RISC-V 64-bit",
        0x5128 => "RISC-V 128-bit",
        0x1a2 => "Hitachi SH3",
        0x1a3 => "Hitachi SH3 DSP",
        0x1a6 => "Hitachi SH4",
        0x1a8 => "Hitachi SH5",
        0x1c2 => "Thumb",
        0x169 => "MIPS LE WCE v2",
        _ => "*** UNKNOWN ***",
    }
}

fn ph_characteristics(flags: u16) -> Vec<&'static str> {
    let mut ch: Vec<&'static str> = vec![];
    if flags & 0x0001 != 0 {
        ch.push("RELOCS_STRIPPED");
    }
    if flags & 0x0002 != 0 {
        ch.push("EXECUTABLE_IMAGE");
    }
    if flags & 0x0004 != 0 {
        ch.push("LINE_NUMS_STRIPPED");
    }
    if flags & 0x0008 != 0 {
        ch.push("LOCAL_SYMS_STRIPPED");
    }
    if flags & 0x0010 != 0 {
        ch.push("AGGRESSIVE_WS_TRIM");
    }
    if flags & 0x0020 != 0 {
        ch.push("LARGE_ADDRESS_AWARE");
    }
    if flags & 0x0040 != 0 {
        ch.push("RESERVED");
    }
    if flags & 0x0080 != 0 {
        ch.push("BYTES_REVERSED_LO");
    }
    if flags & 0x0100 != 0 {
        ch.push("32BIT_MACHINE");
    }
    if flags & 0x0200 != 0 {
        ch.push("DEBUG_STRIPPED");
    }
    if flags & 0x0400 != 0 {
        ch.push("REMOVABLE_RUN_FROM_SWAP");
    }
    if flags & 0x0800 != 0 {
        ch.push("NET_RUN_FROM_SWAP");
    }
    if flags & 0x1000 != 0 {
        ch.push("SYSTEM");
    }
    if flags & 0x2000 != 0 {
        ch.push("DLL");
    }
    if flags & 0x4000 != 0 {
        ch.push("UP_SYSTEM_ONLY");
    }
    if flags & 0x8000 != 0 {
        ch.push("BYTES_REVERSED_HI");
    }
    ch
}

impl PeFileHeader {
    fn new<R: Read>(mut r: R) -> Result<Self, std::io::Error> {
        let mut pe_file_sig = [0u8; 4];
        r.read_exact(&mut pe_file_sig)?;
        if pe_file_sig != PE_FILE_SIGNATURE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid PE file signature",
            ));
        }
        let mut valu16: u16;
        let timestamp: u32;
        Ok(Self {
            Machine: {
                valu16 = rdu16le(&mut r)?;
                valu16
            },
            MachineStr: ph_machine_type(valu16),
            NumberOfSections: rdu16le(&mut r)?,
            TimeDateStamp: {
                timestamp = rdu32le(&mut r)?;
                timestamp
            },
            TimeDateStampString: Utc.timestamp_opt(timestamp.into(), 0).unwrap().to_string(),
            PointerToSymbolTable: rdu32le(&mut r)?,
            NumberOfSymbols: rdu32le(&mut r)?,
            SizeOfOptionalHeader: rdu16le(&mut r)?,
            Characteristics: {
                valu16 = rdu16le(&mut r)?;
                valu16
            },
            CharacteristicsSymbols: ph_characteristics(valu16),
        })
    }
}

#[allow(non_snake_case)]
#[derive(Serialize)]
pub struct PeImageOptionalHeaderDataDir {
    pub VirtualAddress: u32,
    pub Size: u32,
}

#[allow(non_snake_case)]
#[derive(Serialize)]
pub struct PeImageOptionalHeader {
    /// The identifier of the image file (0x10b for regular exe, 0x107 for ROM image, 0x20b PE32+ exe)
    pub Magic: u16,
    /// Description of image file type (not an official field)
    pub MagicStr: &'static str,
    /// The linker major version number (not checked by the OS - can be any)
    pub MajorLinkerVersion: u8,
    /// The linker minor version number (not checked by the OS - can be any)
    pub MinorLinkerVersion: u8,
    /// The size of all code sections, unreliable - not checked
    pub SizeOfCode: u32,
    /// The size of all initialized data sections, unreliable - not checked
    pub SizeOfInitializedData: u32,
    /// The size of all uninitialized data sections (BSS), unreliable - not checked
    pub SizeOfUninitializedData: u32,
    /// The relative address of the entry point (optional for DLLs)
    pub AddressOfEntryPoint: u32,
    /// The relative address of the code section in the loaded image
    pub BaseOfCode: u32,
    /// The relative address of the uninitialized data in the loaded image (PE only, set to 0 in PE+)
    pub BaseOfData: u32,
    /// The preferred address of the beginning of image when loaded into memory - must be a multiple of 64 KB
    pub ImageBase: u64,
    /// The alignment of sections in memory (must be >= FileAlignment)
    pub SectionAlignment: u32,
    /// The alignment of sections in the image file (512-65535, pow of 2)
    pub FileAlignment: u32,
    /// The major version number of the required OS
    pub MajorOperatingSystemVersion: u16,
    /// The minor version number of the required OS
    pub MinorOperatingSystemVersion: u16,
    /// The major version number of the image
    pub MajorImageVersion: u16,
    /// The minor version number of the image
    pub MinorImageVersion: u16,
    /// The major version number of the subsystem
    pub MajorSubsystemVersion: u16,
    /// The minor version number of the subsystem
    pub MinorSubsystemVersion: u16,
    /// Reserved (must be 0)
    pub Win32VersionValue: u32,
    /// The size of image, including all headers (must be a multiple of SectionAlignment)
    pub SizeOfImage: u32,
    /// The combined size of DOS stub, PE header, and section headers rounded up to a multiple of FileAlignment
    pub SizeOfHeaders: u32,
    /// The image file checksum (checked for drivers, and DLLs loaded at boot time or into critical processes
    pub CheckSum: u32,
    /// The subsystem required to run the image
    pub Subsystem: u16,
    /// The description of subsystem (not an official field)
    pub SubsystemStr: &'static str,
    /// The characteristics of a DLL
    pub DllCharacteristics: u16,
    /// The size of the stack to reserve
    pub SizeOfStackReserve: u64,
    /// The size of the stack to commit
    pub SizeOfStackCommit: u64,
    /// The size of the local heap space to reserve
    pub SizeOfHeapReserve: u64,
    /// The size of the local heap space to commit
    pub SizeOfHeapCommit: u64,
    /// Reserved (must be 0)
    pub LoaderFlags: u32,
    /// The number of data-directory entries in the remainder of the optional header
    pub NumberOfRvaAndSizes: u32,
    /// Data directories
    pub DataDirectories: Vec<PeImageOptionalHeaderDataDir>,
}

fn oh_magic(magic: u16) -> &'static str {
    match magic {
        0x10b => "PE32",
        0x20b => "PE32+",
        _ => "*** UNKNOWN ***",
    }
}

fn oh_subsystem(subsystem: u16) -> &'static str {
    match subsystem {
        0 => "Unknown subsystem",
        1 => "Device drivers and native Windows processes",
        2 => "Windows GUI subsystem",
        3 => "Windows character subsystem",
        5 => "OS/2 character subsystem",
        7 => "Posix character subsystem",
        8 => "Native Win9x driver",
        9 => "Windows CE",
        10 => "EFI application",
        11 => "EFI boot driver",
        12 => "EFI runtime driver",
        13 => "EFI ROM image",
        14 => "XBOX",
        16 => "Windows boot application",
        _ => "*** UNKNOWN ***",
    }
}

impl PeImageOptionalHeader {
    fn new<R: Read>(mut r: R, magic: u16) -> Result<Self, std::io::Error> {
        let mut peplus = false;
        let rvas;
        let magicval: u16;
        let subsystem: u16;
        Ok(Self {
            Magic: match magic {
                0x10b => {
                    magicval = magic;
                    magic
                }
                0x20b => {
                    peplus = true;
                    magicval = magic;
                    magic
                }
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Invalid optional header magic number",
                    ));
                }
            },
            MagicStr: oh_magic(magicval),
            MajorLinkerVersion: rdu8(&mut r)?,
            MinorLinkerVersion: rdu8(&mut r)?,
            SizeOfCode: rdu32le(&mut r)?,
            SizeOfInitializedData: rdu32le(&mut r)?,
            SizeOfUninitializedData: rdu32le(&mut r)?,
            AddressOfEntryPoint: rdu32le(&mut r)?,
            BaseOfCode: rdu32le(&mut r)?,
            BaseOfData: match peplus {
                false => rdu32le(&mut r)?,
                true => 0,
            },
            ImageBase: match peplus {
                false => rdu32le(&mut r)?.into(),
                true => rdu64le(&mut r)?,
            },
            SectionAlignment: rdu32le(&mut r)?,
            FileAlignment: rdu32le(&mut r)?,
            MajorOperatingSystemVersion: rdu16le(&mut r)?,
            MinorOperatingSystemVersion: rdu16le(&mut r)?,
            MajorImageVersion: rdu16le(&mut r)?,
            MinorImageVersion: rdu16le(&mut r)?,
            MajorSubsystemVersion: rdu16le(&mut r)?,
            MinorSubsystemVersion: rdu16le(&mut r)?,
            Win32VersionValue: rdu32le(&mut r)?,
            SizeOfImage: rdu32le(&mut r)?,
            SizeOfHeaders: rdu32le(&mut r)?,
            CheckSum: rdu32le(&mut r)?,
            Subsystem: {
                subsystem = rdu16le(&mut r)?;
                subsystem
            },
            SubsystemStr: oh_subsystem(subsystem),
            DllCharacteristics: rdu16le(&mut r)?,
            SizeOfStackReserve: match peplus {
                false => rdu32le(&mut r)?.into(),
                true => rdu64le(&mut r)?,
            },
            SizeOfStackCommit: match peplus {
                false => rdu32le(&mut r)?.into(),
                true => rdu64le(&mut r)?,
            },
            SizeOfHeapReserve: match peplus {
                false => rdu32le(&mut r)?.into(),
                true => rdu64le(&mut r)?,
            },
            SizeOfHeapCommit: match peplus {
                false => rdu32le(&mut r)?.into(),
                true => rdu64le(&mut r)?,
            },
            LoaderFlags: rdu32le(&mut r)?,
            NumberOfRvaAndSizes: {
                let realrvas = rdu32le(&mut r)?;
                rvas = min(realrvas, 16);
                realrvas
            },
            DataDirectories: {
                let mut dd = Vec::new();
                for _ in 0..rvas {
                    let va = rdu32le(&mut r)?;
                    let size = rdu32le(&mut r)?;
                    dd.push(PeImageOptionalHeaderDataDir {
                        VirtualAddress: va,
                        Size: size,
                    });
                }
                dd
            },
        })
    }

    pub fn pe_type(&self) -> &str {
        oh_magic(self.Magic)
    }

    pub fn subsystem(&self) -> &str {
        oh_subsystem(self.Subsystem)
    }
}

#[allow(non_snake_case)]
#[derive(Serialize)]
pub struct PeSectionHeader {
    /// Section name
    pub Name: String,
    /// The total size of the section when loaded into memory (when > SizeOfRawData, the section is zero-padded
    pub VirtualSize: u32,
    /// The address of the first byte of the section relative to the image base when the section is loaded into memory
    pub VirtualAddress: u32,
    /// The size of the section (must be a multiple of FileAlignment, if < VirtualSize, the remainder is zero-filled
    pub SizeOfRawData: u32,
    /// The file pointer to the first page of the section within the COFF file
    pub PointerToRawData: u32,
    /// The file pointer to the beginning of relocation entries for the section
    pub PointerToRelocations: u32,
    /// The file pointer to the beginning of line-number entries for the section
    pub PointerToLinenumbers: u32,
    /// The number of relocation entries for the section
    pub NumberOfRelocations: u16,
    /// The number of line-number entries for the section
    pub NumberOfLinenumbers: u16,
    /// The flags that describe the characteristics of the section
    pub Characteristics: u32,
    /// The description of section attributes (not an official field)
    pub CharacteristicsSymbols: Vec<&'static str>,
}

fn sh_characteristics(flags: u32) -> Vec<&'static str> {
    let mut ch: Vec<&'static str> = vec![];
    if flags & 0x00000001 != 0 {
        ch.push("RES_1");
    }
    if flags & 0x00000002 != 0 {
        ch.push("RES_2");
    }
    if flags & 0x00000004 != 0 {
        ch.push("RES_3");
    }
    if flags & 0x00000008 != 0 {
        ch.push("TYPE_NO_PAD");
    }
    if flags & 0x00000010 != 0 {
        ch.push("RES_5");
    }
    if flags & 0x00000020 != 0 {
        ch.push("CNT_CODE");
    }
    if flags & 0x00000040 != 0 {
        ch.push("CNT_INITIALIZED_DATA");
    }
    if flags & 0x00000080 != 0 {
        ch.push("CNT_UNINITIALIZED_DATA");
    }
    if flags & 0x00000100 != 0 {
        ch.push("LNK_OTHER");
    }
    if flags & 0x00000200 != 0 {
        ch.push("LNK_INFO");
    }
    if flags & 0x00000400 != 0 {
        ch.push("RES_6");
    }
    if flags & 0x00000800 != 0 {
        ch.push("LNK_REMOVE");
    }
    if flags & 0x00001000 != 0 {
        ch.push("LNK_COMDAT");
    }
    if flags & 0x00008000 != 0 {
        ch.push("GPREL");
    }
    if flags & 0x00020000 != 0 {
        ch.push("MEM_PURGEABLE");
    }
    if flags & 0x00040000 != 0 {
        ch.push("MEM_LOCKED");
    }
    if flags & 0x00080000 != 0 {
        ch.push("MEM_PRELOAD");
    }
    if flags & 0x01000000 != 0 {
        ch.push("LNK_NRELOC_OVFL");
    }
    if flags & 0x02000000 != 0 {
        ch.push("MEM_DISCARDABLE");
    }
    if flags & 0x04000000 != 0 {
        ch.push("MEM_NOT_CACHED");
    }
    if flags & 0x08000000 != 0 {
        ch.push("MEM_NOT_PAGED");
    }
    if flags & 0x10000000 != 0 {
        ch.push("MEM_SHARED");
    }
    if flags & 0x20000000 != 0 {
        ch.push("MEM_EXECUTE");
    }
    if flags & 0x40000000 != 0 {
        ch.push("MEM_READ");
    }
    if flags & 0x80000000 != 0 {
        ch.push("MEM_WRITE");
    }
    ch
}

impl PeSectionHeader {
    fn new<R: Read>(mut r: R) -> Result<Self, std::io::Error> {
        let valu32: u32;
        Ok(Self {
            Name: {
                let mut buf = [0u8; 8];
                r.read_exact(&mut buf)?;
                let len = buf.iter().position(|c| *c == b'\0').unwrap_or(buf.len());
                String::from_utf8_lossy(&buf[0..len]).to_string()
            },
            VirtualSize: rdu32le(&mut r)?,
            VirtualAddress: rdu32le(&mut r)?,
            SizeOfRawData: rdu32le(&mut r)?,
            PointerToRawData: rdu32le(&mut r)?,
            PointerToRelocations: rdu32le(&mut r)?,
            PointerToLinenumbers: rdu32le(&mut r)?,
            NumberOfRelocations: rdu16le(&mut r)?,
            NumberOfLinenumbers: rdu16le(&mut r)?,
            Characteristics: {
                valu32 = rdu32le(&mut r)?;
                valu32
            },
            CharacteristicsSymbols: sh_characteristics(valu32),
        })
    }
}

#[derive(Serialize)]
pub struct PE {
    /// The PE file header
    pub pe_header: PeFileHeader,

    /// The "optional" header
    pub optional_header: PeImageOptionalHeader,

    /// The section headers
    pub section_headers: Vec<PeSectionHeader>,

    /// Potential issues detected
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

impl PE {
    /// Parses the PE file and returns its structure or an error
    pub fn new<R: Read + Seek>(mut r: R) -> Result<Self, std::io::Error> {
        let mut issues: Vec<String> = vec![];
        let mut dos_header_sig = [0u8; 2];
        r.read_exact(&mut dos_header_sig)?;
        if dos_header_sig != DOS_HEADER_SIGNATURE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid DOS header signature",
            ));
        }

        r.seek(SeekFrom::Current(58))?;
        let e_lfanew = rdu32le(&mut r)?;
        if (e_lfanew + 20) as u64 > r.seek(SeekFrom::End(0))? {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No valid PE file header data found",
            ));
        }
        r.seek(SeekFrom::Start(e_lfanew.into()))?;

        let pe_header = PeFileHeader::new(&mut r)?;
        if pe_header.Characteristics & 0x0010 != 0 {
            issues.push("PH_AGGRESSIVE_WS_TRIM_SET".to_string());
        }
        if pe_header.Characteristics & 0x0040 != 0 {
            issues.push("PH_RESERVED_SET".to_string());
        }
        /* Deprecated flag but still common, so silencing it
         * if pe_header.Characteristics & 0x8000 != 0 {
         *   issues.push("PH_BYTES_REVERSED_HI_SET".to_string());
         * }
         */
        if pe_header.NumberOfSections > 96 {
            issues.push("PH_TOO_MANY_SECTIONS".to_string());
        }
        if pe_header.TimeDateStamp as i64 > Utc::now().timestamp() {
            issues.push("PH_FUTURE_TIMESTAMP".to_string());
        }

        let opthdr_magic = rdu16le(&mut r)?;
        // TODO: bail out on 0x107 (ROM image)
        let opthdr = PeImageOptionalHeader::new(&mut r, opthdr_magic)?;
        if opthdr.Win32VersionValue != 0 {
            issues.push("OH_WIN32VERSIONVALUE_SET".to_string());
        }
        if opthdr.ImageBase % 65536 > 0 {
            issues.push("OH_IMAGEBASE_BADVAL".to_string());
        }
        if opthdr.SectionAlignment < opthdr.FileAlignment {
            issues.push("OH_SECTIONALIGNMENT_BADVAL".to_string());
        }
        if opthdr.FileAlignment < 512
            || opthdr.FileAlignment > 65536
            || opthdr.FileAlignment % 512 != 0
        {
            issues.push("OH_FILEALIGNMENT_BADVAL".to_string());
        }
        if opthdr.SizeOfImage % opthdr.SectionAlignment != 0 {
            issues.push("OH_SIZEOFIMAGE_BADVAL".to_string());
        }
        if opthdr.SizeOfHeaders % opthdr.FileAlignment != 0 {
            issues.push("OH_SIZEOFHEADERS_BADVAL".to_string());
        }
        if opthdr.LoaderFlags != 0 {
            issues.push("OH_LOADERFLAGS_SET".to_string());
        }
        if opthdr.NumberOfRvaAndSizes > 16 {
            issues.push("OH_TOO_MANY_DATADIRS".to_string());
        }

        let mut section_headers = Vec::new();
        for i in 1..pe_header.NumberOfSections + 1 {
            let sh = PeSectionHeader::new(&mut r)?;
            if sh.Characteristics & 0x00000001 != 0 {
                issues.push(format!("SH{}_RES_1_SET", i));
            }
            if sh.Characteristics & 0x00000002 != 0 {
                issues.push(format!("SH{}_RES_2_SET", i));
            }
            if sh.Characteristics & 0x00000004 != 0 {
                issues.push(format!("SH{}_RES_3_SET", i));
            }
            if sh.Characteristics & 0x00000008 != 0 {
                issues.push(format!("SH{}_TYPE_NO_PAD_SET", i));
            }
            if sh.Characteristics & 0x00000010 != 0 {
                issues.push(format!("SH{}_RES_5_SET", i));
            }
            if sh.Characteristics & 0x00000100 != 0 {
                issues.push(format!("SH{}_LNK_OTHER_SET", i));
            }
            if sh.Characteristics & 0x00000200 != 0 {
                issues.push(format!("SH{}_LNK_INFO_SET", i));
            }
            if sh.Characteristics & 0x00000400 != 0 {
                issues.push(format!("SH{}_RES_6_SET", i));
            }
            if sh.Characteristics & 0x00000800 != 0 {
                issues.push(format!("SH{}_LNK_REMOVE_SET", i));
            }
            if sh.Characteristics & 0x00001000 != 0 {
                issues.push(format!("SH{}_LNK_COMDAT_SET", i));
            }
            if sh.Characteristics & 0x00020000 != 0 {
                issues.push(format!("SH{}_MEM_PURGEABLE_SET", i));
            }
            if sh.Characteristics & 0x00040000 != 0 {
                issues.push(format!("SH{}_MEM_LOCKED_SET", i));
            }
            if sh.Characteristics & 0x00080000 != 0 {
                issues.push(format!("SH{}_MEM_PRELOAD_SET", i));
            }
            section_headers.push(sh);
        }

        Ok(Self {
            pe_header,
            optional_header: opthdr,
            section_headers,
            issues,
        })
    }
}
