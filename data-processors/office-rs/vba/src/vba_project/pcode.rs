#![allow(clippy::manual_range_patterns, clippy::unwrap_or_default)]
#[cfg(test)]
mod test;
use super::*;
use std::fmt;

/// # P-code decompiler
///
/// VBA code is stored in the version dependent portion of the modules in
/// a binary format known as P-code; this interface turns it back into
/// version independent source form (i.e. text)
///
/// # Supported versions
///
/// [SysKind](crate::ProjectInfo::sys_kind) 03 [VBA versions](VbaProject::vba_version)
/// 0x97, 0xa6, 0xb2, 0xb5, 0xd9 and 0xdc were were the main target of the
/// reverse engineering work and should yield nearly perfect results - but
/// see the notes further ahead
///
/// SysKind 01 was later adapted to the existing framework but has been very
/// carefully tested; specifically VBA versions 0x6d, 0x73, 0x76, 0x79, 0x85,
/// 0x88, 0x94, 0x97, 0xa3, 0xaf and 0xb2 have proved to decode properly
///
/// SysKind 01 VBA versions 0x5x (i.e. VBA5) are extremely peculiar and were
/// literally hammered into place solely based on a diverse but relatively
/// small collection; bugs are to be expected
///
/// I couldn't find any sample of earlier VBA versions which are therefore
/// not supported
///
/// Last, a few arcane opcodes (which I found no samples of and which I could
/// not forge into a valid line without Word crashing on me) are possibly
/// missing; due to the nature of this work, a complete list of these cannot
/// be provided
///
/// ## Important note
///
/// The aim of the interface is to produce code in a form which is as close
/// as possible to what is contained in the version independent portion, or,
/// if you wish, to what the VBA editor shows
///
/// This may sound like a stupid remark, however it's entirely possible in the
/// VBA world that what you see is, to some degree, not what actually gets
/// executed
///
/// The exploration of such possibilities is too broad for this scope, therefore
/// just stick to this TL;DR: "you get the code in its **displayed** form"
///
/// ### Notable exceptions
///
/// While the general rule above stands, for consistency reasons, the following
/// items are intentionally rendered differently from the VBA editor and the
/// version independent form:
/// * Float values: instead of a dynamic and locale aware format these are
///   always rendered in scientific notation
/// * Date values: instead of a dynamic and locale aware format these are
///   always rendered in ISO 8601 format
///
/// Additionally the following unintended divergences are currently present:
/// * Module-level attributes are omitted
/// * Line continuation: VBA allows splitting long lines via the line continuation
///   character (`_`), but this decompiler will always produce uninterrupted
///   single lines
/// * Charsets: in principle the project charset is declared in the project itself
///   and should be used for all the text therein; in practice Office doesn't
///   quite care and tends to use *whatever* happens to be the local default,
///   with a strong preference for CP-1252
///   This decompiler instead always converts text into UTF-8
///
/// # Final notes
///
/// This decompiler is the result of many days of reverse engineering, long trial
/// and error sessions, countless outrage bursts and a graveyard of crashed
/// Office instances
///
/// The VBA blobs are a special kind of mess which only ranks second because the
/// interpreter itself is the worst piece of code ever written and cannot but
/// take the lead
///
/// In short, while this decompiler aims to cover all cases, it might fail on you
/// in subtle and unexpected ways; the good news is that Office will probably
/// do much worse
///
/// # Example
/// ```no_run
/// use std::io::{Read, Seek};
/// use crate::vba::*;
/// fn print_decompiled_pcode<R: Read + Seek>(vba: &Vba<R>) {
///     for module in vba.modules().unwrap() {
///         println!("Module {}", module.main_name().unwrap());
///         let decompiler = vba.get_decompiler(&module).unwrap();
///         for line in decompiler.iter() {
///             match line {
///                 Ok(s) => println!("{}", s),
///                 Err(e) => println!("<Decompiler error: {}>", e),
///             }
///         }
///     }
/// }
/// ```
pub struct ModuleDecompiler<'a> {
    project: &'a ProjectPC,
    imptbl: ImpTbl,
    type_names: TypeNames,
    functbl: FuncTbl,
    listing: Vec<CodeLine>,
    codebuf: Vec<u8>,
    nlines: u16,
    noncont: u16,
    is_5x: bool,
}

impl<'a> ModuleDecompiler<'a> {
    pub(crate) fn new<R: Read + Seek>(
        f: &mut R,
        size: u32,
        project: &'a ProjectPC,
        vba_project: &VbaProject,
    ) -> Result<Self, io::Error> {
        if vba_project.vba_version == 0xffff {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PerformanceCache is disabled on this document",
            ));
        }
        if project.sys_kind != 1 && project.sys_kind != 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("SysKind {} is not supported", project.sys_kind),
            ));
        }
        if vba_project.is_5x() {
            Self::mkdecomp_5x(f, size, project)
        } else {
            Self::mkdecomp(f, size, project)
        }
    }

    fn mkdecomp<R: Read + Seek>(
        f: &mut R,
        size: u32,
        project: &'a ProjectPC,
    ) -> Result<Self, io::Error> {
        let magic = rdu16le(f)?;
        debug_assert_eq!(magic, 0x1601);
        let sys_kind = rdu16le(f)?;
        debug_assert_eq!(u32::from(sys_kind), project.sys_kind);
        let _unk01 = rdu8(f)?; // 0, 1, 2, 3, 4, 6

        let offset_to_typetbl = rdu32le(f)?;
        let offset_to_refs = rdu32le(f)?; // points to refs
        let offset_to_me = rdu32le(f)?; // points to "ME"
        let offset_to_functbl = rdu32le(f)?; // points to the functbl
        let usually_missing = rdu32le(f)?; // Only set when objects are used
        let offset_to_cafe = rdu32le(f)?; // points to 0xCAFE
        let offset_to_last = rdu32le(f)?; // points to last 6 bytes??

        // Note: some sys_kind=03 files (typically VBA version 97, d9 and dc) have an
        // extra u32 here which appears to contain the offset to the last dword of the PC
        // This also affects the Procedure fields - see Procedure::new
        let phantom = rdu32le(f)?;
        let has_phantoms = phantom >= offset_to_last;
        let _unk02 = if has_phantoms { rdu32le(f)? } else { phantom };
        let _unk03 = rdu32le(f)?; // always 1
        let _unk04 = rdu32le(f)?; // some uniq?
        let _unk05 = rdu16le(f)?; // always 0
        let _unk06 = rdu16le(f)?; // always ffff
        let _unk07 = rdu32le(f)?; // bitmask? either 1 or some small number ending in 3
        let _unk08 = rdu32le(f)?; // bitmask? 0, 80, 82, 88
        let _unk09 = rdu16le(f)?; // always b6
        let _unk10 = rdu16le(f)?; // almost always ffff
        let _unk11 = rdu16le(f)?; // always 101
        let imptbl_size: u32 = rdu32le(f)?;
        debug!("PCODE:sys_kind(module) {:x}", sys_kind);
        debug!("PCODE:u01 {:x}", _unk01);
        debug!("PCODE:type_names {:x}", offset_to_typetbl);
        debug!("PCODE:refs {:x}", offset_to_refs);
        debug!("PCODE:me {:x}", offset_to_me);
        debug!("PCODE:functbl {:x}", offset_to_functbl);
        debug!("PCODE:missing {:x}", usually_missing);
        debug!("PCODE:cafe {:x}", offset_to_cafe);
        debug!("PCODE:last {:x}", offset_to_last);
        debug!("PCODE:phantom {} {}", phantom >= offset_to_last, phantom);
        debug!("PCODE:u02 {:x}", _unk02);
        debug!("PCODE:u03 {:x}", _unk03);
        debug!("PCODE:u04 {:x}", _unk04);
        debug!("PCODE:u05 {:x}", _unk05);
        debug!("PCODE:u06 {:x}", _unk06);
        debug!("PCODE:u07 {:x}", _unk07);
        debug!("PCODE:u08 {:x}", _unk08);
        debug!("PCODE:u09 {:x}", _unk09);
        debug!("PCODE:u10 {:x}", _unk10);
        debug!("PCODE:u11 {:x}", _unk11);
        debug!("PCODE:import table size {:x}", imptbl_size);
        let imptbl = ImpTbl::new(f, imptbl_size)?;

        debug_assert!(offset_to_typetbl < size);
        debug_assert!(offset_to_typetbl < offset_to_functbl);
        f.seek(io::SeekFrom::Start(offset_to_typetbl.into()))?;
        let type_names = TypeNames::new(project, f)?;
        debug!("PCODE:type_names {:#x?}", type_names);

        debug_assert!(offset_to_refs < size);
        f.seek(io::SeekFrom::Start(offset_to_refs.into()))?;
        let refs_hdr = rdu32le(f)?;
        debug!("PCODE:refs hdr {:x}", refs_hdr);
        let refs_xxx = rdu16le(f)?;
        debug!("PCODE:refs xxx {:x}", refs_xxx);
        let fin: u8 = loop {
            let mark = rdu8(f)?;
            if mark == 0 {
                continue;
            }
            if mark != 1 {
                break mark;
            }
            let s = ProjectPC::get_string(f)?;
            debug!("PCODE:refs {}", s);
        };
        debug_assert_eq!(fin, 0xdf);

        debug_assert!(offset_to_me < size);
        f.seek(io::SeekFrom::Start(offset_to_me.into()))?;
        debug_assert_eq!(rdu32le(f)?, 0x0000454d);
        debug_assert!(offset_to_me <= offset_to_typetbl);
        for _ in 0..((offset_to_typetbl - offset_to_me) / 6) {
            let me1 = rdu16le(f)?;
            let me2 = rdu16le(f)?;
            let me3 = rdu16le(f)?;
            debug!("PCODE:me {:04x} {:04x} {:04x}", me1, me2, me3);
            /* always
            PCODE:me ffff ffff ffff
            PCODE:me 0 0 ffff
            PCODE:me 0 0 ffff
            PCODE:me 101 0 0
             */
        }

        f.seek(io::SeekFrom::Start(offset_to_functbl.into()))?;
        let functbl = FuncTbl::new(f, sys_kind, has_phantoms)?;
        debug!("FUNCTBL: {:x?}", functbl);

        debug_assert!(offset_to_cafe < size);
        f.seek(io::SeekFrom::Start(offset_to_cafe.into()))?;
        let coffee1 = rdu16le(f)?;
        let coffee2 = rdu16le(f)?;
        debug!("PCODE:coffee {:04x} {:04x}", coffee1, coffee2);
        f.seek(io::SeekFrom::Current(0x38))?; // All blank
        let (listing, codebuf, nlines, noncont) = Self::get_code_lines(f)?;
        Ok(Self {
            project,
            imptbl,
            type_names,
            functbl,
            listing,
            codebuf,
            nlines,
            noncont,
            is_5x: false,
        })
    }

    fn mkdecomp_5x<R: Read + Seek>(
        f: &mut R,
        size: u32,
        project: &'a ProjectPC,
    ) -> Result<Self, io::Error> {
        assert!(project.sys_kind == 1);
        let magic = rdu16le(f)?;
        debug_assert_eq!(magic, 0x1601);
        let sys_kind = rdu16le(f)?;
        debug_assert_eq!(u32::from(sys_kind), project.sys_kind);
        let _unk01 = rdu8(f)?; // 0, 1, 2, 3, 4, 6
        debug!("PCODE5X: unk01: {:x}", _unk01);
        let _unk02 = rdu16le(f)?; // always b6
        debug!("PCODE5X: unk02: {:x}", _unk02);
        let _unk03 = rdu16le(f)?; // mostly ffff, rarely 0, 24, 2c, 2b8 11c
        debug!("PCODE5X: unk03: {:x}", _unk03);
        let _unk04 = rdu16le(f)?; // always 101
        debug!("PCODE5X: unk04: {:x}", _unk04);
        let imptbl_size = rdu32le(f)?;
        debug!("PCODE5X: imptbl_size: {:x}", imptbl_size);
        let imptbl = ImpTbl::new(f, imptbl_size)?;
        for i in 0..32 {
            let _unk = rdu16le(f)?;
            debug!("PCODE5X: unktbl1 {}: {:x}", i, _unk);
        }
        let some_cnt = rdu16le(f)?;
        for i in 0..some_cnt {
            let _unkt0 = rdu16le(f)?;
            let _unkt1 = rdu16le(f)?;
            let _unkt2 = rdu16le(f)?;
            let _unkt3 = rdu16le(f)?;
            let _unkt4 = rdu16le(f)?;
            let _unkt5 = rdu16le(f)?;
            let _unkt6 = rdu16le(f)?;
            let _unkt7 = rdu16le(f)?;
            debug!(
                "PCODE5X: unktbl2 {}: {:04x} {:04x} {:04x} {:04x} {:04x} {:04x} {:04x} {:04x}",
                i, _unkt0, _unkt1, _unkt2, _unkt3, _unkt4, _unkt5, _unkt6, _unkt7
            );
        }

        let _cnt_0 = rdu32le(f)?; // always 10
        let _cnt_1 = rdu32le(f)?; // always 3
        let _cnt_2 = rdu32le(f)?; // always 5
        let _cnt_3 = rdu32le(f)?; // always 7
        debug!(
            "PCODE5X: cnts: {:x} {:x} {:x} {:x}",
            _cnt_0, _cnt_1, _cnt_2, _cnt_3
        );
        let _unk05 = rdu32le(f)?; // always ffffffff
        debug!("PCODE5X: unk05: {:x}", _unk05);
        let _unk06 = rdu32le(f)?; // always ffffffff
        debug!("PCODE5X: unk06 {:x}", _unk06);
        let _unk07 = rdu16le(f)?; // always 101
        debug!("PCODE5X: unk07 {:x}", _unk07);
        let skip_sz = rdu32le(f)?; // Nothing interesting
        debug!("PCODE5X: skip_sz {:x}", skip_sz);
        f.seek(io::SeekFrom::Current(skip_sz.into()))?;
        let offset_to_typetbl = rdu32le(f)?;
        debug!("PCODE5X: offset_to_typetbl {:x}", offset_to_typetbl);
        let offset_to_me = rdu32le(f)?;
        debug!("PCODE5X: offset_to_me {:x}", offset_to_me);
        let offset_to_functbl = rdu32le(f)?; // points to the functbl
        debug!("PCODE5X: offset_to_functbl {:x}", offset_to_functbl); // +8
        let _unk10 = rdu32le(f)?; // always ffffffff
        debug!("PCODE5X: unk10 {:x}", _unk10);
        let _unk11 = rdu32le(f)?; // ???
        debug!("PCODE5X: unk11 {:x}", _unk11);
        let _unk12 = rdu32le(f)?; // always 1
        debug!("PCODE5X: unk12 {:x}", _unk12);
        let _unk13 = rdu32le(f)?; // ???
        debug!("PCODE5X: unk13 {:x}", _unk13);
        let _unk14 = rdu32le(f)?; // ???
        debug!("PCODE5X: unk14 {:x}", _unk14);
        let content_flags = rdu32le(f)?;
        debug!("PCODE5X: content_flags {:x}", content_flags);
        let _unk16 = rdu32le(f)?; // ???
        debug!("PCODE5X: unk16 {:x}", _unk16);
        let _vb_controls: Vec<String> = if content_flags & 0x40 != 0 {
            // Module-level Attribute VB_Control lines
            let nstrs = rdu16le(f)?;
            let mut strs = Vec::with_capacity(nstrs.into());
            for _ in 0..nstrs {
                let len = rdu16le(f)?;
                let mut buf = vec![0u8; len.into()];
                f.read_exact(&mut buf)?;
                let s = utf8dec_rs::decode_win_str(&buf, project.codepage);
                debug!("PCODE5X: vb_control: {}", s);
                strs.push(s);
            }
            strs
        } else {
            Vec::new()
        };
        let _unk17 = rdu8(f)?; // 8 or 2
        debug!("PCODE5X: unk17 {:x}", _unk17);
        let mut _unk18 = vec![0u8; 0x20];
        f.read_exact(&mut _unk18)?;
        debug!("PCODE5X: unk18 {:x?}", _unk18);
        let _unk19 = rdu16le(f)?; // ???
        debug!("PCODE5X: unk19 {:x}", _unk19);
        if _unk19 == 0xffff {
            // Not sure this is right, only one sample fails here
            let _unk20 = rdu16le(f)?; // ???
            debug!("PCODE5X: unk20 {:x}", _unk20);
        } else {
            let mut _unk20 = vec![0u8; 8];
            f.read_exact(&mut _unk20)?;
            debug!("PCODE5X: unk19/20 buf {:x} {:x?}", _unk19, _unk20);
        }
        let offset_to_cafe = rdu32le(f)?;
        debug!("PCODE5X: offset_to_cafe {:x}", offset_to_cafe);
        let _unk21 = rdu32le(f)?; // same as 22
        debug!("PCODE5X: unk21 {:x}", _unk21);
        let _unk22 = rdu32le(f)?; // same as 21
        debug!("PCODE5X: unk22 {:x}", _unk22);
        let offset_to_last = rdu32le(f)?;
        debug!("PCODE5X: offset_to_last {:x}", offset_to_last);

        f.seek(io::SeekFrom::Start(offset_to_typetbl.into()))?;
        let type_names = TypeNames::new(project, f)?;
        // FIXME: this contains both the types and the refs
        // if there if there's any space left, the refs are present otherwise nope
        // but what's the size?
        /*
        let refs_hdr = rdu32le(f)?;
        debug!("PCODE5X:refs hdr {:x}", refs_hdr);
        let refs_xxx = rdu16le(f)?;
        debug!("PCODE5X:refs xxx {:x}", refs_xxx);
        let fin: u8 = loop {
            let mark = rdu8(f)?;
            if mark == 0 {
                continue;
            }
            if mark != 1 {
                break mark;
            }
            let s = ProjectPC::get_string(f)?;
            debug!("PCODE5X:refs {}", s);
        };
        debug_assert_eq!(fin, 0xdf);
        */

        f.seek(io::SeekFrom::Start(offset_to_functbl.into()))?;
        let functbl = FuncTbl::new(f, sys_kind, false)?;
        debug!("FUNCTBL: {:x?}", functbl);

        f.seek(io::SeekFrom::Start(offset_to_me.into()))?;
        debug_assert_eq!(rdu32le(f)?, 0x0000454d);
        for _ in 0..((60) / 6) {
            let me1 = rdu16le(f)?;
            let me2 = rdu16le(f)?;
            let me3 = rdu16le(f)?;
            debug!("PCODE5X:me {:04x} {:04x} {:04x}", me1, me2, me3);
            /* always
            PCODE:me ffff ffff ffff
            PCODE:me 0 0 ffff
            PCODE:me 0 0 ffff
            PCODE:me 101 0 0
             */
        }

        debug_assert!(offset_to_cafe < size);
        f.seek(io::SeekFrom::Start(offset_to_cafe.into()))?;
        let coffee1 = rdu16le(f)?;
        let coffee2 = rdu16le(f)?;
        debug!("PCODE5X:coffee {:04x} {:04x}", coffee1, coffee2);
        f.seek(io::SeekFrom::Current(0x38))?; // All blank
        let (listing, codebuf, nlines, noncont) = Self::get_code_lines(f)?;
        Ok(Self {
            project,
            imptbl,
            type_names,
            functbl,
            listing,
            codebuf,
            nlines,
            noncont,
            is_5x: true,
        })
    }

    /// Returns the number of code lines in the module
    ///
    /// The returned tuple contains: the total number of lines and the number of
    /// non-contiguous lines (i.e. all those which are not allocated right after
    /// the preceding line)
    ///
    /// Line fragmentation offers a good estimate of how much manual typing occurred
    /// when the code was written: manually written code will produce a very high ratio
    /// while modules directly imported from .bas files and the likes will result in
    /// nearly zero non-contiguous lines
    pub fn num_lines(&self) -> (u16, u16) {
        (self.nlines, self.noncont)
    }

    fn get_code_lines<R: Read>(f: &mut R) -> Result<(Vec<CodeLine>, Vec<u8>, u16, u16), io::Error> {
        let magic = rdu32le(f)?;
        debug_assert_eq!(magic, 0x01cafe);
        let nlines = rdu16le(f)?;
        let mut listing: Vec<CodeLine> = Vec::with_capacity(nlines.into());
        let mut noncont = 0u16;
        let mut lastoff = 0u32;
        for _ in 0..nlines {
            let line = CodeLine::new(f)?;
            if line.offset != 0xffff && line.offset != lastoff {
                noncont += 1;
            }
            lastoff = line.next_offset().unwrap_or(lastoff);
            listing.push(line);
        }
        debug!(
            "PCODE:listing {:.2}% non contiguous",
            100f64 * f64::from(noncont) / f64::from(nlines)
        );
        let mut buf = [0u8; 10];
        f.read_exact(&mut buf)?;
        debug!("PCODE5X:listing gap {:02x?}", buf);
        let code_size = usize::try_from(
            listing
                .iter()
                .map(|l| l.next_offset().unwrap_or(0))
                .max()
                .unwrap_or(0),
        )
        .map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Cannot cast code size to usize")
        })?;
        let mut codebuf = vec![0u8; code_size];
        f.read_exact(&mut codebuf)?;
        Ok((listing, codebuf, nlines, noncont))
    }

    /// Iterates each line of code in the module
    ///
    /// Each returned item is either the decompiled line or an Error in case the decompilation failed
    pub fn iter(&self) -> impl Iterator<Item = Result<String, io::Error>> + '_ {
        self.listing.iter().map(|line| {
            if line.is_empty() {
                Ok("".to_string())
            } else {
                LineDecompiler::decode(line, self)
            }
        })
    }
}

