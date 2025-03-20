use serde::Serialize;
use std::io::{BufRead, BufReader, Read, Seek};

const ELF_HEADER_SIGNATURE: &[u8] = b"\x7f\x45\x4c\x46";

fn rdu16<R: Read>(r: &mut R, le: bool) -> Result<u16, std::io::Error> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf)?;
    match le {
        true => Ok(u16::from_le_bytes(buf)),
        false => Ok(u16::from_be_bytes(buf)),
    }
}

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

/// ELF Header (right after the ELF magic number)
#[derive(Serialize)]
pub struct ELFHeader {
    /// File identification data
    e_ident: [u8; 16],
    /// Class description (not an official field)
    pub ei_class: &'static str,
    /// Endianness description (not an official field)
    pub ei_data: &'static str,
    /// OS ABI description (not an official field)
    pub ei_osabi: &'static str,
    /// Object file type
    pub e_type: u16,
    /// Description of object file type (not an official field)
    pub e_typestr: &'static str,
    /// Target architecture
    pub e_machine: u16,
    /// Description of target architecture (not an official field)
    pub e_machinestr: &'static str,
    /// ELF format version
    pub e_version: u32,
    /// Entry point address
    pub e_entry: u64,
    /// Offset to the header table
    pub e_phoff: u64,
    /// Offset to the section header table
    pub e_shoff: u64,
    /// Flags (architecture dependant)
    pub e_flags: u32,
    /// Size of this header
    pub e_ehsize: u16,
    /// Size of a program header table
    pub e_phentsize: u16,
    /// Number of entries in the program header table
    pub e_phnum: u16,
    /// Size of a section header table entry
    pub e_shentsize: u16,
    /// Number of entries in the section header table
    pub e_shnum: u16,
    /// Index of the section header table entry containing the section names
    pub e_shstrndx: u16,
}

fn eh_os_abi(abi: u8) -> &'static str {
    match abi {
        0x00 => "No extensions",
        0x01 => "HP-UX",
        0x02 => "NetBSD",
        0x03 => "Linux",
        0x04 => "GNU Hurd",
        0x06 => "Solaris",
        0x07 => "AIX",
        0x08 => "IRIX",
        0x09 => "FreeBSD",
        0x0A => "Tru64",
        0x0B => "Novell Modesto",
        0x0C => "OpenBSD",
        0x0D => "OpenVMS",
        0x0E => "NonStop Kernel",
        0x0F => "AROS",
        0x10 => "FenixOS",
        0x11 => "Nuxi CloudABI",
        0x12 => "OpenVOS",
        _ => "*** UNKNOWN ***",
    }
}

fn eh_object_type(object: u16) -> &'static str {
    match object {
        0x00 => "No type",
        0x01 => "Relocatable",
        0x02 => "Executable",
        0x03 => "Shared object",
        0x04 => "Core",
        0xFE00 => "OS specific (LOOS)",
        0xFEFF => "OS specific (HIOS)",
        0xFF00 => "Processor specific (LOPROC)",
        0xFFFF => "Processor specific (HIPROC)",
        _ => "*** UNKNOWN ***",
    }
}