#[derive(Debug)]
/// Line metadata (color, p-code offset, size, etc)
struct CodeLine {
    _decor: u8, // The decoration/color of the line
    _unk1: u8,  // Color?
    _unk2: u8,  // Color?
    indent: u8,
    len: u16,
    _unk3: u16,
    offset: u32,
}

impl CodeLine {
    fn new<R: Read>(f: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 12];
        f.read_exact(&mut buf)?;
        Ok(Self {
            _decor: buf[0],
            _unk1: buf[1],
            _unk2: buf[2],
            indent: buf[3],
            len: u16::from_le_bytes(buf[4..6].try_into().unwrap()),
            _unk3: u16::from_le_bytes(buf[6..8].try_into().unwrap()),
            offset: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
        })
    }

    fn is_empty(&self) -> bool {
        self.offset == 0xffffffff || self.len == 0
    }

    fn next_offset(&self) -> Option<u32> {
        if self.is_empty() {
            None
        } else {
            let next = self.offset.checked_add(u32::from(self.len))?;
            next.checked_add((8 - (next & 7)) & 7)
        }
    }

    fn range(&self) -> Result<std::ops::Range<usize>, io::Error> {
        let start = usize::try_from(self.offset).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Cannot map line start offset to usize",
            )
        })?;
        let end = self.offset.checked_add(self.len.into()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Cannot map line range wrap")
        })?;
        let end = usize::try_from(end).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Cannot map line end offset to usize",
            )
        })?;
        Ok(start..end)
    }
}

/// The P-code buffer for a CodeLine
struct PCode<'a> {
    codebuf: &'a [u8],
    cp: u16,
}

impl<'a> PCode<'a> {
    fn new(code: &'a [u8], cp: u16) -> Self {
        Self { codebuf: code, cp }
    }

    fn is_empty(&self) -> bool {
        self.codebuf.is_empty()
    }

    fn get_u64(&mut self) -> Result<u64, io::Error> {
        rdu64le(&mut self.codebuf)
    }

    fn get_u32(&mut self) -> Result<u32, io::Error> {
        rdu32le(&mut self.codebuf)
    }

    fn get_u16(&mut self) -> Result<u16, io::Error> {
        rdu16le(&mut self.codebuf)
    }

    fn get_f64(&mut self) -> Result<f64, io::Error> {
        rdf64le(&mut self.codebuf)
    }

    fn get_f32(&mut self) -> Result<f32, io::Error> {
        rdf32le(&mut self.codebuf)
    }

    fn get_string(&mut self) -> Result<String, io::Error> {
        let len: usize = rdu16le(&mut self.codebuf)?.into();
        if len > self.codebuf.len() {
            // for lack of split_off, split_at panics
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Inline string overflow",
            ));
        }
        let s = utf8dec_rs::decode_win_str(&self.codebuf[0..len], self.cp);
        self.codebuf = &self.codebuf[(len + (len & 1))..];
        Ok(s)
    }

    fn get_op(&mut self, project: &ProjectPC, is_5x: bool) -> Result<(u16, u16), io::Error> {
        let op = self.get_u16()?;
        let up = op >> 10;
        let op = op & 0x3ff;
        let op = if is_5x {
            *OP_MAP_V5X.get(usize::from(op)).unwrap()
        } else if project.sys_kind == 1 {
            *OP_MAP_32.get(usize::from(op)).unwrap()
        } else {
            op
        };
        Ok((op, up))
    }
}

#[derive(Debug)]
enum StackItem {
    Plain(String),
    Case(String),
    ArrNil,
}

impl StackItem {
    fn as_str(&self) -> &str {
        match self {
            StackItem::Plain(s) | StackItem::Case(s) => s.as_str(),
            StackItem::ArrNil => "ArrNil",
        }
    }

    fn is_case(&self) -> bool {
        matches!(self, StackItem::Case(_))
    }
}

impl From<StackItem> for String {
    fn from(si: StackItem) -> String {
        match si {
            StackItem::Plain(s) | StackItem::Case(s) => s,
            StackItem::ArrNil => "".to_string(),
        }
    }
}

impl fmt::Display for StackItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug)]
struct Stack {
    stack: Vec<StackItem>,
    labels: Vec<String>,
}

impl Stack {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            labels: Vec::new(),
        }
    }

    fn append<S: Into<String>>(&mut self, s: S) {
        self.stack.push(StackItem::Plain(s.into()));
    }

    fn append_case<S: Into<String>>(&mut self, s: S) {
        self.stack.push(StackItem::Case(s.into()));
    }

    fn append_sep(&mut self) {
        self.stack.push(StackItem::ArrNil);
    }

    fn pop(&mut self) -> Option<String> {
        self.stack.pop().map(|m| m.into())
    }

    fn pop_cases(&mut self) -> Vec<String> {
        let mut i = self.stack.len();
        while i > 0 {
            i -= 1;
            let item = &self.stack[i];
            if !item.is_case() {
                i += 1;
                break;
            }
        }
        self.split_tail(self.stack.len() - i)
    }

    fn map_last(&mut self, mapfn: impl Fn(&mut String)) {
        if self.stack.is_empty() {
            self.append("UnkStkItm");
        }
        if let StackItem::Plain(s) = self.stack.last_mut().unwrap() {
            mapfn(s);
        }
    }

    fn split_tail(&mut self, nitems: usize) -> Vec<String> {
        while self.stack.len() < nitems {
            self.append("UnkStkItm");
        }
        self.stack
            .split_off(self.stack.len() - nitems)
            .into_iter()
            .map(|m| m.into())
            .collect()
    }

    fn array_args_cnt(&mut self, cnt: u16) -> String {
        let nitems = usize::from(cnt) * 2;
        while self.stack.len() < nitems {
            self.append("UnkStkItm");
        }
        self.stack
            .split_off(self.stack.len() - nitems)
            .chunks(2)
            .map(|v| match v[0] {
                StackItem::ArrNil => v[1].to_string(),
                _ => format!("{} To {}", v[0], v[1]),
            })
            .collect::<Vec<String>>()
            .join(", ")
    }

    fn array_args(&mut self, functbl: &FuncTbl, cntoff: u32) -> Option<String> {
        let cnt = functbl.get_u16(cntoff)?;
        Some(self.array_args_cnt(cnt))
    }

    fn push_label<S: Into<String>>(&mut self, label: S) {
        self.labels.push(label.into())
    }
}