fn eh_machine_type(machine: u16) -> &'static str {
    match machine {
        0x00 => "No specific instruction set",
        0x01 => "AT&T WE 32100",
        0x02 => "SPARC",
        0x03 => "x86",
        0x04 => "Motorola 68000 (M68k)",
        0x05 => "Motorola 88000 (M88k)",
        0x06 => "Intel MCU",
        0x07 => "Intel 80860",
        0x08 => "MIPS",
        0x09 => "IBM System/370",
        0x0A => "MIPS RS3000 Little-endian",
        0x0B => "Reserved",
        0x0C => "Reserved",
        0x0D => "Reserved",
        0x0E => "Reserved",
        0x0F => "Hewlett-Packard PA-RISC",
        0x10 => "Reserved",
        0x11 => "Fujitsu VPP500",
        0x12 => "SPARC Version 8+",
        0x13 => "Intel 80960",
        0x14 => "PowerPC",
        0x15 => "PowerPC (64-bit)",
        0x16 => "S390, including S390x",
        0x17 => "IBM SPU/SPC",
        0x18 => "Reserved",
        0x19 => "Reserved",
        0x20 => "Reserved",
        0x21 => "Reserved",
        0x22 => "Reserved",
        0x23 => "Reserved",
        0x24 => "NEC V800",
        0x25 => "Fujitsu FR20",
        0x26 => "TRW RH-32",
        0x27 => "Motorola RCE",
        0x28 => "Arm (up to Armv7/AArch32)",
        0x29 => "Digital Alpha",
        0x2A => "SuperH",
        0x2B => "SPARC Version 9",
        0x2C => "Siemens TriCore embedded",
        0x2D => "Argonaut RISC Core",
        0x2E => "Hitachi H8/300",
        0x2F => "Hitachi H8/300H",
        0x30 => "Hitachi H8S",
        0x31 => "Hitachi H8/500",
        0x32 => "IA-64",
        0x33 => "Stanford MIPS-X",
        0x34 => "Motorola ColdFire",
        0x35 => "Motorola M68HC12",
        0x36 => "Fujitsu MMA Multimedia Accelerator",
        0x37 => "Siemens PCP",
        0x38 => "Sony nCPU embedded RISC",
        0x39 => "Denso NDR1",
        0x3A => "Motorola Star*Core",
        0x3B => "Toyota ME16 processor",
        0x3C => "STMicroelectronics ST100",
        0x3D => "Advanced Logic Corp. TinyJ",
        0x3E => "AMD x86-64",
        0x3F => "Sony DSP Processor",
        0x40 => "Digital Equipment Corp. PDP-10",
        0x41 => "Digital Equipment Corp. PDP-11",
        0x42 => "Siemens FX66 microcontroller",
        0x43 => "STMicroelectronics ST9+ 8/16-bit",
        0x44 => "STMicroelectronics ST7 8-bit",
        0x45 => "Motorola MC68HC16 Microcontroller",
        0x46 => "Motorola MC68HC11 Microcontroller",
        0x47 => "Motorola MC68HC08 Microcontroller",
        0x48 => "Motorola MC68HC05 Microcontroller",
        0x49 => "Silicon Graphics SVx",
        0x4A => "STMicroelectronics ST19 8-bit",
        0x4B => "Digital VAX",
        0x4C => "Axis Communications 32-bit embedded",
        0x4D => "Infineon Technologies 32-bit embedded",
        0x4E => "Element 14 64-bit DSP",
        0x4F => "LSI Logic 16-bit DSP",
        0x50 => "Donald Knuth's educational 64-bit processor",
        0x51 => "Harvard University machine-independent object files",
        0x52 => "SiTera Prism",
        0x53 => "Atmel AVR 8-bit",
        0x54 => "Fujitsu FR30",
        0x55 => "Mitsubishi D10V",
        0x56 => "Mitsubishi D30V",
        0x57 => "NEC v850",
        0x58 => "Mitsubishi M32R",
        0x59 => "Matsushita MN10300",
        0x5A => "Matsushita MN10200",
        0x5B => "picoJava",
        0x5C => "OpenRISC 32-bit embedded",
        0x5D => "ARC International ARCompact",
        0x5E => "Tensilica Xtensa Architecture",
        0x5F => "Alphamosaic VideoCore",
        0x60 => "Thompson Multimedia GPP",
        0x61 => "National Semiconductor 32000",
        0x62 => "Tenor Network TPC",
        0x63 => "Trebia SNP 1000",
        0x64 => "STMicroelectronics ST200",
        0x65 => "Ubicom IP2xxx",
        0x66 => "MAX Processor",
        0x67 => "National Semiconductor CompactRISC",
        0x68 => "Fujitsu F2MC16",
        0x69 => "Texas Instruments msp430",
        0x6A => "Analog Devices Blackfin DSP",
        0x6B => "Seiko Epson S1C33",
        0x6C => "Sharp embedded",
        0x6D => "Arca RISC",
        0x6E => "PKU-Unity Ltd. and MPRC",
        0x6F => "eXcess 16/32/64-bit embedded CPU",
        0x70 => "Icera Semiconductor Inc. DEP",
        0x71 => "Altera Nios II",
        0x72 => "National Semiconductor CompactRISC CRX",
        0x73 => "Motorola XGATE",
        0x74 => "Infineon C16x/XC16x",
        0x75 => "Renesas M16C",
        0x76 => "Microchip Technology dsPIC30F DSC",
        0x77 => "Freescale Communication Engine RISC",
        0x78 => "Renesas M32C",
        0x79 => "Reserved",
        0x80 => "Reserved",
        0x81 => "Reserved",
        0x82 => "Reserved",
        0x83 => "Altium TSK3000 core",
        0x84 => "Freescale RS08",
        0x85 => "Analog Devices SHARC 32-bit DSP",
        0x86 => "Cyan Technology eCOG2",
        0x87 => "Sunplus S+core7 RIS",
        0x88 => "New Japan Radio 24-bit DSP",
        0x89 => "Broadcom VideoCore III",
        0x8A => "RISC Lattice FPGA",
        0x8B => "Seiko Epson C17",
        0x8C => "Texas Instruments TMS320C6000",
        0x8D => "Texas Instruments TMS320C2000 DSP",
        0x8E => "Texas Instruments TMS320C55x DSP",
        0x8F => "Texas Instruments RISC 32bit",
        0x90 => "Texas Instruments PRU",
        0x91 => "Reserved",
        0x92 => "Reserved",
        0x93 => "Reserved",
        0x94 => "Reserved",
        0x95 => "Reserved",
        0x96 => "Reserved",
        0x97 => "Reserved",
        0x98 => "Reserved",
        0x99 => "Reserved",
        0x9A => "Reserved",
        0x9B => "Reserved",
        0x9C => "Reserved",
        0x9D => "Reserved",
        0x9E => "Reserved",
        0x9F => "Reserved",
        0xA0 => "STMicroelectronics 64bit VLIW DSP",
        0xA1 => "Cypress M8C",
        0xA2 => "Renesas R32C",
        0xA3 => "NXP Semiconductors TriMedia",
        0xA4 => "QUALCOMM DSP6",
        0xA5 => "Intel 8051",
        0xA6 => "STMicroelectronics STxP7x",
        0xA7 => "Andes Technology embedded RISC",
        0xA8 => "Cyan Technology eCOG1X",
        0xA9 => "Dallas Semiconductor MAXQ30",
        0xAA => "New Japan Radio 16-bit DSP",
        0xAB => "M2000 Reconfigurable RISC",
        0xAC => "Cray Inc. NV2",
        0xAD => "Renesas RX",
        0xAE => "Imagination Technologies META",
        0xAF => "MCST Elbrus e2k",
        0xB0 => "Cyan Technology eCOG16",
        0xB1 => "CompactRISC CR16",
        0xB2 => "Freescale Extended TPU",
        0xB3 => "Infineon Technologies SLE9X",
        0xB4 => "Intel L10M",
        0xB5 => "Intel K10M",
        0xB6 => "Reserved (Intel)",
        0xB7 => "Arm 64-bits (Armv8/AArch64)",
        0xB8 => "Reserved (Arm)",
        0xB9 => "Atmel 32-bit",
        0xBA => "STMicroeletronics STM8",
        0xBB => "Tilera TILE64",
        0xBC => "Tilera TILEPro",
        0xBD => "Xilinx MicroBlaze 32-bit RISC",
        0xBE => "NVIDIA CUDA",
        0xBF => "Tilera TILE-Gx",
        0xC0 => "CloudShield",
        0xC1 => "KIPO-KAIST Core-A 1st gen",
        0xC2 => "KIPO-KAIST Core-A 2nd gen",
        0xC3 => "Synopsys ARCompact V2",
        0xC4 => "Open8 8-bit RISC",
        0xC5 => "Renesas RL78",
        0xC6 => "Broadcom VideoCore V",
        0xC7 => "Renesas 78KOR",
        0xC8 => "Freescale 56800EX DSC",
        0xC9 => "Beyond BA1",
        0xCA => "Beyond BA2",
        0xCB => "XMOS xCORE",
        0xCC => "Microchip 8-bit",
        0xCD => "Reserved (Intel)",
        0xCE => "Reserved (Intel)",
        0xCF => "Reserved (Intel)",
        0xD0 => "Reserved (Intel)",
        0xD1 => "Reserved (Intel)",
        0xD2 => "KM211 KM32",
        0xD3 => "KM211 KMX32",
        0xD4 => "KM211 KMX16",
        0xD5 => "KM211 KMX8",
        0xD6 => "KM211 KVARC",
        0xD7 => "Paneve CDP",
        0xD8 => "Cognitive Smart Memory Processor",
        0xD9 => "Bluechip Systems CoolEngine",
        0xDA => "Nanoradio Optimized RISC",
        0xDB => "CSR Kalimba",
        0xDC => "Zilog Z80",
        0xDD => "VISIUM",
        0xDE => "FTDI FT32",
        0xDF => "Moxie",
        0xE0 => "AMD GPU",
        0xF3 => "RISC-V",
        0xF7 => "Berkeley Packet Filter",
        0x101 => "WDC 65C816",
        _ => "*** UNKNOWN ***",
    }
}

impl ELFHeader {
    fn new<R: Read>(mut r: R) -> Result<Self, std::io::Error> {
        let mut e_ident = [0u8; 16];
        let mut le = true;
        r.read_exact(&mut e_ident)?;
        if &e_ident[0..4] != ELF_HEADER_SIGNATURE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid ELF header signature",
            ));
        }
        let ei_class;
        let x64 = match e_ident[4] {
            1 => {
                ei_class = "32-bit";
                false
            }
            2 => {
                ei_class = "64-bit";
                true
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid class specification",
                ));
            }
        };
        if e_ident[5] == 2 {
            le = false;
        };

        let mut valu16: u16;
        Ok(Self {
            e_ident,
            ei_class,
            ei_data: match le {
                true => "Little-endian",
                false => "Big-endian",
            },
            ei_osabi: eh_os_abi(e_ident[7]),
            e_type: {
                valu16 = rdu16(&mut r, le)?;
                valu16
            },
            e_typestr: eh_object_type(valu16),
            e_machine: {
                valu16 = rdu16(&mut r, le)?;
                valu16
            },
            e_machinestr: eh_machine_type(valu16),
            e_version: rdu32(&mut r, le)?,
            e_entry: match x64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)?.into(),
            },
            e_phoff: match x64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)?.into(),
            },
            e_shoff: match x64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)?.into(),
            },
            e_flags: rdu32(&mut r, le)?,
            e_ehsize: rdu16(&mut r, le)?,
            e_phentsize: rdu16(&mut r, le)?,
            e_phnum: rdu16(&mut r, le)?,
            e_shentsize: rdu16(&mut r, le)?,
            e_shnum: rdu16(&mut r, le)?,
            e_shstrndx: rdu16(&mut r, le)?,
        })
    }

    pub fn is_x64(&self) -> bool {
        matches!(self.e_ident[4], 2)
    }

    pub fn is_le(&self) -> bool {
        matches!(self.e_ident[5], 1)
    }
}