impl fmt::Display for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let line = [
            self.labels
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<&str>>(),
            self.stack.iter().map(|s| s.as_str()).collect::<Vec<&str>>(),
        ]
        .concat()
        .join(" ");
        write!(f, "{}", line)
    }
}

struct ImpTbl {
    buf: Vec<u8>,
}

impl ImpTbl {
    fn new<R: Read>(f: &mut R, len: u32) -> Result<Self, io::Error> {
        let len: usize = len.try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "ImpTbl size conversion failed")
        })?;
        let mut ret = Self { buf: vec![0; len] };
        f.read_exact(&mut ret.buf)?;
        Ok(ret)
    }

    fn get_import(&self, offset: u32) -> Option<(u16, String)> {
        let offset = usize::try_from(offset).ok()?;
        let mut f = self.buf.get(offset..)?;
        let by_ord = rdu16le(&mut f).ok()? != 0;
        let libid = rdu16le(&mut f).ok()?;
        if by_ord {
            let ord_id = rdu16le(&mut f).ok()?;
            return Some((libid, format!("#{}", ord_id)));
        }
        if let Ok(fun_offset) = rdu16le(&mut f) {
            // For once the string is null terminated, I presume it's strictly ascii
            let offset: usize = fun_offset.into();
            if let Some(buf) = self.buf.get(offset..) {
                let mut found = false;
                let func_name: String = buf
                    .iter()
                    .map_while(|v| {
                        if *v != 0 {
                            Some(char::from(*v))
                        } else {
                            found = true;
                            None
                        }
                    })
                    .collect();
                if found {
                    return Some((libid, func_name));
                }
            }
        }
        Some((libid, "UnkFunc".to_string()))
    }
}

struct LineDecompiler<'a> {
    line: &'a CodeLine,
    project: &'a ProjectPC,
    code: PCode<'a>,
    stack: Stack,
    functbl: &'a FuncTbl,
    type_names: &'a TypeNames,
    imptbl: &'a ImpTbl,
    is_5x: bool,
}

impl<'a> LineDecompiler<'a> {
    fn decode(line: &'a CodeLine, decompiler: &'a ModuleDecompiler) -> Result<String, io::Error> {
        let code = decompiler
            .codebuf
            .get(line.range()?)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Line overflow"))?;
        Self {
            line,
            project: decompiler.project,
            code: PCode::new(code, decompiler.project.codepage),
            stack: Stack::new(),
            functbl: &decompiler.functbl,
            type_names: &decompiler.type_names,
            imptbl: &decompiler.imptbl,
            is_5x: decompiler.is_5x,
        }
        .do_decode()
    }

    fn get_suffix(up: u16) -> &'static str {
        // Note: "up" suffixes are mapped differently from var suffixes
        match up & 0xf {
            1 => "^",
            2 => "%",
            3 => "&",
            4 => "!",
            5 => "#",
            6 => "@",
            8 => "$",
            13 => "^",
            _ => "",
        }
    }

    fn do_decode(&mut self) -> Result<String, io::Error> {
        // Helpers that don't borrow self but only self.xxx
        macro_rules! id2str {
            ($id:expr) => {{
                let id = $id;
                if id == 0xffff {
                    Some("Me")
                } else {
                    self.project.get_string_from_table_or_builtin(id)
                }
            }};
        }
        macro_rules! get_id_str {
            ($default:expr) => {
                id2str!(self.code.get_u16()?).unwrap_or($default)
            };
        }
        macro_rules! get_args {
            () => {
                self.stack
                    .split_tail(self.code.get_u16()?.into())
                    .join(", ")
            };
        }

        let mut attributes: Vec<String> = Vec::new();
        let mut dim_items = 0u32;
        let mut redim_as_new = false;
        let mut last_is_spc_or_tab = false;
        while !self.code.is_empty() {
            let (op, up) = self.code.get_op(self.project, self.is_5x)?;
            macro_rules! get_mangled_name {
                ($default:expr) => {{
                    let name = get_id_str!($default);
                    let sym = Self::get_suffix(up);
                    if up & 0b100000 == 0 {
                        format!("{}{}", name, sym)
                    } else {
                        format!("[{}]{}", name, sym)
                    }
                }};
            }
            debug!(
                "linebuf: op:{:02x} up:{:02x} {:02x?}",
                op, up, &self.code.codebuf
            );
            debug!("stack: {:?}", self.stack);
            match op {
                0x5d => {
                    // Dim start
                    let mut dimo: Vec<&str> = Vec::with_capacity(2);
                    // This follows the precedence used by office in case of conflicting flags
                    if up & 0b1000 != 0 {
                        dimo.push("Public");
                    } else if up & 0b10000 != 0 {
                        dimo.push("Private");
                    } else if up & 0b100 != 0 {
                        dimo.push("Global");
                    } else if up & 0b100000 != 0 {
                        dimo.push("Static");
                    } else if up & 1 == 0 {
                        dimo.push("Dim");
                    }
                    if up & 1 != 0 {
                        dimo.push("Const");
                    }
                    self.stack.append(dimo.join(" "));
                    dim_items = 0;
                }
                0x5e => {
                    // Type start
                    dim_items = 0;
                }
                0xf5 => {
                    // Dim item
                    let varoff = self.code.get_u32()?;
                    if up & 0b1_0000 != 0 {
                        // FIXME: what's this?
                        self.code.get_u16()?;
                    }
                    let var = Var::from_dim_data(self.functbl, varoff)
                        .and_then(|v| {
                            let ret = v.as_dim(
                                self.project,
                                &mut self.stack,
                                self.functbl,
                                self.type_names,
                            );
                            if ret.is_some() && v.has_module_scope() && v.arg_flags != 0 {
                                attributes.push(format!(
                                    "Attribute {}.VB_VarHelpID = {}",
                                    v.get_name(self.project),
                                    v.arg_flags as i32
                                ));
                            }
                            ret
                        })
                        .unwrap_or_else(|| "UnkVar".to_owned());
                    if dim_items > 0 {
                        self.stack.map_last(|l| *l += ",");
                    }
                    dim_items += 1;
                    if up & 0b10_0000 != 0 {
                        self.stack.append("WithEvents");
                    }
                    self.stack.append(var);
                }
                0x103 => {
                    redim_as_new = true;
                }
                0xe4 | 0xe5 | 0xc2 | 0xc4 | 0xc3 | 0xc5 => {
                    // ReDim
                    let varid = self.code.get_u16()?;
                    let cnt = self.code.get_u16()?;
                    let varoff = self.code.get_u32()?;
                    let obj = match op {
                        0xe4 | 0xe5 => "".to_string(),
                        0xc2 | 0xc4 => {
                            self.stack.pop().unwrap_or_else(|| "UnkObj".to_string()) + "."
                        }
                        0xc3 | 0xc5 => ".".to_string(),
                        _ => unreachable!(),
                    };
                    let redim_as = match op {
                        0xe4 | 0xc2 | 0xc3 => false,
                        0xe5 | 0xc4 | 0xc5 => true,
                        _ => unreachable!(),
                    };
                    let var = Var::from_redim_data(varid, varoff, redim_as, redim_as_new).as_redim(
                        cnt,
                        up,
                        self.project,
                        &mut self.stack,
                        self.functbl,
                        self.type_names,
                    );
                    redim_as_new = false;
                    if dim_items == 0 {
                        // Note: the "Preserve" flag is internally kept per var, however the syntax
                        // allows only a single "Preserve"
                        // Office sets it based on the first redimmed variable
                        let opstr = if up & 0x10 != 0 {
                            "ReDim Preserve"
                        } else {
                            "ReDim"
                        };
                        self.stack.append(format!("{} {}{}", opstr, obj, var));
                    } else {
                        self.stack
                            .map_last(|l| *l = format!("{}, {}{}", l, obj, var));
                    }
                    dim_items += 1;
                }

                0xb9 => {
                    // string literal
                    let s = self.code.get_string()?.replace('"', "\"\"");
                    self.stack.append(format!("\"{}\"", s));
                }

                0xac => {
                    // literal u16 decimal value
                    let value = self.code.get_u16()?;
                    self.stack.append(format!("{}", value));
                }
                0xb3 => {
                    // literal u16 octal value
                    let value = self.code.get_u16()?;
                    self.stack.append(format!("&O{:o}", value));
                }
                0xaf => {
                    // literal u16 hexadecimal value
                    let value = self.code.get_u16()?;
                    self.stack.append(format!("&H{:X}", value));
                }

                0xad => {
                    // literal u32 decimal value
                    let value = self.code.get_u32()?;
                    let suffix = if value < 0x8000 { "&" } else { "" };
                    self.stack.append(format!("{}{}", value, suffix));
                }
                0xb4 => {
                    // literal u32 octal value
                    let value = self.code.get_u32()?;
                    let suffix = if value < 0x8000 { "&" } else { "" };
                    self.stack.append(format!("&O{:o}{}", value, suffix));
                }
                0xb0 => {
                    // literal u32 hexadecimal value
                    let value = self.code.get_u32()?;
                    let suffix = if value < 0x10000 { "&" } else { "" };
                    self.stack.append(format!("&H{:X}{}", value, suffix));
                }

                0xae => {
                    // literal u64 decimal value
                    let value = self.code.get_u64()?;
                    self.stack.append(format!("{}^", value));
                }
                0xb5 => {
                    // literal u64 octal value
                    let value = self.code.get_u64()?;
                    self.stack.append(format!("&O{:o}^", value));
                }
                0xb1 => {
                    // literal u64 hexadecimal value
                    let value = self.code.get_u64()?;
                    self.stack.append(format!("&H{:X}^", value));
                }

                0xba => {
                    // literal special value
                    let bltin = match up {
                        0 => "False",
                        1 => "True",
                        2 => "Null",
                        3 => "Empty",
                        _ => "",
                    };
                    self.stack.append(bltin);
                }

                0xb6 => {
                    // literal single precision floating point value
                    let value = self.code.get_f32()?;
                    // Note: the actual format switches to something close to UpperExp
                    // based on the number of decimal digits and exponent
                    self.stack.append(format!("{:.7E}!", value));
                }
                0xb7 => {
                    // literal double precision floating point value
                    let value = self.code.get_f64()?;
                    // Note: the actual format switches to something close to UpperExp
                    // based on the number of decimal digits and exponent
                    // Additionally VBA renders integer floats with the # suffix but only
                    // when the exponential notation is not in use
                    self.stack.append(format!("{:.15E}", value));
                }

                0xa9 => {
                    // literal currency value (64-bit integer scaled by 10k)
                    let value = self.code.get_u64()?;
                    let mut intpt = format!("{:05}", value);
                    let decpt = intpt.split_off(intpt.len() - 4);
                    let decpt = decpt.trim_end_matches('0');
                    if decpt.is_empty() {
                        self.stack.append(format!("{}@", intpt));
                    } else {
                        self.stack.append(format!("{}.{}@", intpt, decpt));
                    }
                }

                0xaa => {
                    // literal date value
                    let t = self.code.get_f64()?;
                    let dt = t.trunc() as i64;
                    let tm = (t.fract() * 24f64 * 60f64 * 60f64).round() as u64;
                    let ss = tm % 60;
                    let mm = tm / 60 % 60;
                    let mut hh = (tm / 60 / 60) % 24;
                    let ampm = if hh > 11 { "PM" } else { "AM" };
                    hh %= 12;
                    if hh == 0 {
                        hh = 12;
                    }
                    if dt == 0 {
                        self.stack
                            .append(format!("#{}:{:02}:{:02} {}#", hh, mm, ss, ampm));
                    } else {
                        // Note: the date portion is represented here in ISO 8601 format for consistency
                        // VBA however, not only shows it localized, but also stores it localized
                        // in the source code portion of the module
                        let vba_epoch: time::Date =
                            time::Date::from_calendar_date(1899, time::Month::December, 30)
                                .unwrap();
                        let dt = vba_epoch
                            .checked_add(time::Duration::days(dt))
                            .and_then(|date| {
                                date.format(time::macros::format_description!(
                                    "[year]-[month]-[day]"
                                ))
                                .ok()
                            })
                            .unwrap_or_else(|| "UnkDate".to_string());
                        if tm == 0 {
                            self.stack.append(format!("#{}#", dt));
                        } else {
                            self.stack
                                .append(format!("#{} {}:{:02}:{:02} {}#", dt, hh, mm, ss, ampm));
                        }
                    }
                }

                0x100 => {} // # marker
                0xfb => {
                    // #Const
                    let label = get_id_str!("UnkConst");
                    let val = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack.append(format!("#Const {} = {}", label, val));
                }
                0xfc => {
                    // #If ... Then
                    self.stack.map_last(|l| *l = format!("#If {} Then", l));
                }
                0xfd => {
                    // #Else
                    self.stack.append("#Else");
                }
                0xfe => {
                    // #ElseIf ... Then
                    self.stack.map_last(|l| *l = format!("#ElseIf {} Then", l));
                }
                0xff => {
                    // #End If
                    self.stack.append("#End If");
                }

                // FIXME: op 0x97
                0x96 | 0x74 => {
                    // procedure and similar definitions
                    // Note: procs and events have distinct opcodes (96 and 74) however office
                    // appears to handle them in the same way and to rely on up and flags to
                    // tell them apart
                    let tbloff = self.code.get_u32()?;
                    let mut procdef = String::new();
                    if let Some(proc) = Procedure::new(self.functbl, tbloff, up, self.is_5x) {
                        let name = id2str!(proc.nameid).unwrap_or("UnkProc");
                        debug!("PCODE: {}: {:x?}", name, proc);

                        // Public, Private, etc
                        procdef += proc.visibility_str();

                        // Declare
                        procdef += proc.declare_str();

                        // Static
                        procdef += proc.static_str();

                        // Sub, Function, Event, etc.
                        procdef += proc.type_str();

                        // Decorated procedure name
                        let (ret_type, decor) = Var::from_proc(&proc)
                            .as_return_type(self.functbl, self.type_names)
                            .unwrap_or(("UnkRet".to_string(), ""));
                        procdef += name;
                        procdef += decor;

                        // Lib (and Alias) if declare
                        if proc.is_declare() {
                            // Note: weird things happen when the table is overflown
                            let (lib, alias) = if let Some((a, b)) =
                                self.imptbl.get_import(proc.import_offset.into())
                            {
                                let lib = id2str!(a).unwrap_or("UnkLib");
                                let alias = if b == name {
                                    "".to_string()
                                } else {
                                    format!("Alias \"{}\" ", b)
                                };
                                (lib, alias)
                            } else {
                                ("", "".to_string())
                            };
                            procdef += &format!(" Lib \"{}\" {}", lib, alias);
                        }

                        // Args and return type (if any)
                        let mut args = Vec::<String>::with_capacity(proc.get_argcnt().into());
                        let mut varoff = proc.first_var_offset;
                        for _ in 0..proc.get_argcnt() {
                            if let Some(var) = Var::from_dim_data(self.functbl, varoff) {
                                let arg = var
                                    .as_arg(
                                        self.project,
                                        &mut self.stack,
                                        self.functbl,
                                        self.type_names,
                                    )
                                    .unwrap_or_else(|| "UnkVar".to_string());
                                args.push(arg);
                                varoff = var.next_offset;
                            }
                        }
                        if proc.has_return() {
                            args.pop();
                        }
                        if proc.is_vararg() {
                            if let Some(l) = args.last_mut() {
                                *l = format!("ParamArray {}", l);
                            }
                        }
                        procdef += &format!("({}){}", args.join(", "), ret_type);
                        attributes = proc.attributes(name, self.functbl, self.project.codepage);
                    } else {
                        procdef = "UnkProc()".to_string();
                    }
                    self.stack.append(procdef);
                }
                0xfa => {
                    // Indicates literals on the stack belong to the proc def
                    // E.g. Sub(var() As String * 20)
                }
                // proc closers and exits
                0x67 => self.stack.append("End".to_string()),
                0x69 => self.stack.append("End Function".to_string()),
                0x6d => self.stack.append("End Property".to_string()),
                0x6f => self.stack.append("End Sub".to_string()),
                0x7a => self.stack.append("Exit Function".to_string()),
                0x7b => self.stack.append("Exit Property".to_string()),
                0x7c => self.stack.append("Exit Sub".to_string()),
                0x75 => {
                    // RaiseEvent
                    let name = get_id_str!("UnkEvt");
                    let args = get_args!();
                    if args.is_empty() {
                        self.stack.append(format!("RaiseEvent {}", name));
                    } else {
                        self.stack.append(format!("RaiseEvent {}({})", name, args));
                    }
                }
                0x76 => {
                    // RaiseEvent (obj)
                    let name = get_mangled_name!("UnkEvt");
                    let obj = self.stack.pop().unwrap_or_else(|| "UnkObj".to_string());
                    let event = format!("{}.{}", obj, name);
                    let args = get_args!();
                    if args.is_empty() {
                        self.stack.append(format!("RaiseEvent {}", event));
                    } else {
                        self.stack.append(format!("RaiseEvent {}({})", event, args));
                    }
                }
                0x77 => {
                    // RaiseEvent (with)
                    let name = get_mangled_name!("UnkEvt");
                    let event = format!(".{}", name);
                    let args = get_args!();
                    if args.is_empty() {
                        self.stack.append(format!("RaiseEvent {}", event));
                    } else {
                        self.stack.append(format!("RaiseEvent {}({})", event, args));
                    }
                }

                0x41 | 0x42 | 0x43 => {
                    // call sub | call method | call With method
                    let mut name = get_mangled_name!("UnkProc");
                    if op == 0x42 {
                        let obj = self.stack.pop().unwrap_or_else(|| "UnkObj".to_string());
                        name = format!("{}.{}", obj, name);
                    } else if op == 0x43 {
                        name = format!(".{}", name);
                    }
                    let args = get_args!();
                    self.stack.append(if up & 0b10000 == 0 {
                        if !args.is_empty() {
                            format!("Call {}({})", name, args)
                        } else {
                            format!("Call {}", name)
                        }
                    } else if !args.is_empty() {
                        format!("{} {}", name, args)
                    } else {
                        name
                    });
                }
                0x44 => {
                    // array args ()
                    let func = get_mangled_name!("UnkArr");
                    let args = get_args!();
                    self.stack.append(format!("{}({})", func, args));
                }
                0x23 => {
                    // array index
                    let ar = self.stack.pop().unwrap_or_else(|| "UnkArr".to_string());
                    let args = get_args!();
                    self.stack.append(format!("{}({})", ar, args));
                }
                0x2a | 0x31 => {
                    // array index assign
                    let set = if op == 0x2a { "" } else { "Set " };
                    let ar = self.stack.pop().unwrap_or_else(|| "UnkArr".to_string());
                    let args = get_args!();
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack
                        .append(format!("{}{}({}) = {}", set, ar, args, src));
                }

                0xa4 => {
                    self.stack.append("Let".to_string());
                }
                0x1f => {
                    // Note: appears to act as 0x20 except excel crashes when executed
                    let id = get_mangled_name!("UnkId");
                    self.stack.append(id);
                }
                0x20 => {
                    // literal identifier
                    let id = get_mangled_name!("UnkId");
                    self.stack.append(id);
                }
                0x21 | 0x35 => {
                    // literal property | With property
                    let name = get_mangled_name!("UnkProp");
                    if op == 0x21 {
                        self.stack.map_last(|l| *l = format!("{}.{}", l, name));
                    } else {
                        self.stack.append(format!(".{}", name));
                    }
                }
                0x22 | 0x36 => {
                    // bang property | With bang property
                    let name = get_mangled_name!("UnkProp");
                    if op == 0x22 {
                        self.stack.map_last(|l| *l = format!("{}!{}", l, name));
                    } else {
                        self.stack.append(format!("!{}", name));
                    }
                }
                0x24 => {
                    // call func
                    let name = get_mangled_name!("UnkFunc");
                    let args = get_args!();
                    self.stack.append(format!("{}({})", name, args));
                }
                0x25 | 0x37 => {
                    // call method func | With method func
                    let name = get_mangled_name!("UnkProc");
                    let obj = if op == 0x25 {
                        self.stack.pop().unwrap_or_else(|| "UnkObj".to_string())
                    } else {
                        "".to_string()
                    };
                    let args = get_args!();
                    self.stack.append(format!("{}.{}({})", obj, name, args));
                }
                0x26 | 0x38 => {
                    // bang method func | With bang method func
                    let name = get_mangled_name!("UnkProc");
                    let obj = if op == 0x26 {
                        self.stack.pop().unwrap_or_else(|| "UnkObj".to_string())
                    } else {
                        "".to_string()
                    };
                    let args = get_args!();
                    self.stack.append(format!("{}!{}({})", obj, name, args));
                }
                0xc9 => {
                    // New obj
                    let id = self.code.get_u16()?;
                    self.stack
                        .append(format!("New {}", self.type_names.get_name(id)));
                }

                // Assign
                0x27 | 0x2e => {
                    // assign to var
                    let set = if op == 0x27 { "" } else { "Set " };
                    let dst = get_mangled_name!("UnkVar");
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack.append(format!("{}{} = {}", set, dst, src));
                }
                0x28 | 0x2f => {
                    // assign to prop
                    let set = if op == 0x28 { "" } else { "Set " };
                    let dst = get_mangled_name!("UnkProp");
                    let obj = self.stack.pop().unwrap_or_else(|| "UnkObj".to_string());
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack
                        .append(format!("{}{}.{} = {}", set, obj, dst, src));
                }
                0x29 | 0x30 => {
                    // assign to bang
                    let set = if op == 0x29 { "" } else { "Set " };
                    let dst = get_mangled_name!("UnkProp");
                    let obj = self.stack.pop().unwrap_or_else(|| "UnkObj".to_string());
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack
                        .append(format!("{}{}!{} = {}", set, obj, dst, src));
                }
                0x2c | 0x33 => {
                    // assign to args method
                    let set = if op == 0x2c { "" } else { "Set " };
                    let obj = self.stack.pop().unwrap_or_else(|| "UnkObj".to_string());
                    let prop = get_mangled_name!("UnkProp");
                    let args = get_args!();
                    let value = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack
                        .append(format!("{}{}.{}({}) = {}", set, obj, prop, args, value));
                }
                0x2d | 0x34 => {
                    // assign to bang method
                    let set = if op == 0x2d { "" } else { "Set " };
                    let obj = self.stack.pop().unwrap_or_else(|| "UnkObj".to_string());
                    let prop = get_mangled_name!("UnkProp");
                    let args = get_args!();
                    let value = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack
                        .append(format!("{}{}!{}({}) = {}", set, obj, prop, args, value));
                }
                0x39 | 0x3d => {
                    // assign to With prop
                    let set = if op == 0x39 { "" } else { "Set " };
                    let dst = get_mangled_name!("UnkProp");
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack.append(format!("{}.{} = {}", set, dst, src));
                }
                0x3a | 0x3e => {
                    // assign to With bang
                    let set = if op == 0x3a { "" } else { "Set " };
                    let dst = get_mangled_name!("UnkProp");
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack.append(format!("{}!{} = {}", set, dst, src));
                }
                0x3b | 0x3f => {
                    // assign to With array
                    let set = if op == 0x3b { "" } else { "Set " };
                    let dst = get_mangled_name!("UnkProp");
                    let args = get_args!();
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack
                        .append(format!("{}.{}({}) = {}", set, dst, args, src));
                }
                0x3c | 0x40 => {
                    // assign to With bang array
                    let set = if op == 0x3c { "" } else { "Set " };
                    let dst = get_mangled_name!("UnkProp");
                    let args = get_args!();
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack
                        .append(format!("{}!{}({}) = {}", set, dst, args, src));
                }
                0x2b | 0x32 => {
                    // assign to array
                    let set = if op == 0x2b { "" } else { "Set " };
                    let dst = get_mangled_name!("UnkVar");
                    let args = get_args!();
                    let src = self.stack.pop().unwrap_or_else(|| "UnkVal".to_string());
                    self.stack
                        .append(format!("{}{}({}) = {}", set, dst, args, src));
                }
                0xd1 => {
                    // Array separator
                    self.stack.append_sep();
                }

                0xf0 => {
                    // Set marker
                }

                0xab => {
                    // default value
                    // Note: this is a bitch
                    //   up&1 matters in whether an empty param is shown or not, however
                    //   the display of the empty param is inconsistent (e.g. Open vs Mid)
                    self.stack.append("".to_string());
                }
                0xd2 => {
                    // ByVal param
                    self.stack.map_last(|l| *l = format!("ByVal {}", l));
                }
                0xd3 => {
                    // omitted param
                    self.stack.append("".to_string());
                }
                0xd4 => {
                    // named param
                    let id = get_id_str!("UnkId");
                    self.stack.map_last(|l| *l = format!("{}:={}", id, l));
                }
                0xb2 => {
                    self.stack.append("Nothing".to_string());
                }

                // Do
                0x5f => {
                    self.stack.append("Do".to_string());
                }
                0x61 => {
                    self.stack.map_last(|l| *l = format!("Do Until {}", l));
                }
                0x62 => {
                    self.stack.map_last(|l| *l = format!("Do While {}", l));
                }
                0xbc => {
                    self.stack.append("Loop".to_string());
                }
                0xbd => {
                    self.stack.map_last(|l| *l = format!("Loop Until {}", l));
                }
                0xbe => {
                    self.stack.map_last(|l| *l = format!("Loop While {}", l));
                }
                0x78 => {
                    self.stack.append("Exit Do".to_string());
                }

                // For
                0x102 => {} // Begin for variable
                0x101 => {} // End for variable
                0x92 => {
                    let to = self.stack.pop().unwrap_or_else(|| "UnkTo".to_string());
                    let from = self.stack.pop().unwrap_or_else(|| "UnkFrom".to_string());
                    let var = self.stack.pop().unwrap_or_else(|| "UnkVar".to_string());
                    self.stack
                        .append(format!("For {} = {} To {}", var, from, to));
                }
                0x93 => {
                    // FIXME op 94 is similar but cannot forge it properly
                    let ar = self.stack.pop().unwrap_or_else(|| "UnkAry".to_string());
                    let var = self.stack.pop().unwrap_or_else(|| "UnkVar".to_string());
                    self.stack.append(format!("For Each {} In {}", var, ar));
                }
                0x95 => {
                    let step = self.stack.pop().unwrap_or_else(|| "UnkStep".to_string());
                    let to = self.stack.pop().unwrap_or_else(|| "UnkTo".to_string());
                    let from = self.stack.pop().unwrap_or_else(|| "UnkFrom".to_string());
                    let var = self.stack.pop().unwrap_or_else(|| "UnkVar".to_string());
                    self.stack
                        .append(format!("For {} = {} To {} Step {}", var, from, to, step));
                }
                0xca => {
                    self.stack.append("Next".to_string());
                }
                0xcb => {
                    self.stack.map_last(|l| *l = format!("Next {}", l));
                }
                0x79 => {
                    self.stack.append("Exit For".to_string());
                }

                // While
                0xf7 => {
                    self.stack.map_last(|l| *l = format!("While {}", l));
                }
                0xf6 => {
                    self.stack.append("Wend".to_string());
                }

                // If
                0x9b | 0x9c => {
                    // single | multi line
                    self.stack.map_last(|l| *l = format!("If {} Then", l));
                }
                0x9e => {
                    // If TypeOf (legacy)
                    let id = self.code.get_u16()?;
                    self.stack.map_last(|l| {
                        *l = format!("If TypeOf {} Is {} Then", l, self.type_names.get_name(id))
                    });
                }
                0x65 => {
                    self.stack.map_last(|l| *l = format!("ElseIf {} Then", l));
                }
                0x66 => {
                    // ElseIf TypeOf (legacy)
                    let id = self.code.get_u16()?;
                    self.stack.map_last(|l| {
                        *l = format!(
                            "ElseIf TypeOf {} Is {} Then",
                            l,
                            self.type_names.get_name(id)
                        )
                    });
                }
                0x63 | 0x64 => {
                    // single | multi line
                    self.stack.append("Else".to_string());
                }
                0x47 => {
                    // inline Then (marks the block)
                }
                0x6a => {
                    // inline End If (terminates the statement)
                }
                0x6b => {
                    self.stack.append("End If".to_string());
                }
                0x9d => {
                    // TypeOf
                    let id = self.code.get_u16()?;
                    self.stack.map_last(|l| {
                        *l = format!("TypeOf {} Is {}", l, self.type_names.get_name(id))
                    });
                }

                // Select + Case
                0xed => {
                    self.stack.map_last(|l| *l = format!("Select Case {}", l));
                }
                0x4b => {
                    // Case item expression
                    let arg = self.stack.pop().unwrap_or_else(|| "UnkCase".to_string());
                    self.stack.append_case(arg);
                }
                0x4c => {
                    // Case item expressionToexpression
                    let args = self.stack.split_tail(2);
                    self.stack
                        .append_case(format!("{} To {}", args[0], args[1]));
                }
                0x4d | 0x4e | 0x4f | 0x50 | 0x51 | 0x52 => {
                    // Case item Iscomparisonoperator
                    const ISOP: [&str; 6] = [">", "<", ">=", "<=", "<>", "="];
                    let next = self.stack.pop().unwrap_or_else(|| "???".to_string());
                    self.stack
                        .append_case(format!("Is {} {}", ISOP[usize::from(op - 0x4d)], next));
                }
                0x53 => {
                    self.stack.append("Case Else".to_string());
                }
                0x54 => {
                    // Case expressionlist
                    let args = self.stack.pop_cases().join(", ");
                    self.stack.append(format!("Case {}", args));
                }
                0x6e => {
                    self.stack.append("End Select".to_string());
                }
                /* Note:
                 * the following ops can be forged, but tend to lead to crashes
                 * - 0xef yields "Select TypeOf xxx"
                 * - 0xee yields "Case Is TYPE"
                 */
                // With (see above for 0x43, 0x35, 0x36, 0x37, 0x38, 0x39)
                0x104 => {} // Start of With
                0xf8 => {
                    self.stack.map_last(|l| *l = format!("With {}", l));
                }
                0x71 => {
                    self.stack.append("End With".to_string());
                }

                // Type & Enum
                0xf3 => {
                    let typ = if up & 2 != 0 { "Enum" } else { "Type" };
                    let off = self.code.get_u32()?;
                    if let Some(var) = EnumType::new(self.functbl, off, up) {
                        let name = id2str!(var.nameid).unwrap_or("UnkTy");
                        self.stack
                            .append(format!("{}{} {}", var.vis_name(), typ, name));
                    } else {
                        self.stack.append(format!("{} UnkTy", typ));
                    }
                }
                0x70 => self.stack.append("End Type".to_string()),
                0x106 => self.stack.append("End Enum".to_string()),

                0x58 => {
                    // type conversion funcs
                    let ctype = match up {
                        0x0b => "CBool",
                        0x11 => "CByte",
                        0x06 => "CCur",
                        0x07 => "CDate",
                        0x05 => "CDbl",
                        0x02 => "CInt",
                        0x03 => "CLng",
                        0x0d => "CLngLng",
                        0x01 => "CLngPtr",
                        0x04 => "CSng",
                        0x08 => "CStr",
                        0x00 => "CVar",
                        _ => "CUnknown",
                    };
                    self.stack.map_last(|l| *l = format!("{}({})", ctype, l));
                }
                0x59 => {
                    // variant conv funcs
                    let ctype = if up == 0x0a { "CVErr" } else { "CVDate" };
                    self.stack.map_last(|l| *l = format!("{}({})", ctype, l));
                }

                0x7d => {
                    // CurDir (legacy)
                    self.stack.map_last(|l| *l = format!("CurDir({})", l));
                }
                0x7e => {
                    // Dir (legacy)
                    let args = self.stack.split_tail(2).join(", ");
                    self.stack.append(format!("Dir({})", args));
                }
                0x81 => {
                    // Error (legacy)
                    self.stack.map_last(|l| *l = format!("Error({})", l));
                }
                0x82 => {
                    // Format (legacy)
                    let args = self.stack.split_tail(2).join(", ");
                    self.stack.append(format!("Format({})", args));
                }
                0x83 => {
                    // FreeFile (legacy)
                    self.stack.map_last(|l| *l = format!("FreeFile({})", l));
                }
                0x84 | 0x85 | 0x86 => {
                    // InStr
                    let nargs: usize = (op - 0x82).into();
                    let args = self.stack.split_tail(nargs).join(", ");
                    self.stack.append(format!("InStr({})", args));
                }
                0x87 | 0x88 | 0x89 => {
                    // InStrB
                    let nargs: usize = (op - 0x85).into();
                    let args = self.stack.split_tail(nargs).join(", ");
                    self.stack.append(format!("InStrB({})", args));
                }
                0x8a | 0x91 => {
                    // XBound
                    let instr = if op == 0x8a { "LBound" } else { "UBound" };
                    let nargs: usize = if self.code.get_u16()? == 0 { 1 } else { 2 };
                    let args = self.stack.split_tail(nargs).join(", ");
                    self.stack.append(format!("{}({})", instr, args));
                }
                0x8b | 0x8c => {
                    // Mid/MidB (legacy)
                    let instr = if op == 0x8b { "Mid" } else { "MidB" };
                    let args = self.stack.split_tail(3).join(", ");
                    self.stack.append(format!("{}({})", instr, args));
                }
                0x8d | 0x8e => {
                    // StrComp(2 and 3)
                    let args = self.stack.split_tail((op - 0x8b).into()).join(", ");
                    self.stack.append(format!("StrComp({})", args));
                }
                0x8f | 0x90 => {
                    // String (legacy)
                    let instr = if op == 0x8f { "String" } else { "String$" };
                    let args = self.stack.split_tail(2).join(", ");
                    self.stack.append(format!("{}({})", instr, args));
                }
                0x72 => {
                    // Erase
                    let args = get_args!();
                    self.stack.append(format!("Erase {}", args));
                }
                0xe1 => {
                    // PSet
                    self.code.get_u16()?; // FIXME: is this always 2?
                    let obj = self.stack.pop().unwrap_or_else(|| "UnkObj".to_string());
                    let args = self.stack.split_tail(3)[0..2].join(", "); // Last arg is 0 and is ignored
                    self.stack.append(format!("{}.PSet ({})", obj, args));
                }

                // two operand ops
                0x00..=0x14 if up == 0 => {
                    // Note: weird things happen if up is !=0
                    let args = self.stack.split_tail(2);
                    let opstr = match op {
                        0x00 => "Imp",
                        0x01 => "Eqv",
                        0x02 => "Xor",
                        0x03 => "Or",
                        0x04 => "And",
                        0x05 => "=",
                        0x06 => "<>",
                        0x07 => "<=",
                        0x08 => ">=",
                        0x09 => "<",
                        0x0a => ">",
                        0x0b => "+",
                        0x0c => "-",
                        0x0d => "Mod",
                        0x0e => "\\",
                        0x0f => "*",
                        0x10 => "/",
                        0x11 => "&",
                        0x12 => "Like",
                        0x13 => "^",
                        0x14 => "Is",
                        _ => unreachable!(),
                    };
                    self.stack
                        .append(format!("{} {} {}", args[0], opstr, args[1]));
                }
                // single operand ops
                0x15 => {
                    // Not
                    self.stack.map_last(|l| *l = format!("Not {}", l));
                }
                0x16 => {
                    // -
                    self.stack.map_last(|l| *l = format!("-{}", l));
                }
                // Math
                0x17 => {
                    self.stack.map_last(|l| *l = format!("Abs({})", l));
                }
                0x18 => {
                    self.stack.map_last(|l| *l = format!("Fix({})", l));
                }
                0x19 => {
                    self.stack.map_last(|l| *l = format!("Int({})", l));
                }
                0x1a => {
                    self.stack.map_last(|l| *l = format!("Sgn({})", l));
                }
                // Len
                0x1b => {
                    self.stack.map_last(|l| *l = format!("Len({})", l));
                }
                0x1c => {
                    self.stack.map_last(|l| *l = format!("LenB({})", l));
                }
                0x1e => {
                    // #
                    self.stack.map_last(|l| *l = format!("#{}", l));
                }
                0x1d => {
                    // () wrappage
                    self.stack.map_last(|l| *l = format!("({})", l));
                }

                0x73 => {
                    // Error
                    self.stack.map_last(|l| *l = format!("Error {}", l));
                }

                0xbf | 0xea => {
                    // XSet
                    let instr = if op == 0xbf { "LSet" } else { "RSet" };
                    let args = self.stack.split_tail(2);
                    self.stack
                        .append(format!("{} {} = {}", instr, args[1], args[0]));
                }

                0xc6 | 0xc7 => {
                    // Mid
                    let opstr = if op == 0xc6 { "Mid" } else { "MidB" };
                    let dol = if up & 0x8 != 0 { "$" } else { "" };
                    let mut args = self.stack.split_tail(3);
                    if args[2].is_empty() {
                        args.pop();
                    }
                    let val = self
                        .stack
                        .pop()
                        .unwrap_or_else(|| "UnknownValue".to_string());
                    self.stack
                        .append(format!("{}{}({}) = {}", opstr, dol, args.join(", "), val));
                }

                0xc8 => {
                    let args = self.stack.split_tail(2);
                    self.stack
                        .append(format!("Name {} As {}", args[0], args[1]));
                }

                // Jumps
                0xa3 => {
                    // Label:
                    let label = get_id_str!("UnkLab");
                    self.stack.push_label(format!("{}:", label));
                }
                0xa8 => {
                    // Line number
                    let label = get_id_str!("UnkLine");
                    self.stack.push_label(label);
                }
                0x99 | 0x9a => {
                    let label = get_id_str!("UnkLab");
                    let jmptype = if op == 0x99 { "GoSub" } else { "GoTo" };
                    self.stack.append(format!("{} {}", jmptype, label));
                }
                0xe9 => self.stack.append("Return".to_string()),
                0xcc => {
                    // On Error...
                    let label = get_id_str!("UnkLab");
                    let local = if up & 0x04 != 0 { "Local " } else { "" };
                    if up & 0x01 != 0 {
                        self.stack.append(format!("On {}Error Resume Next", local));
                    } else if up & 0x02 != 0 {
                        self.stack.append(format!("On {}Error GoTo 0", local));
                    } else if up & 0x10 != 0 {
                        self.stack.append(format!("On {}Error GoTo -1", local));
                    } else {
                        self.stack
                            .append(format!("On {}Error GoTo {}", local, label));
                    }
                }
                0xcd | 0xce => {
                    // On Cond...
                    let ntargets = self.code.get_u16()? >> 1;
                    let mut targets: Vec<&str> = Vec::with_capacity(ntargets.into());
                    for _ in 0..ntargets {
                        targets.push(get_id_str!("UnkLab"));
                    }
                    let cond = self.stack.pop().unwrap_or_else(|| "UnkCond".to_string());
                    let jmptype = if op == 0xcd { "GoSub" } else { "GoTo" };
                    self.stack
                        .append(format!("On {} {} {}", cond, jmptype, targets.join(", ")));
                }
                0xe8 => {
                    // Resume
                    let label = get_id_str!("UnkLab");
                    let whatres = match up {
                        1 => "Resume Next".to_string(),
                        2 => "Resume 0".to_string(),
                        8 => "Resume".to_string(),
                        _ => format!("Resume {}", label),
                    };
                    self.stack.append(whatres);
                }
                0xf2 => {
                    // Stop
                    self.stack.append("Stop".to_string())
                }
                0x49 => {
                    // AddressOf
                    let name = get_mangled_name!("UnkProc");
                    self.stack.append(format!("AddressOf {}", name));
                }
                0x4a => {
                    // AddressOf obj
                    let name = get_mangled_name!("UnkProc");
                    let obj = self.stack.pop().unwrap_or_else(|| "UnkObj".to_string());
                    self.stack.append(format!("AddressOf {}.{}", obj, name));
                }

                0xe3 => {
                    // '-style comment
                    let column: usize = self.code.get_u16()?.into();
                    let stacklen = self.stack.to_string().len() + usize::from(self.line.indent) + 1;
                    let pad = column.saturating_sub(stacklen);
                    let comment = self.code.get_string()?;
                    let line = format!("{:pad$}'{}", "", comment, pad = pad);
                    self.stack.append(line);
                }
                0xe7 => {
                    // Rem comment
                    let s = self.code.get_string()?;
                    self.stack.append(format!("Rem{}", s));
                }
                0xe6 => {
                    // Non-compiled
                    let code = self.code.get_string()?;
                    self.stack.append(code);
                }

                // I/O
                0xcf => {
                    // Open
                    let mode = self.code.get_u16()?;
                    let modestr = match mode & 0xff {
                        0x01 => "Input",
                        0x02 => "Output",
                        0x04 => "Random",
                        0x08 => "Append",
                        0x20 => "Binary",
                        _ => "0",
                    };
                    let access = match (mode >> 8) & 0x0f {
                        1 => " Access Read",
                        2 => " Access Write",
                        3 => " Access Read Write",
                        _ => "",
                    };
                    let lock = match mode >> 12 {
                        1 => " Lock Read Write",
                        2 => " Lock Write",
                        3 => " Lock Read",
                        4 => " Shared",
                        _ => "",
                    };

                    let len = self.stack.pop().unwrap_or_else(|| "???".to_string());
                    let len = if len.is_empty() {
                        // Note: this typically happens via op 0xab (default value)
                        //   however op 0xd3 is handled in the same fashion
                        // Note: op 0xab with up&1 set results in "Len =" shown but
                        //   there is little value in handling this "properly"
                        len
                    } else {
                        format!(" Len = {}", len)
                    };
                    let fnum = self.stack.pop().unwrap_or_else(|| "???".to_string());
                    let fnam = self.stack.pop().unwrap_or_else(|| "???".to_string());
                    self.stack.append(format!(
                        "Open {} For {}{}{} As {}{}",
                        fnam, modestr, access, lock, fnum, len,
                    ));
                }
                0x56 => {
                    // Close file(s)
                    let args = get_args!();
                    self.stack.append(format!("Close {}", args));
                }
                0x57 => {
                    // Close all
                    self.stack.append("Close".to_string());
                }
                0x98 | 0xe2 => {
                    // Get | Put
                    let instr = if op == 0x98 { "Get" } else { "Put" };
                    let args = self.stack.split_tail(3).join(", ");
                    self.stack.append(format!("{} {}", instr, args));
                }
                0xec => {
                    // Seek
                    let args = self.stack.split_tail(2).join(", ");
                    self.stack.append(format!("Seek {}", args));
                }
                0xbb | 0xf4 => {
                    // Lock | Unlock
                    let instr = if op == 0xbb { "Lock" } else { "Unlock" };
                    let args = self.stack.split_tail(3);
                    self.stack.append(match up & 3 {
                        0 => format!("{} {}, {} To {}", instr, args[0], args[1], args[2]),
                        1 => format!("{} {}, To {}", instr, args[0], args[2]),
                        2 => format!("{} {}, {}", instr, args[0], args[1]),
                        3 => format!("{} {}", instr, args[0]),
                        _ => unreachable!(),
                    });
                }
                // Line Input #
                0xa7 => {
                    let args = self.stack.split_tail(2);
                    self.stack
                        .append(format!("Line Input #{}, {}", args[0], args[1]));
                }
                // Input #
                0xa0 => {
                    let fnum = self.stack.pop().unwrap_or_else(|| "???".to_string());
                    self.stack.append(format!("Input {},", fnum));
                }
                0xa2 => {
                    // Input# variable list separator
                    self.stack.map_last(|l| *l += ",");
                }
                0xa1 => {
                    // Input# variable list terminator
                    self.stack.map_last(|l| {
                        if l.ends_with(',') {
                            l.truncate(l.len() - 1);
                        }
                    });
                }
                // Print, Print #, Write #
                0xdc => {
                    let obj = self.stack.pop().unwrap_or_else(|| "".to_string());
                    self.stack.append(obj + "Print");
                }
                0xd5 => {
                    let fnum = self.stack.pop().unwrap_or_else(|| "???".to_string());
                    self.stack.append(format!("Print {},", fnum));
                }
                0xf9 => {
                    let fnum = self.stack.pop().unwrap_or_else(|| "???".to_string());
                    self.stack.append(format!("Write {},", fnum));
                }
                0xd8 => {
                    // Print# variable list separator
                    self.stack.map_last(|l| *l += ",");
                }
                0xd6 => {
                    // Print# variable list separator (alone)
                    if last_is_spc_or_tab {
                        self.stack.map_last(|l| *l += ",");
                    } else {
                        self.stack.append(",");
                    }
                }
                0xda => {
                    // Print# next charpos
                    self.stack.map_last(|l| *l += ";");
                }
                0xdd => {
                    // Print# next charpos (alone)
                    if last_is_spc_or_tab {
                        self.stack.map_last(|l| *l += ";");
                    } else {
                        self.stack.append(";");
                    }
                }
                0xde => {
                    // Print# Spc() charpos
                    self.stack.map_last(|l| *l = format!("Spc({})", l));
                }
                0xe0 => {
                    // Print# Tab charpos (alone)
                    self.stack.append("Tab".to_string());
                }
                0xdf => {
                    // Print# Tab() charpos
                    self.stack.map_last(|l| *l = format!("Tab({})", l));
                }
                0xd9 => {
                    // Print# regular terminator
                }
                0xdb => {
                    // Print# noargs terminator
                }
                0xd7 => {
                    // Print# comma terminator
                }

                // Special objects
                0xc0 => {
                    self.stack.append("Me");
                }
                0xc1 => {
                    // Implicit self
                }
                0x5b => {
                    self.stack.append("Debug.".to_string());
                }
                0x45 => {
                    self.stack.map_last(|l| *l = format!("Debug.Assert {}", l));
                }

                0x46 => {
                    // :
                    let column: usize = self.code.get_u16()?.into();
                    let stacklen = self.stack.to_string().len() + usize::from(self.line.indent) + 2;
                    let pad = column.saturating_sub(stacklen);
                    let last = self.stack.pop().unwrap_or_else(|| "".to_string());
                    let line = format!("{}:{:pad$}", last, "", pad = pad);
                    dim_items = 0;
                    self.stack.append(line);
                }
                0xa6 => {
                    // line continuation

                    // TODO: supporting this is non trivial
                    // The first u16 contains the size in bytes of the break pairs
                    // The rest of the code contains the break pairs
                    //   Each brake pair consists of 2x u16's
                    //   The first element is the position of the next break (i.e. where "_\n" is inserted)
                    //     The position is in number of tokens where a token is each space separated
                    //     element in the rendered line considering however that string-like elements
                    //     are always considered as a whole (i.e. they always constitute a single element
                    //     even whey they contain spaces).
                    //     Also note that:
                    //       - 0x46 is counted as an element although it's rendered with no spacing from
                    //         its preceding element
                    //       - 0xa3 is similarly counted as two elements (with an invisible space between
                    //         the label and the colon - this gap is indeed suitable for line cont)
                    //     String-like elements are: 0xb9, 0xe3, 0xe6, 0xe7
                    //   The second element is the indentation to apply after the break
                    let nentries = self.code.get_u16()?;
                    for _ in 0..nentries / 2 {
                        self.code.get_u16()?;
                    }
                }

                0x5c => {
                    // DefXXX
                    let instr = match up {
                        1 => "DefLngPtr",
                        2 => "DefInt",
                        3 => "DefLng",
                        4 => "DefSng",
                        5 => "DefDbl",
                        6 => "DefCur",
                        7 => "DefDate",
                        8 => "DefStr",
                        9 => "DefObj",
                        11 => "DefBool",
                        12 => "DefVar",
                        13 => "DefLngLng",
                        14 => "DefDec",
                        17 => "DefByte",
                        20 => "DefLngPtr",

                        // Some of these result in garbage, but it's likely just a glitch
                        _ => "DefUnk",
                    };
                    let val = self.code.get_u32()?;
                    let mut s = String::new();
                    let mut it = (0u8..26).filter(|i| val & 1 << i != 0);
                    let mut cur = it.next();
                    while cur.is_some() {
                        let startv = cur.unwrap();
                        s += ", ";
                        s.push(char::from(65 + startv));
                        let mut curv = startv;
                        loop {
                            let next = it.next();
                            if next == Some(curv + 1) {
                                curv += 1;
                                continue;
                            }
                            if curv != startv {
                                s.push('-');
                                s.push(char::from(65 + curv));
                            }
                            cur = next;
                            break;
                        }
                    }
                    self.stack
                        .append(format!("{} {}", instr, s.get(2..).unwrap_or("???")));
                }

                // Option
                0xd0 => {
                    let option = match up {
                        0 => "Base 0",
                        1 => "Base 1",
                        2 => "Compare Text",
                        3 => "Compare Binary",
                        4 => "Explicit",
                        5 => "Private Module",
                        7 => "Compare Database",
                        _ => "UnkOpt",
                    };
                    self.stack.append(format!("Option {}", option));
                }

                0x9f => {
                    let off = self.code.get_u32()?;
                    let classname = if let Some(mut var) = Var::from_dim_data(self.functbl, off) {
                        // Office reuses the dim struct but ignores the flags
                        var.flags = 0x8024;
                        var.as_implements(self.functbl, self.type_names)
                    } else {
                        None
                    };
                    self.stack.append(format!(
                        "Implements {}",
                        classname.unwrap_or_else(|| "UnkObj".to_string())
                    ));
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Unknown opcode: {:02x} up:{:02x}\n{:02x?}\n{:?}",
                            op, up, &self.code.codebuf, self.stack
                        ),
                    ));
                }
            }

            last_is_spc_or_tab = matches!(op, 0xde | 0xdf | 0xe0);
        }

        let mut res =
            vec![String::from(" ").repeat(self.line.indent.into()) + &self.stack.to_string()];
        res.extend(attributes);
        Ok(res.join("\n"))
    }
}