/// ELF Header (right after the ELF magic number)
#[derive(Serialize)]
pub struct ELFProgramHeader {
    /// Segment type
    pub p_type: u32,
    /// Description of segment type (not an official field)
    pub p_typestr: &'static str,
    /// Segment flags
    pub p_flags: u32,
    /// Description of segment flags (not an official field)
    pub p_flagsvec: Vec<&'static str>,
    /// Offset of the segment in the file image
    pub p_offset: u64,
    /// Offset of the segment in memory
    pub p_vaddr: u64,
    /// Segment's physical address (on some systems)
    pub p_paddr: u64,
    /// Size of the segment in the file image
    pub p_filesz: u64,
    /// Size of the segment in memory
    pub p_memsz: u64,
    /// Alignment (0/1 = no alignment, otherwise 2^n)
    pub p_align: u64,
}

fn ph_segment_type(segment: u32) -> &'static str {
    match segment {
        0x0 => "Unused",
        0x1 => "Loadable segment",
        0x2 => "Dynamic linking information",
        0x3 => "Interpreter information",
        0x4 => "Auxiliary information",
        0x5 => "Reserved",
        0x6 => "Program header table",
        0x7 => "Thread-Local Storage",
        0x60000000..=0x6FFFFFFF => "OS specific",
        0x70000000..=0x7FFFFFFF => "Processor specific",
        _ => "*** UNKNOWN ***",
    }
}

fn ph_segment_flags(flags: u32) -> Vec<&'static str> {
    let mut f: Vec<&'static str> = vec![];
    if flags & 0x4 > 0 {
        f.push("READ");
    }
    if flags & 0x2 > 0 {
        f.push("WRITE");
    }
    if flags & 0x1 > 0 {
        f.push("EXECUTE");
    }
    f
}