#[derive(Debug)]
/// A table of VBA type names
///
/// Exposed for raw digging and statistical analysis
pub struct TypeNames {
    sys_kind: u32,
    /// Unparsed bytes
    pub header: Vec<u16>,
    /// The declared types
    ///
    /// Use [get_name()](Self::get_name())
    pub types: HashMap<u32, Option<String>>,
    /// No clue
    pub unk: u16,
    /// The VB_Base Attribute
    pub vb_base: Option<String>,
    /// The total number of slots available for types
    pub total_types: u32,
    /// The number of slots reserved for specific types
    pub reserved_types: u32,
    /// The number of types actually declared
    pub mapped_types: u32,
}

impl TypeNames {
    fn new<R: Read>(project: &ProjectPC, f: &mut R) -> Result<Self, io::Error> {
        let mut header: Vec<u16> = Vec::with_capacity(0x45);
        for _ in 0..0x45 {
            header.push(rdu16le(f)?);
        }
        if header[0] != 0x00df {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid magic {:04x}, expected df00", header[0]),
            ));
        }
        let len = rdu32le(f)?;
        let mut types: HashMap<u32, Option<String>> = HashMap::new();
        let mut multi_token: Vec<(u32, u16)> = Vec::new();
        let mut reserved_types = 0u32;
        for i in 0..(len / 10) {
            let refcount = rdu16le(f)?;
            let map_type = rdu16le(f)?;
            let rsvd_id = rdu16le(f)?;
            let map_id = rdu16le(f)?;
            let _zero = rdu16le(f)?; // Normal: 0, rare: 1, extremely rare: 2
            if map_type & 2 != 0 {
                let tok = Self::get_single_token_string(project, rsvd_id);
                if tok.is_some() {
                    reserved_types += 1;
                }
                debug!(
                    "PCODE:type_names {:x}: RSVD \"{}\" (refs: {})",
                    i,
                    tok.unwrap_or("???"),
                    refcount
                );
            } else if map_type & 1 == 0 {
                let tok = Self::get_single_token_string(project, map_id).map(|s| s.to_owned());
                types.insert(i, tok);
            } else {
                multi_token.push((i, map_id));
            }
        }
        // FIXME: should probably sink len % 10 just in case

        let _end1 = rdu16le(f)?; // always ffff
        let _end2 = rdu16le(f)?; // always 0101

        // Multi token objects
        // Note: this is likely intended as a table (entries are 32bit aligned via pad)
        // However entries are addressed by offset rather than index (and it's entirely
        // possible to craft entries crossing the entity boundary)
        let multi_len = usize::try_from(rdu32le(f)?).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Multitoken integer conversion failed",
            )
        })?;
        let mut multi_data = vec![0u8; multi_len];
        f.read_exact(&mut multi_data)?;

        for (i, id) in multi_token.into_iter() {
            let tok = Self::get_multi_token_string(project, multi_data.as_slice(), id);
            types.insert(i, tok);
        }

        let unk = rdu16le(f)?;
        let vb_base = ProjectPC::get_string(f).ok();
        let mapped_types = types.len() as u32; // Safe
        Ok(Self {
            sys_kind: project.sys_kind,
            header,
            types,
            unk,
            vb_base,
            total_types: len / 10,
            reserved_types,
            mapped_types,
        })
    }

    /// Get the name of a type ID
    pub fn get_name(&self, id: u16) -> &str {
        let id = if self.sys_kind == 3 { id >> 3 } else { id >> 2 };
        if let Some(Some(tok)) = self.types.get(&id.into()) {
            tok.as_str()
        } else {
            "UnkType"
        }
    }

    fn get_single_token_string(project: &ProjectPC, id: u16) -> Option<&str> {
        project.get_string_from_table_or_builtin(id)
    }

    fn get_multi_token_string(
        project: &ProjectPC,
        multi_data: &[u8],
        offset: u16,
    ) -> Option<String> {
        let offset: usize = offset.into();
        let cnt: usize =
            u16::from_le_bytes(multi_data.get(offset..(offset + 2))?.try_into().unwrap()).into();
        let mut buf = multi_data.get((offset + 2)..(offset + 2 + cnt * 2))?;
        let mut toks: Vec<&str> = Vec::with_capacity(cnt);
        for _ in 0..cnt {
            // Office crashes if some id is unmappable
            let id = rdu16le(&mut buf).ok()?;
            toks.push(project.get_string_from_table_or_builtin(id)?);
        }
        Some(toks.join("."))
    }
}