impl ELFProgramHeader {
    fn new<R: Read>(mut r: R, x64: bool, le: bool) -> Result<Self, std::io::Error> {
        let (p_type, p_flags): (u32, u32);
        let (p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_align): (
            u64,
            u64,
            u64,
            u64,
            u64,
            u64,
        );

        if x64 {
            p_type = rdu32(&mut r, le)?;
            p_flags = rdu32(&mut r, le)?;
            p_offset = rdu64(&mut r, le)?;
            p_vaddr = rdu64(&mut r, le)?;
            p_paddr = rdu64(&mut r, le)?;
            p_filesz = rdu64(&mut r, le)?;
            p_memsz = rdu64(&mut r, le)?;
            p_align = rdu64(&mut r, le)?;
        } else {
            p_type = rdu32(&mut r, le)?;
            p_offset = rdu32(&mut r, le)?.into();
            p_vaddr = rdu32(&mut r, le)?.into();
            p_paddr = rdu32(&mut r, le)?.into();
            p_filesz = rdu32(&mut r, le)?.into();
            p_memsz = rdu32(&mut r, le)?.into();
            p_flags = rdu32(&mut r, le)?;
            p_align = rdu32(&mut r, le)?.into();
        }

        Ok(Self {
            p_type,
            p_typestr: ph_segment_type(p_type),
            p_flags,
            p_flagsvec: ph_segment_flags(p_flags),
            p_offset,
            p_vaddr,
            p_paddr,
            p_filesz,
            p_memsz,
            p_align,
        })
    }
}

/// ELF section header structure
#[derive(Serialize)]
pub struct ELFSectionHeader {
    /// Index into section header string table
    pub sh_name: u32,
    /// Section name (not an official field)
    pub sh_namestr: String,
    /// Section type
    pub sh_type: u32,
    /// Section type description (not an official field)
    pub sh_typestr: &'static str,
    /// Section flags
    pub sh_flags: u64,
    /// Description of flags (not an official field)
    pub sh_flagsvec: Vec<&'static str>,
    /// Section address in memory
    pub sh_addr: u64,
    /// Section offset in the file
    pub sh_offset: u64,
    /// Section size (in bytes)
    pub sh_size: u64,
    /// Section header table index link (section header table index link)
    pub sh_link: u32,
    /// Extra information (depends on section's type)
    pub sh_info: u32,
    /// Address alignment constraints (0/1 = no alignment, powers of two otherwise)
    pub sh_addralign: u64,
    /// Size of an entry, for sections that hold a table of fixed-size entries
    pub sh_entsize: u64,
}

fn section_type(sh_type: u32) -> &'static str {
    match sh_type {
        0x0 => "NULL",
        0x1 => "PROGBITS",
        0x2 => "SYMTAB",
        0x3 => "STRTAB",
        0x4 => "RELA",
        0x5 => "HASH",
        0x6 => "DYNAMIC",
        0x7 => "NOTE",
        0x8 => "NOBITS",
        0x9 => "REL",
        0x0A => "SHLIB",
        0x0B => "DYNSYM",
        0x0E => "INIT_ARRAY",
        0x0F => "FINI_ARRAY",
        0x10 => "PREINIT_ARRAY",
        0x11 => "GROUP",
        0x12 => "SYMTAB_SHNDX",
        0x13 => "NUM",
        0x60000000..=0x6FFFFFFF => "LOOS",
        0x70000000..=0x7FFFFFFF => "LOPROC",
        _ => "*** UKNOWN ***",
    }
}

fn section_flags(sh_flags: u64) -> Vec<&'static str> {
    let mut f: Vec<&'static str> = vec![];
    if sh_flags & 0x1 > 0 {
        f.push("WRITE");
    }
    if sh_flags & 0x2 > 0 {
        f.push("ALLOC");
    }
    if sh_flags & 0x4 > 0 {
        f.push("EXEC");
    }
    if sh_flags & 0x10 > 0 {
        f.push("MERGE");
    }
    if sh_flags & 0x20 > 0 {
        f.push("STRINGS");
    }
    if sh_flags & 0x40 > 0 {
        f.push("INFO_LINK");
    }
    if sh_flags & 0x80 > 0 {
        f.push("LINK_ORDER");
    }
    if sh_flags & 0x100 > 0 {
        f.push("OS_NONCONFORMING");
    }
    if sh_flags & 0x200 > 0 {
        f.push("GROUP");
    }
    if sh_flags & 0x400 > 0 {
        f.push("TLS");
    }
    if sh_flags & 0x800 > 0 {
        f.push("COMPRESSED");
    }
    if sh_flags & 0x0ff00000 > 0 {
        f.push("MASKOS");
    }
    if sh_flags & 0xf0000000 > 0 {
        f.push("MASKPROC");
    }
    f
}