#[derive(Debug)]
struct Var {
    flags: u16,
    id: u16,
    _unk1: u32,
    _unk2: u32,
    _unk3: u32,
    bltin_or_offset: u32,
    _unk4: u32,
    next_offset: u32,
    arg_flags: u32, // Repurposed as VB_VarHelpID
    from_decorated_proc: bool,
    /*
    flags meaning:
    bit 0: is proc argument (otherwise is dim item)
    bit 1: Module scope
    bit 2: always unset ???
    bit 3: Public
    bit 4: has_suffix
    bit 5: As (explicit)
    bit 6: builtin
    bit 7: ???
    bit 8: same as bit 0
    bit 9: same as bit 0
    bit a: Function scope
    bit b: Const with numeric value
    bit c: Const
    bit d: New
    bit e: always unset ???
    bit f: As (implicit): same as bit 5 + implicit "As Builtin"
     */
}

impl Var {
    fn from_dim_data(functbl: &FuncTbl, varoff: u32) -> Option<Self> {
        let size: u32 = if functbl.sys_kind == 3 { 32 } else { 28 };
        let mut varbuf = functbl.get_slice(varoff, size)?;
        let ret = Self {
            flags: rdu16le(&mut varbuf).unwrap(),
            id: rdu16le(&mut varbuf).unwrap(),
            _unk1: rdu32le(&mut varbuf).unwrap(), // used with consts
            _unk2: rdu32le(&mut varbuf).unwrap(), // the const value, if numeric (see flags bit b)
            _unk3: if functbl.sys_kind == 3 {
                rdu32le(&mut varbuf).unwrap() // usually 0xffff
            } else {
                0xffffffff
            },
            // Note: this is either u16 or u32; the upper part is chopped off when not needed
            bltin_or_offset: rdu32le(&mut varbuf).unwrap(),
            // These fields can contain rubbish, depending on whether this is a dim item,
            // a proc argument, etc. It's just convenient to reuse the same interface
            _unk4: rdu32le(&mut varbuf).unwrap(),
            next_offset: rdu32le(&mut varbuf).unwrap(),
            arg_flags: rdu32le(&mut varbuf).unwrap(),
            from_decorated_proc: false,
        };
        debug!("PCODE:var: {:x?}", ret);
        Some(ret)
    }

    fn from_redim_data(id: u16, bltin_or_offset: u32, redim_as: bool, as_new: bool) -> Self {
        let flags: u16 = if redim_as {
            if as_new {
                0b10000000100000
            } else {
                0b100000
            }
        } else {
            0
        };
        Self {
            flags,
            id,
            _unk1: 0,
            _unk2: 0,
            _unk3: 0,
            bltin_or_offset,
            _unk4: 0,
            next_offset: 0,
            arg_flags: 0,
            from_decorated_proc: false,
        }
    }

    fn from_proc(proc: &Procedure) -> Self {
        let flags = if !proc.has_explicit_return() {
            0b1000000
        } else if proc.has_builtin_ret() {
            0b1100000
        } else {
            0b00000000100000
        };
        if proc.is_decorated() {
            debug!("DECORATED!");
        }
        let ret = Self {
            flags,
            id: 0xfffe,
            _unk1: 0,
            _unk2: 0,
            _unk3: 0,
            bltin_or_offset: proc.ret_bltin_or_offset,
            _unk4: 0,
            next_offset: 0,
            arg_flags: 0,
            from_decorated_proc: proc.is_decorated(),
        };
        debug!("PCODE:ret: {:x?}", ret);
        ret
    }

    fn vartype(typeid: u16) -> &'static str {
        match typeid & 0xbf {
            // See VarType() docs
            // 0 => "Empty", // Forged only
            // 1 => "Null", // Forged only
            0x02 => "Integer",
            0x03 => "Long",
            0x04 => "Single",
            0x05 => "Double",
            0x06 => "Currency",
            0x07 => "Date",
            0x08 => "String",
            0x09 => "Object",
            0x0a => "Error", // Forged only
            0x0b => "Boolean",
            0x0c => "Variant",
            // 0x0d => "DataObject", // Forged only
            0x0e => "Decimal", // Forged only
            0x11 => "Byte",
            0x14 => "LongLong",
            0x18 => "Any",
            0x1b => "Any", // Forged only
            0x83 => "LongPtr",
            0x94 => "LongPtr",
            _ => {
                debug!("PCODE: Unknown type {:x}", typeid);
                "<UnkType>"
            }
        }
    }

    fn varsuffix(typeid: u16) -> &'static str {
        // Note: these mappings are different from the "up" mappings
        match typeid & 0xbf {
            // See "Declaring variables" docs
            2 => "%",
            3 => "&",
            4 => "!",
            5 => "#",
            6 => "@",
            8 => "$",
            20 => "^",
            _ => "",
        }
    }

    fn as_dim(
        &self,
        pc: &ProjectPC,
        stack: &mut Stack,
        functbl: &FuncTbl,
        type_names: &TypeNames,
    ) -> Option<String> {
        self.parse_full(None, stack, functbl, type_names)
            .map(|(tail, bltin_type)| {
                bltin_type
                    .map(|t| self.get_decorated_name(t, pc))
                    .unwrap_or_else(|| self.get_name(pc).to_owned())
                    + &tail
            })
    }

    fn as_redim(
        &self,
        count: u16,
        up: u16,
        pc: &ProjectPC,
        stack: &mut Stack,
        functbl: &FuncTbl,
        type_names: &TypeNames,
    ) -> String {
        format!(
            "{}{}{}",
            self.get_name(pc),
            LineDecompiler::get_suffix(up),
            self.parse_full(Some(count), stack, functbl, type_names)
                .map(|(tail, _)| tail)
                .unwrap_or_else(|| "???".to_string())
        )
    }

    fn as_arg(
        &self,
        pc: &ProjectPC,
        stack: &mut Stack,
        functbl: &FuncTbl,
        type_names: &TypeNames,
    ) -> Option<String> {
        self.as_dim(pc, stack, functbl, type_names)
            .map(|v| {
                if self.arg_flags & 4 != 0 {
                    format!("ByVal {}", v)
                } else if self.arg_flags & 2 != 0 {
                    format!("ByRef {}", v)
                } else if self.arg_flags & 8 != 0 {
                    format!("AddressOf {}", v)
                } else {
                    v
                }
            })
            .map(|v| {
                if self.arg_flags & 0x200 != 0 {
                    format!("Optional {}", v)
                } else {
                    v
                }
            })
            .map(|v| {
                if self.arg_flags & 0x400 != 0 {
                    // for once office doesn't crash when the stack underflows
                    format!("{} = {}", v, stack.pop().unwrap_or_else(|| " ".to_string()))
                } else {
                    v
                }
            })
    }

    fn as_implements(&self, functbl: &FuncTbl, type_names: &TypeNames) -> Option<String> {
        self.parse_simple(functbl, type_names)
    }

    fn as_return_type(
        &self,
        functbl: &FuncTbl,
        type_names: &TypeNames,
    ) -> Option<(String, &'static str)> {
        let ret_type = self.parse_simple(functbl, type_names)?;
        let ret_decor = if self.from_decorated_proc && self.has_implicit_type() {
            Self::varsuffix(self.bltin_or_offset as u16)
        } else {
            ""
        };
        if ret_type.is_empty() {
            Some((ret_type, ret_decor))
        } else {
            Some((format!(" As {}", ret_type), ret_decor))
        }
    }

    fn masked_flags(&self) -> u16 {
        self.flags & 0b0011_0000_0110_0000
    }

    fn has_implicit_type(&self) -> bool {
        self.masked_flags() == 0b100_0000
    }

    fn has_builtin_type(&self) -> bool {
        self.masked_flags() == 0b110_0000
    }

    fn has_suffix(&self) -> bool {
        self.flags & 0b1_0000 != 0
    }

    fn is_const(&self) -> bool {
        self.masked_flags() & 0b0001_0000_0000_0000 != 0
    }

    fn get_const_type(&self) -> Option<u8> {
        if !self.is_const() {
            None
        } else {
            Some(((self.masked_flags() & 0b000_0000_0110_0000) >> 5) as u8)
        }
    }

    fn is_untyped_array(&self) -> bool {
        self.masked_flags() == 0
    }

    fn has_explicit_type(&self) -> bool {
        self.masked_flags() & !0b10000000000000 == 0b00000000100000
    }

    fn has_new(&self) -> bool {
        self.masked_flags() & 0b10000000000000 != 0
    }

    fn get_name<'a>(&self, pc: &'a ProjectPC) -> &'a str {
        if self.id == 0xfffe {
            "Me"
        } else {
            pc.get_string_from_table(self.id).unwrap_or("UnkVar")
        }
    }

    fn has_module_scope(&self) -> bool {
        self.flags & 2 != 0
    }

    fn get_decorated_name(&self, bltin_type: u16, pc: &ProjectPC) -> String {
        let name = self.get_name(pc);
        if self.has_suffix() {
            format!("{}{}", name, Self::varsuffix(bltin_type))
        } else {
            name.to_owned()
        }
    }

    fn parse_full(
        &self,
        force_arcnt: Option<u16>,
        stack: &mut Stack,
        functbl: &FuncTbl,
        type_names: &TypeNames,
    ) -> Option<(String, Option<u16>)> {
        if self.has_implicit_type() {
            // Dim var
            return Some(("".to_string(), Some(self.bltin_or_offset as u16)));
        }

        if self.has_builtin_type() {
            // Dim var As BUILTINTYPE
            return Some((
                format!(" As {}", Self::vartype(self.bltin_or_offset as u16)),
                None,
            ));
        }

        if let Some(const_type) = self.get_const_type() {
            // Const
            let k = stack.pop().unwrap_or_else(|| "?".to_string());
            match const_type {
                0 => {
                    // 0b0000_0000
                    // Weird things happen here
                    return None;
                }
                1 => {
                    // 0b0010_0000
                    // Const var as String * N = k
                    let strtimes = stack.pop().unwrap_or_else(|| "?".to_string());
                    return Some((format!(" As String * {} = {}", strtimes, k), None));
                }
                2 => {
                    // 0b0100_0000
                    // Const var = k
                    return Some((format!(" = {}", k), Some(self.bltin_or_offset as u16)));
                }
                3 => {
                    // 0b0110_0000
                    // Const var as X = k
                    return Some((
                        format!(" As {} = {}", Self::vartype(self.bltin_or_offset as u16), k),
                        None,
                    ));
                }
                // FIXME non builtin const ?
                _ => unreachable!(),
            }
        }

        // Note: sometimes the offset has the low bit set, but Office ignores it
        let mut extrabuf = functbl.get_tail(self.bltin_or_offset & !1)?;
        let extraflags = rdu16le(&mut extrabuf).ok()?;

        if self.is_untyped_array() {
            // Dim var(?)
            if extraflags & 0xff == 0x1b {
                let cntoff = rdu32le(&mut extrabuf).ok()?;
                let bltin_type = rdu16le(&mut extrabuf).ok()?;
                // Dim var(...)
                let arargs = if let Some(cnt) = force_arcnt {
                    debug!(
                        "PCODE:varextra: xf:{:04x} cnt:{:04x} ty:{:04x}",
                        extraflags, cnt, bltin_type
                    );
                    stack.array_args_cnt(cnt)
                } else {
                    debug!(
                        "PCODE:varextra: xf:{:04x} of:{:08x} ty:{:04x}",
                        extraflags, cntoff, bltin_type
                    );
                    stack.array_args(functbl, cntoff)?
                };
                return Some((format!("({})", arargs), Some(bltin_type)));
            } else {
                // Probably unintended, but that what Office shows
                return Some(("".to_string(), None));
            }
        }

        if self.has_explicit_type() {
            // Dim var... As [New] TYPE
            let new = if self.has_new() { "New " } else { "" };
            // Dim var(?) As TYPE
            if extraflags & 0xff == 0x0020 {
                // Dim var As String * N
                debug!("PCODE:varextra: xf:{:04x}", extraflags);
                let strtimes = stack.pop().unwrap_or_else(|| "?".to_string());
                return Some((format!(" As String * {}", strtimes), None));
            }
            if extraflags & 0xff == 0x001d {
                let token = rdu16le(&mut extrabuf).ok()?;
                // Dim var As TYPE
                debug!("PCODE:varextra: xf:{:04x} tk:{:04x}", extraflags, token);
                return Some((format!(" As {}{}", new, type_names.get_name(token)), None));
            }

            let cntoff = rdu32le(&mut extrabuf).ok()?;
            let extra3 = rdu16le(&mut extrabuf).ok()?;
            if extraflags & 0xff == 0x001b {
                // Dim var(...) As ...
                let strtimes = if extra3 & 0xff == 0x0020 {
                    stack.pop().unwrap_or_else(|| "?".to_string())
                } else {
                    "".to_string()
                };
                let arargs = if let Some(cnt) = force_arcnt {
                    debug!(
                        "PCODE:varextra: xf:{:04x} cnt:{:04x} 03:{:04x}",
                        extraflags, cnt, extra3
                    );
                    stack.array_args_cnt(cnt)
                } else {
                    debug!(
                        "PCODE:varextra: xf:{:04x} of:{:08x} 03:{:04x}",
                        extraflags, cntoff, extra3
                    );
                    stack.array_args(functbl, cntoff)?
                };
                if extra3 == 0x0020 {
                    return Some((format!("({}) As String * {}", arargs, strtimes), None));
                } else if extra3 & 0x00ff == 0x001d {
                    // Dim var(...) As TYPE
                    let extra4 = rdu16le(&mut extrabuf).ok()?;
                    debug!("PCODE:varextra: 04: {:04x}", extra4);
                    return Some((
                        format!("({}) As {}{}", arargs, new, type_names.get_name(extra4)),
                        None,
                    ));
                } else {
                    // Dim var(...) As BUILTINTYPE
                    return Some((
                        format!("({}) As {}{}", arargs, new, Self::vartype(extra3)),
                        None,
                    ));
                }
            }
        }
        None
    }

    // A stripped down version of parse_full which handles "Implements" and function return
    // types in declarations. No point in merging despite the logic being roughly the same
    fn parse_simple(&self, functbl: &FuncTbl, type_names: &TypeNames) -> Option<String> {
        if self.has_implicit_type() {
            // (implicit Variant)
            return Some("".to_string());
        }

        if self.has_builtin_type() {
            // .. As BUILTINTYPE
            return Some(Self::vartype(self.bltin_or_offset as u16).to_owned());
        }

        // Note: sometimes the offset has the low bit set, but Office ignores it
        let mut extrabuf = functbl.get_tail(self.bltin_or_offset & !1)?;
        let extratype = rdu16le(&mut extrabuf).ok()?;

        if self.has_explicit_type() {
            // ... As TYPE(?)
            if extratype & 0xff == 0x0020 {
                // Office crashes here; no working sample could be forged
                return None;
            }
            if extratype & 0xff == 0x001d {
                // ... As TYPE
                let token = rdu16le(&mut extrabuf).ok()?;
                debug!("PCODE:varextra: ty:{:04x} tk:{:04x}", extratype, token);
                return Some(type_names.get_name(token).to_owned());
            }

            // Note: Office actually counts array args and typically crashes
            let cntoff: usize = rdu32le(&mut extrabuf).ok()?.try_into().ok()?;
            let extra3 = rdu16le(&mut extrabuf).ok()?;
            if extratype & 0xff == 0x001b {
                // As XXX()
                debug!(
                    "PCODE:varextra: ty:{:04x} of:{:08x} 03:{:04x}",
                    extratype, cntoff, extra3
                );
                if extra3 & 0xff == 0x001d {
                    // As TYPE()
                    let extra4 = rdu16le(&mut extrabuf).ok()?;
                    debug!("PCODE:varextra: 04: {:04x}", extra4);
                    return Some(format!("{}()", type_names.get_name(extra4)));
                } else {
                    // As BUILTINTYPE()
                    return Some(format!("{}()", Self::vartype(extra3)));
                }
            }
        }
        None
    }
}