impl ELFSectionHeader {
    fn new<R: Read>(mut r: R, x64: bool, le: bool) -> Result<Self, std::io::Error> {
        let sh_type: u32;
        let sh_flags: u64;
        Ok(Self {
            sh_name: rdu32(&mut r, le)?,
            sh_namestr: String::new(),
            sh_type: {
                sh_type = rdu32(&mut r, le)?;
                sh_type
            },
            sh_typestr: section_type(sh_type),
            sh_flags: {
                sh_flags = match x64 {
                    true => rdu64(&mut r, le)?,
                    false => rdu32(&mut r, le)?.into(),
                };
                sh_flags
            },
            sh_flagsvec: section_flags(sh_flags),
            sh_addr: match x64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)?.into(),
            },
            sh_offset: match x64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)?.into(),
            },
            sh_size: match x64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)?.into(),
            },
            sh_link: rdu32(&mut r, le)?,
            sh_info: rdu32(&mut r, le)?,
            sh_addralign: match x64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)?.into(),
            },
            sh_entsize: match x64 {
                true => rdu64(&mut r, le)?,
                false => rdu32(&mut r, le)?.into(),
            },
        })
    }
}

fn get_section_names<R: Read + Seek>(
    mut r: R,
    section_headers: &Vec<ELFSectionHeader>,
    e_shstrndx: usize,
) -> Result<Vec<String>, std::io::Error> {
    let mut names: Vec<String> = vec![];
    let sn = &section_headers[e_shstrndx];

    if sn.sh_size > 2048 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid names section size",
        ));
    }

    r.seek(std::io::SeekFrom::Start(sn.sh_offset))?;
    for sh in section_headers {
        let mut bytes = Vec::new();
        let mut br = BufReader::new(&mut r);
        br.seek(std::io::SeekFrom::Start(sn.sh_offset + sh.sh_name as u64))?;
        br.read_until(b'\0', &mut bytes)?;
        if bytes.last() == Some(&0) {
            bytes.pop();
        }
        let name = match String::from_utf8(bytes) {
            Ok(n) => n,
            Err(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid section name",
                ));
            }
        };
        names.push(name);
    }

    Ok(names)
}

#[derive(Serialize)]
pub struct ELF {
    /// The ELF header
    pub elf_header: ELFHeader,

    /// The program headers
    pub program_headers: Vec<ELFProgramHeader>,

    /// The section headers
    pub section_headers: Vec<ELFSectionHeader>,

    /// Potential issues detected
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
}

impl ELF {
    /// Parses the ELF file and returns its structure or an error
    pub fn new<R: Read + Seek>(mut r: R) -> Result<Self, std::io::Error> {
        let mut issues: Vec<String> = vec![];

        let elf_header = ELFHeader::new(&mut r)?;

        if (elf_header.is_x64() && elf_header.e_phoff != 0x40)
            || (!elf_header.is_x64() && elf_header.e_phoff != 0x34)
        {
            issues.push("EH_UNUSUAL_PHOFF".to_string());
            r.seek(std::io::SeekFrom::Start(elf_header.e_phoff))?;
        }
        if elf_header.e_shentsize != 40 && elf_header.e_shentsize != 64 {
            issues.push("EH_UNUSUAL_SHENTSIZE".to_string());
        }

        let mut program_headers = Vec::new();
        for _i in 1..elf_header.e_phnum + 1 {
            let ph = ELFProgramHeader::new(&mut r, elf_header.is_x64(), elf_header.is_le())?;

            program_headers.push(ph);
        }

        r.seek(std::io::SeekFrom::Start(elf_header.e_shoff))?;
        let mut section_headers = Vec::new();
        for _ in 0..elf_header.e_shnum {
            let sh = ELFSectionHeader::new(&mut r, elf_header.is_x64(), elf_header.is_le())?;
            section_headers.push(sh);
        }

        if elf_header.e_shstrndx > elf_header.e_shnum {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid index of names section",
            ));
        }
        let section_names =
            get_section_names(&mut r, &section_headers, elf_header.e_shstrndx as usize)?;

        for i in 0..elf_header.e_shnum as usize {
            section_headers[i].sh_namestr.clone_from(&section_names[i]);
        }

        if section_headers[0].sh_type != 0 || !section_headers[0].sh_namestr.is_empty() {
            issues.push("SH_INVALID_NULL_SECTION".to_string());
        }

        Ok(Self {
            elf_header,
            program_headers,
            section_headers,
            issues,
        })
    }
}