#[derive(Debug)]
struct Procedure {
    is_5x: bool,
    up: u16, // 1, 2, 5, 6
    flags: u16,
    /// ID of the procedure name
    nameid: u16,
    /// Pointer to the next procedure, -1 on last
    _next_proc_offset: u32,
    /// Attribute VB_UserMemId
    attr_memid: i32,
    /// Attribute VB_HelpID
    attr_helpid: u32,
    /// Attribute VB_Description
    attr_desc: u32,
    /// Attribute VB_ProcData.VB_Invoke_Func
    attr_invoke: u32,
    /// Attribute VB_MemberFlags and others
    attr_flags: u32,
    /// Missing in 5x, 0 otherwise
    _unk02: u32,
    /// Missing in syskind 01, 0 in syskind 03
    _unk03: u32,
    /// Missing in syskind 01; 0 in syskind 03
    _unk04: u32,
    /// -1 if absent, otherwise a pointer to some data struct in the functbl
    /// this appears to contain in turn a set of -1's or some more offsets
    _unk05: u32,
    /// Either 0 or some small negative number (4byte aligned on 01, 8 on 03)
    _unk06: i32,
    /// a tiny integer:
    /// 00, 04, 08, 0a, 0e,
    /// 2b, 2f,
    /// 40, 44, 48, 4a, 4c, 4e
    /// 69, 6b, 6f
    _unk07: u32,
    /// Missing in syskind 01; always 0 in syskind 03
    _unk08: u32,
    first_var_offset: u32,
    /// Pointer in functbl to the first var, or -1 if no var
    ret_bltin_or_offset: u32,
    /// For Declare, a pointer to the imported symbol inside imptbl
    import_offset: u16,
    /// The 0-based ordinal of the procedure (in the _next_proc_offset chain) or -1
    _unk09: u16,
    /// Mostly a repeated word
    _unk10: u32,
    /// A relatively small increasing number, but unlikely an offset
    _unk11: u16,
    /// Missing in syskind 01; 0 in syskind 03
    _unk12: u16,
    /// Missing in syskind 01; 0 in syskind 03
    _unk13: u32,
    /// Flags affecting the return value
    ret_flags: u8,
    /// The number of argument (+1 for the return a value); sometimes the high bit is set for no reason
    argcnt: u8,
    /// The number of trailing Optional args or 0x3f for vararg
    vararg: u8,
    extra_visibility: u8,
}

impl Procedure {
    fn new(functbl: &FuncTbl, offset: u32, up: u16, is_5x: bool) -> Option<Self> {
        let size: u32 = if functbl.sys_kind == 3 { 0x58 } else { 0x40 }; // FIXME correct
        let mut tbl = functbl.get_slice(offset, size)?;
        let flags = rdu16le(&mut tbl).unwrap();
        let nameid = rdu16le(&mut tbl).unwrap();
        let next_proc_offset = rdu32le(&mut tbl).unwrap();
        let attr_memid = rdi32le(&mut tbl).unwrap();
        let attr_helpid = rdu32le(&mut tbl).unwrap();
        let attr_desc = rdu32le(&mut tbl).unwrap();
        let attr_invoke = rdu32le(&mut tbl).unwrap();
        let attr_flags = rdu32le(&mut tbl).unwrap();
        let _unk02 = if is_5x { 0 } else { rdu32le(&mut tbl).unwrap() };
        let _unk03 = if functbl.sys_kind == 1 {
            0
        } else {
            rdu32le(&mut tbl).unwrap()
        };
        let _unk04 = if functbl.sys_kind == 1 {
            0
        } else {
            rdu32le(&mut tbl).unwrap()
        };
        let _unk05 = rdu32le(&mut tbl).unwrap();
        let _unk06: i32 = if functbl.sys_kind == 1 {
            rdi16le(&mut tbl).unwrap().into()
        } else {
            rdi32le(&mut tbl).unwrap()
        };
        let _unk07: u32 = if functbl.sys_kind == 1 {
            rdu16le(&mut tbl).unwrap().into()
        } else {
            rdu32le(&mut tbl).unwrap()
        };
        let _unk08 = if functbl.sys_kind == 1 {
            0
        } else {
            rdu32le(&mut tbl).unwrap()
        };
        let first_var_offset = rdu32le(&mut tbl).unwrap();
        let ret_bltin_or_offset = rdu32le(&mut tbl).unwrap();
        let import_offset = rdu16le(&mut tbl).unwrap();
        let _unk09 = rdu16le(&mut tbl).unwrap();
        let _unk10 = rdu32le(&mut tbl).unwrap();
        let _unk11 = rdu16le(&mut tbl).unwrap();
        let _unk12 = if functbl.sys_kind == 3 && !functbl.has_phantoms {
            rdu16le(&mut tbl).unwrap()
        } else {
            0
        };
        let _unk13 = if functbl.sys_kind == 3 && !functbl.has_phantoms {
            rdu32le(&mut tbl).unwrap()
        } else {
            0
        };
        let ret_flags = rdu8(&mut tbl).unwrap();
        let argcnt = rdu8(&mut tbl).unwrap();
        let vararg = rdu8(&mut tbl).unwrap();
        let extra_visibility = rdu8(&mut tbl).unwrap();
        Some(Self {
            is_5x,
            up,
            flags,
            nameid,
            _next_proc_offset: next_proc_offset,
            attr_memid,
            attr_helpid,
            attr_desc,
            attr_invoke,
            attr_flags,
            _unk02,
            _unk03,
            _unk04,
            _unk05,
            _unk06,
            _unk07,
            _unk08,
            first_var_offset,
            ret_bltin_or_offset,
            import_offset,
            _unk09,
            _unk10,
            _unk11,
            _unk12,
            _unk13,
            ret_flags,
            argcnt,
            vararg,
            extra_visibility,
        })
    }

    fn is_sub_like(&self) -> bool {
        // Typically office only sets one bit at a time: bit0 (for sub-like) and bit1 (for func-like)
        // However testing mixed bits reveals that bit1 is not actually used
        self.up & 1 != 0
    }

    fn is_public(&self) -> bool {
        self.up & 4 != 0
    }

    fn is_static(&self) -> bool {
        self.flags & 0x80 != 0
    }

    fn type_str(&self) -> &str {
        if self.is_sub_like() {
            if self.flags & 0x8000 != 0 {
                "Property Set "
            } else if self.flags & 0x4000 != 0 {
                "Property Let "
            } else {
                "Sub "
            }
        } else if self.up & 0x10 != 0 {
            // Event is op 0x74 however this is what Office does ¯\_(ツ)_/¯
            "Event "
        } else if self.flags & 0x2000 != 0 {
            "Property Get "
        } else {
            "Function "
        }
    }

    fn visibility_str(&self) -> &str {
        if self.is_public() {
            "Public "
        } else {
            if self.is_5x {
                if self.flags & 0x8 != 0 {
                    ""
                } else {
                    "Private "
                }
            } else {
                if self.extra_visibility & 2 != 0 {
                    if self.extra_visibility & 4 != 0 {
                        "Friend "
                    } else {
                        ""
                    }
                } else {
                    "Private "
                }
            }
        }
    }

    fn declare_str(&self) -> &str {
        if self.is_declare() {
            if self.is_ptrsafe() {
                "Declare PtrSafe "
            } else {
                "Declare "
            }
        } else {
            ""
        }
    }

    fn static_str(&self) -> &str {
        if self.is_static() {
            "Static "
        } else {
            ""
        }
    }

    fn is_ptrsafe(&self) -> bool {
        self.extra_visibility & 0x20 != 0
    }

    fn has_explicit_return(&self) -> bool {
        self.flags & 0x30 == 0x20
    }

    fn has_return(&self) -> bool {
        self.ret_flags & 0x20 != 0
    }

    fn has_builtin_ret(&self) -> bool {
        self.ret_flags & 0x8 != 0
    }

    fn is_vararg(&self) -> bool {
        self.vararg & 0x3f == 0x3f
    }

    fn is_declare(&self) -> bool {
        self.import_offset != 0xffff && self.flags & 0x200 != 0
    }

    fn is_decorated(&self) -> bool {
        self.flags & 0x10 != 0
    }

    fn get_argcnt(&self) -> u8 {
        self.argcnt & !0x80
    }

    fn get_attribute(offset: u32, functbl: &FuncTbl, cp: u16) -> Option<String> {
        if offset == 0xffffffff {
            return None;
        }
        let attr_len = functbl.get_u32(offset)?;
        let chars = functbl.get_slice(offset.checked_add(4)?, attr_len)?;
        Some(
            utf8dec_rs::decode_win_str(chars, cp)
                .replace('"', "\"\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r"),
        )
    }

    // FIXME: check if there is a flag enabling or disabling these
    fn attributes(&self, name: &str, functbl: &FuncTbl, cp: u16) -> Vec<String> {
        let mut ret: Vec<String> = Vec::new();
        if let Some(attr) = Self::get_attribute(self.attr_desc, functbl, cp) {
            if !attr.is_empty() {
                ret.push(format!("Attribute {}.VB_Description = \"{}\"", name, &attr));
            }
        }
        if self.attr_helpid != 0 {
            ret.push(format!(
                "Attribute {}.VB_HelpID = {}",
                name, self.attr_helpid
            ))
        }
        if let Some(attr) = Self::get_attribute(self.attr_invoke, functbl, cp) {
            ret.push(format!(
                "Attribute {}.VB_ProcData.VB_Invoke_Func = \"{}\"",
                name, &attr
            ))
        }
        if self.attr_flags & 0x2000 != 0 {
            ret.push(format!(
                "Attribute {}.VB_UserMemId = {}",
                name, self.attr_memid
            ));
        }
        if self.attr_flags & 0x1fff != 0 {
            ret.push(format!(
                "Attribute {}.VB_MemberFlags = \"{:x}\"",
                name,
                self.attr_flags & 0x1fff
            ));
        }
        ret
    }
}

#[derive(Debug)]
/// A helper for custom types
struct EnumType {
    up: u16,
    _flags: u16,
    nameid: u16,
    _next_offset: u32,
    _unk1: u32,
    _unk2: u32,
    _ord: u16,
    visibility: u16,
    _unk3: u32,
}

impl EnumType {
    fn new(functbl: &FuncTbl, offset: u32, up: u16) -> Option<Self> {
        let mut tbl = functbl.get_slice(offset, 0x18)?;
        Some(Self {
            up,
            _flags: rdu16le(&mut tbl).unwrap(),
            nameid: rdu16le(&mut tbl).unwrap(),
            _next_offset: rdu32le(&mut tbl).unwrap(),
            _unk1: rdu32le(&mut tbl).unwrap(),
            _unk2: rdu32le(&mut tbl).unwrap(),
            _ord: rdu16le(&mut tbl).unwrap(),
            visibility: rdu16le(&mut tbl).unwrap(),
            _unk3: rdu32le(&mut tbl).unwrap(),
        })
    }

    fn vis_name(&self) -> &'static str {
        if self.up & 1 != 0 {
            if self.visibility == 1 {
                return "Public ";
            } else if self.visibility == 0 {
                return "Private ";
            }
        }
        ""
    }
}

#[derive(Debug)]
struct FuncTbl {
    sys_kind: u16,
    has_phantoms: bool,
    _header: Vec<u8>,
    data: Vec<u8>,
}

impl FuncTbl {
    fn new<R: Read>(f: &mut R, sys_kind: u16, has_phantoms: bool) -> Result<Self, io::Error> {
        let header_len: usize = if sys_kind == 1 { 0x0e } else { 0x10 };
        let mut header = vec![0u8; header_len];
        f.read_exact(&mut header)?;
        let data_len = u32::from_le_bytes(header[(header_len - 4)..header_len].try_into().unwrap());
        let data_len = usize::try_from(data_len).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cannot convert table size {:#x} to usize", data_len),
            )
        })?;
        let mut data = vec![0u8; data_len + 4];
        f.read_exact(&mut data)?;
        Ok(Self {
            sys_kind,
            has_phantoms,
            _header: header,
            data,
        })
    }

    fn get_u16(&self, offset: u32) -> Option<u16> {
        Some(u16::from_le_bytes(
            self.get_slice(offset, 2)?.try_into().unwrap(),
        ))
    }

    fn get_u32(&self, offset: u32) -> Option<u32> {
        Some(u32::from_le_bytes(
            self.get_slice(offset, 4)?.try_into().unwrap(),
        ))
    }

    fn get_slice(&self, offset: u32, len: u32) -> Option<&[u8]> {
        let start = usize::try_from(offset).ok()?;
        let end = usize::try_from(offset.checked_add(len)?).ok()?;
        self.data.get(start..end)
    }

    fn get_tail(&self, offset: u32) -> Option<&[u8]> {
        let start = usize::try_from(offset).ok()?;
        self.data.get(start..)
    }
}
