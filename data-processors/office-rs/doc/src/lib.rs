//! # Word Binary File Format parser (.doc)
//!
//! This module provides a parser for the *Word Binary File Format* (.doc)
//!
//! Word Binary files are *Compound File Binary Format* structures: check the [ole] crate for details
//!
//! The main interface documentation and code examples are under the [Doc] stuct
//!
//! The implementation was written from scratch based entirely on
//! [\[MS-DOC\]](https://docs.microsoft.com/en-us/openspecs/office_file_formats/ms-doc/ccd7b486-7881-484c-a137-51170af7cc22)
//!

// TODO: extract images, tables, fields, embedded xml, embedded ole
pub mod dop;

use ctxole::{Encryption, NoValidPasswordError, Ole, OleStreamReader, crypto};
use ctxutils::{cmp::*, io::*};
use std::convert::TryInto;
use std::fmt::Debug;
use std::io::{self, Read, Seek};
use std::iter::Iterator;
use std::ops::Range;
use tracing::{debug, warn};
use vba::{Vba, VbaDocument};

/// The fixed-size portion of the [`Fib`]
///
/// See \[MS-DOC\]
#[allow(non_snake_case, missing_docs)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
pub struct FibBase {
    pub wIdent: u16,
    pub nFib: u16,
    pub lid: u16,
    pub pnNext: u16,
    pub fDot: bool,
    pub fGlsy: bool,
    pub fComplex: bool,
    pub fHasPic: bool,
    pub cQuickSaves: u8,
    pub fEncrypted: bool,
    pub fWhichTblStm: bool,
    pub fReadOnlyRecommended: bool,
    pub fWriteReservation: bool,
    pub fExtChar: bool,
    pub fLoadOverride: bool,
    pub fFarEast: bool,
    pub fObfuscated: bool,
    pub nFibBack: u16,
    pub lKey: u32,
    pub fLoadOverridePage: bool,
    pub fcMin: u32,
    pub fcMac: u32,
    pub anomalies: Vec<String>,
}

impl FibBase {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        let mut ret = Self {
            wIdent: u16::from_le_bytes(buf[0..2].try_into().unwrap()),
            nFib: u16::from_le_bytes(buf[2..4].try_into().unwrap()),
            // unused u16::from_le_bytes(buf[4..6].try_into().unwrap())
            lid: u16::from_le_bytes(buf[6..8].try_into().unwrap()),
            pnNext: u16::from_le_bytes(buf[6..8].try_into().unwrap()),
            fDot: (buf[10] & 0b1) != 0,
            fGlsy: (buf[10] & 0b10) != 0,
            fComplex: (buf[10] & 0b100) != 0,
            fHasPic: (buf[10] & 0b1000) != 0,
            cQuickSaves: buf[10] >> 4,
            fEncrypted: (buf[11] & 0b1) != 0,
            fWhichTblStm: (buf[11] & 0b10) != 0,
            fReadOnlyRecommended: (buf[11] & 0b100) != 0,
            fWriteReservation: (buf[11] & 0b1000) != 0,
            fExtChar: (buf[11] & 0b1_0000) != 0,
            fLoadOverride: (buf[11] & 0b10_0000) != 0,
            fFarEast: (buf[11] & 0b100_0000) != 0,
            fObfuscated: (buf[11] & 0b1000_0000) != 0,
            nFibBack: u16::from_le_bytes(buf[12..14].try_into().unwrap()),
            lKey: u32::from_le_bytes(buf[14..18].try_into().unwrap()),
            // envr buf[18],
            // fMac (buf[19] & 0b1),
            // fEmptySpecial (buf[19] & 0b10)
            fLoadOverridePage: (buf[19] & 0b100) != 0,
            //reserved1, reserved2, fSpare0
            //reserved3 [20..22]
            //reserved4 [22..24]
            fcMin: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            fcMac: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            anomalies: Vec::new(),
        };
        if ret.wIdent != 0xa5ec {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid FibBase magic (wIdent is {:x}, should be a5ec)",
                    ret.wIdent
                ),
            ));
        }
        if ret.nFib != 0x00c1 {
            ret.anomalies
                .push(format!("Invalid nFib ({:x} instead of c1)", ret.nFib));
        }
        if ret.pnNext != 0 {
            if ret.fGlsy {
                ret.anomalies
                    .push("Found next Fib in an AutoText only document".to_string());
            }
            if !ret.fDot {
                ret.anomalies
                    .push("Found next Fib in a non-template document".to_string());
            }
        }
        if ret.nFib < 0x00d9 && ret.cQuickSaves != 0xf {
            ret.anomalies
                .push("Found invalid cQuickSaves in \"modern\" document".to_string());
        }
        if ret.nFibBack != 0x00bf && ret.nFibBack != 0x00c1 {
            ret.anomalies
                .push(format!("Found invalid nFibBack {:x}", ret.nFibBack));
        }
        if !ret.fEncrypted && ret.lKey != 0 {
            ret.anomalies
                .push("Document is not encrypted but the encryptiuon lKey is set".to_string());
        }
        if buf[11] & 0b1_0000 == 0 {
            ret.anomalies
                .push("The state of fExtChar is unset".to_string());
        }
        if buf[18] != 0 {
            ret.anomalies
                .push(format!("Value of envr is nonzero {:x}", buf[18]));
        }
        if buf[19] & 0b1 != 0 {
            ret.anomalies.push("The state of fMac is set".to_string());
        }
        if buf[19] & 0b10 != 0 {
            ret.anomalies
                .push("The state of fEmptySpecial is set".to_string());
        }

        Ok(ret)
    }

    fn table_name(&self) -> &'static str {
        if self.fWhichTblStm {
            "1Table"
        } else {
            "0Table"
        }
    }
}

#[allow(non_snake_case)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
pub struct FibRgLw97 {
    pub cbMac: u32,
    pub ccpText: u32,
    pub ccpFtn: u32,
    pub ccpHdd: u32,
    pub ccpAtn: u32,
    pub ccpEdn: u32,
    pub ccpTxbx: u32,
    pub ccpHdrTxbx: u32,
}

impl FibRgLw97 {
    fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, io::Error> {
        let cslw = rdu16le(reader)?;
        if cslw < 44 / 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid cslw ({:x})", cslw),
            ));
        }
        let mut buf = [0u8; 44];
        reader.read_exact(&mut buf)?;
        // ... and skip the tail, whatever the size
        reader.seek(io::SeekFrom::Current(i64::from(cslw - 44 / 4) * 4))?;

        let cbmac = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        // reserved1; reserved2
        let ccptext = i32::from_le_bytes(buf[12..16].try_into().unwrap());
        let ccpftn = i32::from_le_bytes(buf[16..20].try_into().unwrap());
        let ccphdd = i32::from_le_bytes(buf[20..24].try_into().unwrap());
        // reserved3
        let ccpatn = i32::from_le_bytes(buf[28..32].try_into().unwrap());
        let ccpedn = i32::from_le_bytes(buf[32..36].try_into().unwrap());
        let ccptxbx = i32::from_le_bytes(buf[36..40].try_into().unwrap());
        let ccphdrtxbx = i32::from_le_bytes(buf[40..44].try_into().unwrap());
        // reserved4-14
        Ok(Self {
            cbMac: cbmac,
            ccpText: ccptext.try_into().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Found negative value for ccpHdrTxbx ({})", ccphdrtxbx),
                )
            })?,
            ccpFtn: ccpftn.try_into().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Found negative value for ccpFtn ({})", ccpftn),
                )
            })?,
            ccpHdd: ccphdd.try_into().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Found negative value for ccpHdd ({})", ccphdd),
                )
            })?,
            ccpAtn: ccpatn.try_into().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Found negative value for ccpAtn ({})", ccpatn),
                )
            })?,
            ccpEdn: ccpedn.try_into().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Found negative value for ccpEdn ({})", ccpedn),
                )
            })?,
            ccpTxbx: ccptxbx.try_into().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Found negative value for ccpTxbx ({})", ccptxbx),
                )
            })?,
            ccpHdrTxbx: ccphdrtxbx.try_into().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Found negative value for ccpHdrTxbx ({})", ccphdrtxbx),
                )
            })?,
        })
    }
}

#[allow(non_snake_case, dead_code)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
pub struct FibRgFcLcb {
    // FibRgFcLcb97
    pub fcStshfOrig: u32,
    pub lcbStshfOrig: u32,
    pub fcStshf: u32,
    pub lcbStshf: u32,
    pub fcPlcffndRef: u32,
    pub lcbPlcffndRef: u32,
    pub fcPlcffndTxt: u32,
    pub lcbPlcffndTxt: u32,
    pub fcPlcfandRef: u32,
    pub lcbPlcfandRef: u32,
    pub fcPlcfandTxt: u32,
    pub lcbPlcfandTxt: u32,
    pub fcPlcfSed: u32,
    pub lcbPlcfSed: u32,
    pub fcPlcPad: u32,
    pub lcbPlcPad: u32,
    pub fcPlcfPhe: u32,
    pub lcbPlcfPhe: u32,
    pub fcSttbfGlsy: u32,
    pub lcbSttbfGlsy: u32,
    pub fcPlcfGlsy: u32,
    pub lcbPlcfGlsy: u32,
    pub fcPlcfHdd: u32,
    pub lcbPlcfHdd: u32,
    pub fcPlcfBteChpx: u32,
    pub lcbPlcfBteChpx: u32,
    pub fcPlcfBtePapx: u32,
    pub lcbPlcfBtePapx: u32,
    pub fcPlcfSea: u32,
    pub lcbPlcfSea: u32,
    pub fcSttbfFfn: u32,
    pub lcbSttbfFfn: u32,
    pub fcPlcfFldMom: u32,
    pub lcbPlcfFldMom: u32,
    pub fcPlcfFldHdr: u32,
    pub lcbPlcfFldHdr: u32,
    pub fcPlcfFldFtn: u32,
    pub lcbPlcfFldFtn: u32,
    pub fcPlcfFldAtn: u32,
    pub lcbPlcfFldAtn: u32,
    pub fcPlcfFldMcr: u32,
    pub lcbPlcfFldMcr: u32,
    pub fcSttbfBkmk: u32,
    pub lcbSttbfBkmk: u32,
    pub fcPlcfBkf: u32,
    pub lcbPlcfBkf: u32,
    pub fcPlcfBkl: u32,
    pub lcbPlcfBkl: u32,
    pub fcCmds: u32,
    pub lcbCmds: u32,
    pub fcUnused1: u32,
    pub lcbUnused1: u32,
    pub fcSttbfMcr: u32,
    pub lcbSttbfMcr: u32,
    pub fcPrDrvr: u32,
    pub lcbPrDrvr: u32,
    pub fcPrEnvPort: u32,
    pub lcbPrEnvPort: u32,
    pub fcPrEnvLand: u32,
    pub lcbPrEnvLand: u32,
    pub fcWss: u32,
    pub lcbWss: u32,
    pub fcDop: u32,
    pub lcbDop: u32,
    pub fcSttbfAssoc: u32,
    pub lcbSttbfAssoc: u32,
    pub fcClx: u32,
    pub lcbClx: u32,
    pub fcPlcfPgdFtn: u32,
    pub lcbPlcfPgdFtn: u32,
    pub fcAutosaveSource: u32,
    pub lcbAutosaveSource: u32,
    pub fcGrpXstAtnOwners: u32,
    pub lcbGrpXstAtnOwners: u32,
    pub fcSttbfAtnBkmk: u32,
    pub lcbSttbfAtnBkmk: u32,
    pub fcUnused2: u32,
    pub lcbUnused2: u32,
    pub fcUnused3: u32,
    pub lcbUnused3: u32,
    pub fcPlcSpaMom: u32,
    pub lcbPlcSpaMom: u32,
    pub fcPlcSpaHdr: u32,
    pub lcbPlcSpaHdr: u32,
    pub fcPlcfAtnBkf: u32,
    pub lcbPlcfAtnBkf: u32,
    pub fcPlcfAtnBkl: u32,
    pub lcbPlcfAtnBkl: u32,
    pub fcPms: u32,
    pub lcbPms: u32,
    pub fcFormFldSttbs: u32,
    pub lcbFormFldSttbs: u32,
    pub fcPlcfendRef: u32,
    pub lcbPlcfendRef: u32,
    pub fcPlcfendTxt: u32,
    pub lcbPlcfendTxt: u32,
    pub fcPlcfFldEdn: u32,
    pub lcbPlcfFldEdn: u32,
    pub fcUnused4: u32,
    pub lcbUnused4: u32,
    pub fcDggInfo: u32,
    pub lcbDggInfo: u32,
    pub fcSttbfRMark: u32,
    pub lcbSttbfRMark: u32,
    pub fcSttbfCaption: u32,
    pub lcbSttbfCaption: u32,
    pub fcSttbfAutoCaption: u32,
    pub lcbSttbfAutoCaption: u32,
    pub fcPlcfWkb: u32,
    pub lcbPlcfWkb: u32,
    pub fcPlcfSpl: u32,
    pub lcbPlcfSpl: u32,
    pub fcPlcftxbxTxt: u32,
    pub lcbPlcftxbxTxt: u32,
    pub fcPlcfFldTxbx: u32,
    pub lcbPlcfFldTxbx: u32,
    pub fcPlcfHdrtxbxTxt: u32,
    pub lcbPlcfHdrtxbxTxt: u32,
    pub fcPlcffldHdrTxbx: u32,
    pub lcbPlcffldHdrTxbx: u32,
    pub fcStwUser: u32,
    pub lcbStwUser: u32,
    pub fcSttbTtmbd: u32,
    pub lcbSttbTtmbd: u32,
    pub fcCookieData: u32,
    pub lcbCookieData: u32,
    pub fcPgdMotherOldOld: u32,
    pub lcbPgdMotherOldOld: u32,
    pub fcBkdMotherOldOld: u32,
    pub lcbBkdMotherOldOld: u32,
    pub fcPgdFtnOldOld: u32,
    pub lcbPgdFtnOldOld: u32,
    pub fcBkdFtnOldOld: u32,
    pub lcbBkdFtnOldOld: u32,
    pub fcPgdEdnOldOld: u32,
    pub lcbPgdEdnOldOld: u32,
    pub fcBkdEdnOldOld: u32,
    pub lcbBkdEdnOldOld: u32,
    pub fcSttbfIntlFld: u32,
    pub lcbSttbfIntlFld: u32,
    pub fcRouteSlip: u32,
    pub lcbRouteSlip: u32,
    pub fcSttbSavedBy: u32,
    pub lcbSttbSavedBy: u32,
    pub fcSttbFnm: u32,
    pub lcbSttbFnm: u32,
    pub fcPlfLst: u32,
    pub lcbPlfLst: u32,
    pub fcPlfLfo: u32,
    pub lcbPlfLfo: u32,
    pub fcPlcfTxbxBkd: u32,
    pub lcbPlcfTxbxBkd: u32,
    pub fcPlcfTxbxHdrBkd: u32,
    pub lcbPlcfTxbxHdrBkd: u32,
    pub fcDocUndoWord9: u32,
    pub lcbDocUndoWord9: u32,
    pub fcRgbUse: u32,
    pub lcbRgbUse: u32,
    pub fcUsp: u32,
    pub lcbUsp: u32,
    pub fcUskf: u32,
    pub lcbUskf: u32,
    pub fcPlcupcRgbUse: u32,
    pub lcbPlcupcRgbUse: u32,
    pub fcPlcupcUsp: u32,
    pub lcbPlcupcUsp: u32,
    pub fcSttbGlsyStyle: u32,
    pub lcbSttbGlsyStyle: u32,
    pub fcPlgosl: u32,
    pub lcbPlgosl: u32,
    pub fcPlcocx: u32,
    pub lcbPlcocx: u32,
    pub fcPlcfBteLvc: u32,
    pub lcbPlcfBteLvc: u32,
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
    pub fcPlcfLvcPre10: u32,
    pub lcbPlcfLvcPre10: u32,
    pub fcPlcfAsumy: u32,
    pub lcbPlcfAsumy: u32,
    pub fcPlcfGram: u32,
    pub lcbPlcfGram: u32,
    pub fcSttbListNames: u32,
    pub lcbSttbListNames: u32,
    pub fcSttbfUssr: u32,
    pub lcbSttbfUssr: u32,
    pub w2000: Option<FibRgFcLcb2000>,
    pub w2002: Option<FibRgFcLcb2002>,
    pub w2003: Option<FibRgFcLcb2003>,
    pub w2007: Option<FibRgFcLcb2007>,
}

impl FibRgFcLcb {
    fn new<R: Read + Seek>(reader: &mut R, len: u16) -> Result<Self, io::Error> {
        if len < 0x005d {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid FibRgFcLcb length ({})", len),
            ));
        }

        let mut buf = [0u8; 744];
        reader.read_exact(&mut buf)?;
        let mut ret = Self {
            fcStshfOrig: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            lcbStshfOrig: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            fcStshf: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            lcbStshf: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            fcPlcffndRef: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            lcbPlcffndRef: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            fcPlcffndTxt: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            lcbPlcffndTxt: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            fcPlcfandRef: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
            lcbPlcfandRef: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            fcPlcfandTxt: u32::from_le_bytes(buf[40..44].try_into().unwrap()),
            lcbPlcfandTxt: u32::from_le_bytes(buf[44..48].try_into().unwrap()),
            fcPlcfSed: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
            lcbPlcfSed: u32::from_le_bytes(buf[52..56].try_into().unwrap()),
            fcPlcPad: u32::from_le_bytes(buf[56..60].try_into().unwrap()),
            lcbPlcPad: u32::from_le_bytes(buf[60..64].try_into().unwrap()),
            fcPlcfPhe: u32::from_le_bytes(buf[64..68].try_into().unwrap()),
            lcbPlcfPhe: u32::from_le_bytes(buf[68..72].try_into().unwrap()),
            fcSttbfGlsy: u32::from_le_bytes(buf[72..76].try_into().unwrap()),
            lcbSttbfGlsy: u32::from_le_bytes(buf[76..80].try_into().unwrap()),
            fcPlcfGlsy: u32::from_le_bytes(buf[80..84].try_into().unwrap()),
            lcbPlcfGlsy: u32::from_le_bytes(buf[84..88].try_into().unwrap()),
            fcPlcfHdd: u32::from_le_bytes(buf[88..92].try_into().unwrap()),
            lcbPlcfHdd: u32::from_le_bytes(buf[92..96].try_into().unwrap()),
            fcPlcfBteChpx: u32::from_le_bytes(buf[96..100].try_into().unwrap()),
            lcbPlcfBteChpx: u32::from_le_bytes(buf[100..104].try_into().unwrap()),
            fcPlcfBtePapx: u32::from_le_bytes(buf[104..108].try_into().unwrap()),
            lcbPlcfBtePapx: u32::from_le_bytes(buf[108..112].try_into().unwrap()),
            fcPlcfSea: u32::from_le_bytes(buf[112..116].try_into().unwrap()),
            lcbPlcfSea: u32::from_le_bytes(buf[116..120].try_into().unwrap()),
            fcSttbfFfn: u32::from_le_bytes(buf[120..124].try_into().unwrap()),
            lcbSttbfFfn: u32::from_le_bytes(buf[124..128].try_into().unwrap()),
            fcPlcfFldMom: u32::from_le_bytes(buf[128..132].try_into().unwrap()),
            lcbPlcfFldMom: u32::from_le_bytes(buf[132..136].try_into().unwrap()),
            fcPlcfFldHdr: u32::from_le_bytes(buf[136..140].try_into().unwrap()),
            lcbPlcfFldHdr: u32::from_le_bytes(buf[140..144].try_into().unwrap()),
            fcPlcfFldFtn: u32::from_le_bytes(buf[144..148].try_into().unwrap()),
            lcbPlcfFldFtn: u32::from_le_bytes(buf[148..152].try_into().unwrap()),
            fcPlcfFldAtn: u32::from_le_bytes(buf[152..156].try_into().unwrap()),
            lcbPlcfFldAtn: u32::from_le_bytes(buf[156..160].try_into().unwrap()),
            fcPlcfFldMcr: u32::from_le_bytes(buf[160..164].try_into().unwrap()),
            lcbPlcfFldMcr: u32::from_le_bytes(buf[164..168].try_into().unwrap()),
            fcSttbfBkmk: u32::from_le_bytes(buf[168..172].try_into().unwrap()),
            lcbSttbfBkmk: u32::from_le_bytes(buf[172..176].try_into().unwrap()),
            fcPlcfBkf: u32::from_le_bytes(buf[176..180].try_into().unwrap()),
            lcbPlcfBkf: u32::from_le_bytes(buf[180..184].try_into().unwrap()),
            fcPlcfBkl: u32::from_le_bytes(buf[184..188].try_into().unwrap()),
            lcbPlcfBkl: u32::from_le_bytes(buf[188..192].try_into().unwrap()),
            fcCmds: u32::from_le_bytes(buf[192..196].try_into().unwrap()),
            lcbCmds: u32::from_le_bytes(buf[196..200].try_into().unwrap()),
            fcUnused1: u32::from_le_bytes(buf[200..204].try_into().unwrap()),
            lcbUnused1: u32::from_le_bytes(buf[204..208].try_into().unwrap()),
            fcSttbfMcr: u32::from_le_bytes(buf[208..212].try_into().unwrap()),
            lcbSttbfMcr: u32::from_le_bytes(buf[212..216].try_into().unwrap()),
            fcPrDrvr: u32::from_le_bytes(buf[216..220].try_into().unwrap()),
            lcbPrDrvr: u32::from_le_bytes(buf[220..224].try_into().unwrap()),
            fcPrEnvPort: u32::from_le_bytes(buf[224..228].try_into().unwrap()),
            lcbPrEnvPort: u32::from_le_bytes(buf[228..232].try_into().unwrap()),
            fcPrEnvLand: u32::from_le_bytes(buf[232..236].try_into().unwrap()),
            lcbPrEnvLand: u32::from_le_bytes(buf[236..240].try_into().unwrap()),
            fcWss: u32::from_le_bytes(buf[240..244].try_into().unwrap()),
            lcbWss: u32::from_le_bytes(buf[244..248].try_into().unwrap()),
            fcDop: u32::from_le_bytes(buf[248..252].try_into().unwrap()),
            lcbDop: u32::from_le_bytes(buf[252..256].try_into().unwrap()),
            fcSttbfAssoc: u32::from_le_bytes(buf[256..260].try_into().unwrap()),
            lcbSttbfAssoc: u32::from_le_bytes(buf[260..264].try_into().unwrap()),
            fcClx: u32::from_le_bytes(buf[264..268].try_into().unwrap()),
            lcbClx: u32::from_le_bytes(buf[268..272].try_into().unwrap()),
            fcPlcfPgdFtn: u32::from_le_bytes(buf[272..276].try_into().unwrap()),
            lcbPlcfPgdFtn: u32::from_le_bytes(buf[276..280].try_into().unwrap()),
            fcAutosaveSource: u32::from_le_bytes(buf[280..284].try_into().unwrap()),
            lcbAutosaveSource: u32::from_le_bytes(buf[284..288].try_into().unwrap()),
            fcGrpXstAtnOwners: u32::from_le_bytes(buf[288..292].try_into().unwrap()),
            lcbGrpXstAtnOwners: u32::from_le_bytes(buf[292..296].try_into().unwrap()),
            fcSttbfAtnBkmk: u32::from_le_bytes(buf[296..300].try_into().unwrap()),
            lcbSttbfAtnBkmk: u32::from_le_bytes(buf[300..304].try_into().unwrap()),
            fcUnused2: u32::from_le_bytes(buf[304..308].try_into().unwrap()),
            lcbUnused2: u32::from_le_bytes(buf[308..312].try_into().unwrap()),
            fcUnused3: u32::from_le_bytes(buf[312..316].try_into().unwrap()),
            lcbUnused3: u32::from_le_bytes(buf[316..320].try_into().unwrap()),
            fcPlcSpaMom: u32::from_le_bytes(buf[320..324].try_into().unwrap()),
            lcbPlcSpaMom: u32::from_le_bytes(buf[324..328].try_into().unwrap()),
            fcPlcSpaHdr: u32::from_le_bytes(buf[328..332].try_into().unwrap()),
            lcbPlcSpaHdr: u32::from_le_bytes(buf[332..336].try_into().unwrap()),
            fcPlcfAtnBkf: u32::from_le_bytes(buf[336..340].try_into().unwrap()),
            lcbPlcfAtnBkf: u32::from_le_bytes(buf[340..344].try_into().unwrap()),
            fcPlcfAtnBkl: u32::from_le_bytes(buf[344..348].try_into().unwrap()),
            lcbPlcfAtnBkl: u32::from_le_bytes(buf[348..352].try_into().unwrap()),
            fcPms: u32::from_le_bytes(buf[352..356].try_into().unwrap()),
            lcbPms: u32::from_le_bytes(buf[356..360].try_into().unwrap()),
            fcFormFldSttbs: u32::from_le_bytes(buf[360..364].try_into().unwrap()),
            lcbFormFldSttbs: u32::from_le_bytes(buf[364..368].try_into().unwrap()),
            fcPlcfendRef: u32::from_le_bytes(buf[368..372].try_into().unwrap()),
            lcbPlcfendRef: u32::from_le_bytes(buf[372..376].try_into().unwrap()),
            fcPlcfendTxt: u32::from_le_bytes(buf[376..380].try_into().unwrap()),
            lcbPlcfendTxt: u32::from_le_bytes(buf[380..384].try_into().unwrap()),
            fcPlcfFldEdn: u32::from_le_bytes(buf[384..388].try_into().unwrap()),
            lcbPlcfFldEdn: u32::from_le_bytes(buf[388..392].try_into().unwrap()),
            fcUnused4: u32::from_le_bytes(buf[392..396].try_into().unwrap()),
            lcbUnused4: u32::from_le_bytes(buf[396..400].try_into().unwrap()),
            fcDggInfo: u32::from_le_bytes(buf[400..404].try_into().unwrap()),
            lcbDggInfo: u32::from_le_bytes(buf[404..408].try_into().unwrap()),
            fcSttbfRMark: u32::from_le_bytes(buf[408..412].try_into().unwrap()),
            lcbSttbfRMark: u32::from_le_bytes(buf[412..416].try_into().unwrap()),
            fcSttbfCaption: u32::from_le_bytes(buf[416..420].try_into().unwrap()),
            lcbSttbfCaption: u32::from_le_bytes(buf[420..424].try_into().unwrap()),
            fcSttbfAutoCaption: u32::from_le_bytes(buf[424..428].try_into().unwrap()),
            lcbSttbfAutoCaption: u32::from_le_bytes(buf[428..432].try_into().unwrap()),
            fcPlcfWkb: u32::from_le_bytes(buf[432..436].try_into().unwrap()),
            lcbPlcfWkb: u32::from_le_bytes(buf[436..440].try_into().unwrap()),
            fcPlcfSpl: u32::from_le_bytes(buf[440..444].try_into().unwrap()),
            lcbPlcfSpl: u32::from_le_bytes(buf[444..448].try_into().unwrap()),
            fcPlcftxbxTxt: u32::from_le_bytes(buf[448..452].try_into().unwrap()),
            lcbPlcftxbxTxt: u32::from_le_bytes(buf[452..456].try_into().unwrap()),
            fcPlcfFldTxbx: u32::from_le_bytes(buf[456..460].try_into().unwrap()),
            lcbPlcfFldTxbx: u32::from_le_bytes(buf[460..464].try_into().unwrap()),
            fcPlcfHdrtxbxTxt: u32::from_le_bytes(buf[464..468].try_into().unwrap()),
            lcbPlcfHdrtxbxTxt: u32::from_le_bytes(buf[468..472].try_into().unwrap()),
            fcPlcffldHdrTxbx: u32::from_le_bytes(buf[472..476].try_into().unwrap()),
            lcbPlcffldHdrTxbx: u32::from_le_bytes(buf[476..480].try_into().unwrap()),
            fcStwUser: u32::from_le_bytes(buf[480..484].try_into().unwrap()),
            lcbStwUser: u32::from_le_bytes(buf[484..488].try_into().unwrap()),
            fcSttbTtmbd: u32::from_le_bytes(buf[488..492].try_into().unwrap()),
            lcbSttbTtmbd: u32::from_le_bytes(buf[492..496].try_into().unwrap()),
            fcCookieData: u32::from_le_bytes(buf[496..500].try_into().unwrap()),
            lcbCookieData: u32::from_le_bytes(buf[500..504].try_into().unwrap()),
            fcPgdMotherOldOld: u32::from_le_bytes(buf[504..508].try_into().unwrap()),
            lcbPgdMotherOldOld: u32::from_le_bytes(buf[508..512].try_into().unwrap()),
            fcBkdMotherOldOld: u32::from_le_bytes(buf[512..516].try_into().unwrap()),
            lcbBkdMotherOldOld: u32::from_le_bytes(buf[516..520].try_into().unwrap()),
            fcPgdFtnOldOld: u32::from_le_bytes(buf[520..524].try_into().unwrap()),
            lcbPgdFtnOldOld: u32::from_le_bytes(buf[524..528].try_into().unwrap()),
            fcBkdFtnOldOld: u32::from_le_bytes(buf[528..532].try_into().unwrap()),
            lcbBkdFtnOldOld: u32::from_le_bytes(buf[532..536].try_into().unwrap()),
            fcPgdEdnOldOld: u32::from_le_bytes(buf[536..540].try_into().unwrap()),
            lcbPgdEdnOldOld: u32::from_le_bytes(buf[540..544].try_into().unwrap()),
            fcBkdEdnOldOld: u32::from_le_bytes(buf[544..548].try_into().unwrap()),
            lcbBkdEdnOldOld: u32::from_le_bytes(buf[548..552].try_into().unwrap()),
            fcSttbfIntlFld: u32::from_le_bytes(buf[552..556].try_into().unwrap()),
            lcbSttbfIntlFld: u32::from_le_bytes(buf[556..560].try_into().unwrap()),
            fcRouteSlip: u32::from_le_bytes(buf[560..564].try_into().unwrap()),
            lcbRouteSlip: u32::from_le_bytes(buf[564..568].try_into().unwrap()),
            fcSttbSavedBy: u32::from_le_bytes(buf[568..572].try_into().unwrap()),
            lcbSttbSavedBy: u32::from_le_bytes(buf[572..576].try_into().unwrap()),
            fcSttbFnm: u32::from_le_bytes(buf[576..580].try_into().unwrap()),
            lcbSttbFnm: u32::from_le_bytes(buf[580..584].try_into().unwrap()),
            fcPlfLst: u32::from_le_bytes(buf[584..588].try_into().unwrap()),
            lcbPlfLst: u32::from_le_bytes(buf[588..592].try_into().unwrap()),
            fcPlfLfo: u32::from_le_bytes(buf[592..596].try_into().unwrap()),
            lcbPlfLfo: u32::from_le_bytes(buf[596..600].try_into().unwrap()),
            fcPlcfTxbxBkd: u32::from_le_bytes(buf[600..604].try_into().unwrap()),
            lcbPlcfTxbxBkd: u32::from_le_bytes(buf[604..608].try_into().unwrap()),
            fcPlcfTxbxHdrBkd: u32::from_le_bytes(buf[608..612].try_into().unwrap()),
            lcbPlcfTxbxHdrBkd: u32::from_le_bytes(buf[612..616].try_into().unwrap()),
            fcDocUndoWord9: u32::from_le_bytes(buf[616..620].try_into().unwrap()),
            lcbDocUndoWord9: u32::from_le_bytes(buf[620..624].try_into().unwrap()),
            fcRgbUse: u32::from_le_bytes(buf[624..628].try_into().unwrap()),
            lcbRgbUse: u32::from_le_bytes(buf[628..632].try_into().unwrap()),
            fcUsp: u32::from_le_bytes(buf[632..636].try_into().unwrap()),
            lcbUsp: u32::from_le_bytes(buf[636..640].try_into().unwrap()),
            fcUskf: u32::from_le_bytes(buf[640..644].try_into().unwrap()),
            lcbUskf: u32::from_le_bytes(buf[644..648].try_into().unwrap()),
            fcPlcupcRgbUse: u32::from_le_bytes(buf[648..652].try_into().unwrap()),
            lcbPlcupcRgbUse: u32::from_le_bytes(buf[652..656].try_into().unwrap()),
            fcPlcupcUsp: u32::from_le_bytes(buf[656..660].try_into().unwrap()),
            lcbPlcupcUsp: u32::from_le_bytes(buf[660..664].try_into().unwrap()),
            fcSttbGlsyStyle: u32::from_le_bytes(buf[664..668].try_into().unwrap()),
            lcbSttbGlsyStyle: u32::from_le_bytes(buf[668..672].try_into().unwrap()),
            fcPlgosl: u32::from_le_bytes(buf[672..676].try_into().unwrap()),
            lcbPlgosl: u32::from_le_bytes(buf[676..680].try_into().unwrap()),
            fcPlcocx: u32::from_le_bytes(buf[680..684].try_into().unwrap()),
            lcbPlcocx: u32::from_le_bytes(buf[684..688].try_into().unwrap()),
            fcPlcfBteLvc: u32::from_le_bytes(buf[688..692].try_into().unwrap()),
            lcbPlcfBteLvc: u32::from_le_bytes(buf[692..696].try_into().unwrap()),
            dwLowDateTime: u32::from_le_bytes(buf[696..700].try_into().unwrap()),
            dwHighDateTime: u32::from_le_bytes(buf[700..704].try_into().unwrap()),
            fcPlcfLvcPre10: u32::from_le_bytes(buf[704..708].try_into().unwrap()),
            lcbPlcfLvcPre10: u32::from_le_bytes(buf[708..712].try_into().unwrap()),
            fcPlcfAsumy: u32::from_le_bytes(buf[712..716].try_into().unwrap()),
            lcbPlcfAsumy: u32::from_le_bytes(buf[716..720].try_into().unwrap()),
            fcPlcfGram: u32::from_le_bytes(buf[720..724].try_into().unwrap()),
            lcbPlcfGram: u32::from_le_bytes(buf[724..728].try_into().unwrap()),
            fcSttbListNames: u32::from_le_bytes(buf[728..732].try_into().unwrap()),
            lcbSttbListNames: u32::from_le_bytes(buf[732..736].try_into().unwrap()),
            fcSttbfUssr: u32::from_le_bytes(buf[736..740].try_into().unwrap()),
            lcbSttbfUssr: u32::from_le_bytes(buf[740..744].try_into().unwrap()),
            w2000: None,
            w2002: None,
            w2003: None,
            w2007: None,
        };

        if len < 0x006c {
            reader.seek(io::SeekFrom::Current(i64::from(len - 0x005d) * 8))?;
        } else {
            ret.w2000 = Some(FibRgFcLcb2000::new(reader)?);
            if len < 0x0088 {
                reader.seek(io::SeekFrom::Current(i64::from(len - 0x006c) * 8))?;
            } else {
                ret.w2002 = Some(FibRgFcLcb2002::new(reader)?);
                if len < 0x00a4 {
                    reader.seek(io::SeekFrom::Current(i64::from(len - 0x0088) * 8))?;
                } else {
                    ret.w2003 = Some(FibRgFcLcb2003::new(reader)?);
                    if len < 0x00b7 {
                        reader.seek(io::SeekFrom::Current(i64::from(len - 0x00a4) * 8))?;
                    } else {
                        ret.w2007 = Some(FibRgFcLcb2007::new(reader)?);
                        reader.seek(io::SeekFrom::Current(i64::from(len - 0x00b7) * 8))?;
                    }
                }
            }
        }

        Ok(ret)
    }
}

#[allow(non_snake_case, dead_code)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
pub struct FibRgFcLcb2000 {
    pub fcPlcfTch: u32,
    pub lcbPlcfTch: u32,
    pub fcRmdThreading: u32,
    pub lcbRmdThreading: u32,
    pub fcMid: u32,
    pub lcbMid: u32,
    pub fcSttbRgtplc: u32,
    pub lcbSttbRgtplc: u32,
    pub fcMsoEnvelope: u32,
    pub lcbMsoEnvelope: u32,
    pub fcPlcfLad: u32,
    pub lcbPlcfLad: u32,
    pub fcRgDofr: u32,
    pub lcbRgDofr: u32,
    pub fcPlcosl: u32,
    pub lcbPlcosl: u32,
    pub fcPlcfCookieOld: u32,
    pub lcbPlcfCookieOld: u32,
    pub fcPgdMotherOld: u32,
    pub lcbPgdMotherOld: u32,
    pub fcBkdMotherOld: u32,
    pub lcbBkdMotherOld: u32,
    pub fcPgdFtnOld: u32,
    pub lcbPgdFtnOld: u32,
    pub fcBkdFtnOld: u32,
    pub lcbBkdFtnOld: u32,
    pub fcPgdEdnOld: u32,
    pub lcbPgdEdnOld: u32,
    pub fcBkdEdnOld: u32,
    pub lcbBkdEdnOld: u32,
}

impl FibRgFcLcb2000 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 120];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            fcPlcfTch: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            lcbPlcfTch: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            fcRmdThreading: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            lcbRmdThreading: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            fcMid: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            lcbMid: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            fcSttbRgtplc: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            lcbSttbRgtplc: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            fcMsoEnvelope: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
            lcbMsoEnvelope: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            fcPlcfLad: u32::from_le_bytes(buf[40..44].try_into().unwrap()),
            lcbPlcfLad: u32::from_le_bytes(buf[44..48].try_into().unwrap()),
            fcRgDofr: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
            lcbRgDofr: u32::from_le_bytes(buf[52..56].try_into().unwrap()),
            fcPlcosl: u32::from_le_bytes(buf[56..60].try_into().unwrap()),
            lcbPlcosl: u32::from_le_bytes(buf[60..64].try_into().unwrap()),
            fcPlcfCookieOld: u32::from_le_bytes(buf[64..68].try_into().unwrap()),
            lcbPlcfCookieOld: u32::from_le_bytes(buf[68..72].try_into().unwrap()),
            fcPgdMotherOld: u32::from_le_bytes(buf[72..76].try_into().unwrap()),
            lcbPgdMotherOld: u32::from_le_bytes(buf[76..80].try_into().unwrap()),
            fcBkdMotherOld: u32::from_le_bytes(buf[80..84].try_into().unwrap()),
            lcbBkdMotherOld: u32::from_le_bytes(buf[84..88].try_into().unwrap()),
            fcPgdFtnOld: u32::from_le_bytes(buf[88..92].try_into().unwrap()),
            lcbPgdFtnOld: u32::from_le_bytes(buf[92..96].try_into().unwrap()),
            fcBkdFtnOld: u32::from_le_bytes(buf[96..100].try_into().unwrap()),
            lcbBkdFtnOld: u32::from_le_bytes(buf[100..104].try_into().unwrap()),
            fcPgdEdnOld: u32::from_le_bytes(buf[104..108].try_into().unwrap()),
            lcbPgdEdnOld: u32::from_le_bytes(buf[108..112].try_into().unwrap()),
            fcBkdEdnOld: u32::from_le_bytes(buf[112..116].try_into().unwrap()),
            lcbBkdEdnOld: u32::from_le_bytes(buf[116..120].try_into().unwrap()),
        })
    }
}

#[allow(non_snake_case, dead_code)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
pub struct FibRgFcLcb2002 {
    pub fcUnused1: u32,
    pub lcbUnused1: u32,
    pub fcPlcfPgp: u32,
    pub lcbPlcfPgp: u32,
    pub fcPlcfuim: u32,
    pub lcbPlcfuim: u32,
    pub fcPlfguidUim: u32,
    pub lcbPlfguidUim: u32,
    pub fcAtrdExtra: u32,
    pub lcbAtrdExtra: u32,
    pub fcPlrsid: u32,
    pub lcbPlrsid: u32,
    pub fcSttbfBkmkFactoid: u32,
    pub lcbSttbfBkmkFactoid: u32,
    pub fcPlcfBkfFactoid: u32,
    pub lcbPlcfBkfFactoid: u32,
    pub fcPlcfcookie: u32,
    pub lcbPlcfcookie: u32,
    pub fcPlcfBklFactoid: u32,
    pub lcbPlcfBklFactoid: u32,
    pub fcFactoidData: u32,
    pub lcbFactoidData: u32,
    pub fcDocUndo: u32,
    pub lcbDocUndo: u32,
    pub fcSttbfBkmkFcc: u32,
    pub lcbSttbfBkmkFcc: u32,
    pub fcPlcfBkfFcc: u32,
    pub lcbPlcfBkfFcc: u32,
    pub fcPlcfBklFcc: u32,
    pub lcbPlcfBklFcc: u32,
    pub fcSttbfbkmkBPRepairs: u32,
    pub lcbSttbfbkmkBPRepairs: u32,
    pub fcPlcfbkfBPRepairs: u32,
    pub lcbPlcfbkfBPRepairs: u32,
    pub fcPlcfbklBPRepairs: u32,
    pub lcbPlcfbklBPRepairs: u32,
    pub fcPmsNew: u32,
    pub lcbPmsNew: u32,
    pub fcODSO: u32,
    pub lcbODSO: u32,
    pub fcPlcfpmiOldXP: u32,
    pub lcbPlcfpmiOldXP: u32,
    pub fcPlcfpmiNewXP: u32,
    pub lcbPlcfpmiNewXP: u32,
    pub fcPlcfpmiMixedXP: u32,
    pub lcbPlcfpmiMixedXP: u32,
    pub fcUnused2: u32,
    pub lcbUnused2: u32,
    pub fcPlcffactoid: u32,
    pub lcbPlcffactoid: u32,
    pub fcPlcflvcOldXP: u32,
    pub lcbPlcflvcOldXP: u32,
    pub fcPlcflvcNewXP: u32,
    pub lcbPlcflvcNewXP: u32,
    pub fcPlcflvcMixedXP: u32,
    pub lcbPlcflvcMixedXP: u32,
}

impl FibRgFcLcb2002 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 224];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            fcUnused1: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            lcbUnused1: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            fcPlcfPgp: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            lcbPlcfPgp: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            fcPlcfuim: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            lcbPlcfuim: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            fcPlfguidUim: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            lcbPlfguidUim: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            fcAtrdExtra: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
            lcbAtrdExtra: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            fcPlrsid: u32::from_le_bytes(buf[40..44].try_into().unwrap()),
            lcbPlrsid: u32::from_le_bytes(buf[44..48].try_into().unwrap()),
            fcSttbfBkmkFactoid: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
            lcbSttbfBkmkFactoid: u32::from_le_bytes(buf[52..56].try_into().unwrap()),
            fcPlcfBkfFactoid: u32::from_le_bytes(buf[56..60].try_into().unwrap()),
            lcbPlcfBkfFactoid: u32::from_le_bytes(buf[60..64].try_into().unwrap()),
            fcPlcfcookie: u32::from_le_bytes(buf[64..68].try_into().unwrap()),
            lcbPlcfcookie: u32::from_le_bytes(buf[68..72].try_into().unwrap()),
            fcPlcfBklFactoid: u32::from_le_bytes(buf[72..76].try_into().unwrap()),
            lcbPlcfBklFactoid: u32::from_le_bytes(buf[76..80].try_into().unwrap()),
            fcFactoidData: u32::from_le_bytes(buf[80..84].try_into().unwrap()),
            lcbFactoidData: u32::from_le_bytes(buf[84..88].try_into().unwrap()),
            fcDocUndo: u32::from_le_bytes(buf[88..92].try_into().unwrap()),
            lcbDocUndo: u32::from_le_bytes(buf[92..96].try_into().unwrap()),
            fcSttbfBkmkFcc: u32::from_le_bytes(buf[96..100].try_into().unwrap()),
            lcbSttbfBkmkFcc: u32::from_le_bytes(buf[100..104].try_into().unwrap()),
            fcPlcfBkfFcc: u32::from_le_bytes(buf[104..108].try_into().unwrap()),
            lcbPlcfBkfFcc: u32::from_le_bytes(buf[108..112].try_into().unwrap()),
            fcPlcfBklFcc: u32::from_le_bytes(buf[112..116].try_into().unwrap()),
            lcbPlcfBklFcc: u32::from_le_bytes(buf[116..120].try_into().unwrap()),
            fcSttbfbkmkBPRepairs: u32::from_le_bytes(buf[120..124].try_into().unwrap()),
            lcbSttbfbkmkBPRepairs: u32::from_le_bytes(buf[124..128].try_into().unwrap()),
            fcPlcfbkfBPRepairs: u32::from_le_bytes(buf[128..132].try_into().unwrap()),
            lcbPlcfbkfBPRepairs: u32::from_le_bytes(buf[132..136].try_into().unwrap()),
            fcPlcfbklBPRepairs: u32::from_le_bytes(buf[136..140].try_into().unwrap()),
            lcbPlcfbklBPRepairs: u32::from_le_bytes(buf[140..144].try_into().unwrap()),
            fcPmsNew: u32::from_le_bytes(buf[144..148].try_into().unwrap()),
            lcbPmsNew: u32::from_le_bytes(buf[148..152].try_into().unwrap()),
            fcODSO: u32::from_le_bytes(buf[152..156].try_into().unwrap()),
            lcbODSO: u32::from_le_bytes(buf[156..160].try_into().unwrap()),
            fcPlcfpmiOldXP: u32::from_le_bytes(buf[160..164].try_into().unwrap()),
            lcbPlcfpmiOldXP: u32::from_le_bytes(buf[164..168].try_into().unwrap()),
            fcPlcfpmiNewXP: u32::from_le_bytes(buf[168..172].try_into().unwrap()),
            lcbPlcfpmiNewXP: u32::from_le_bytes(buf[172..176].try_into().unwrap()),
            fcPlcfpmiMixedXP: u32::from_le_bytes(buf[176..180].try_into().unwrap()),
            lcbPlcfpmiMixedXP: u32::from_le_bytes(buf[180..184].try_into().unwrap()),
            fcUnused2: u32::from_le_bytes(buf[184..188].try_into().unwrap()),
            lcbUnused2: u32::from_le_bytes(buf[188..192].try_into().unwrap()),
            fcPlcffactoid: u32::from_le_bytes(buf[192..196].try_into().unwrap()),
            lcbPlcffactoid: u32::from_le_bytes(buf[196..200].try_into().unwrap()),
            fcPlcflvcOldXP: u32::from_le_bytes(buf[200..204].try_into().unwrap()),
            lcbPlcflvcOldXP: u32::from_le_bytes(buf[204..208].try_into().unwrap()),
            fcPlcflvcNewXP: u32::from_le_bytes(buf[208..212].try_into().unwrap()),
            lcbPlcflvcNewXP: u32::from_le_bytes(buf[212..216].try_into().unwrap()),
            fcPlcflvcMixedXP: u32::from_le_bytes(buf[216..220].try_into().unwrap()),
            lcbPlcflvcMixedXP: u32::from_le_bytes(buf[220..224].try_into().unwrap()),
        })
    }
}

#[allow(non_snake_case, dead_code)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
pub struct FibRgFcLcb2003 {
    pub fcHplxsdr: u32,
    pub lcbHplxsdr: u32,
    pub fcSttbfBkmkSdt: u32,
    pub lcbSttbfBkmkSdt: u32,
    pub fcPlcfBkfSdt: u32,
    pub lcbPlcfBkfSdt: u32,
    pub fcPlcfBklSdt: u32,
    pub lcbPlcfBklSdt: u32,
    pub fcCustomXForm: u32,
    pub lcbCustomXForm: u32,
    pub fcSttbfBkmkProt: u32,
    pub lcbSttbfBkmkProt: u32,
    pub fcPlcfBkfProt: u32,
    pub lcbPlcfBkfProt: u32,
    pub fcPlcfBklProt: u32,
    pub lcbPlcfBklProt: u32,
    pub fcSttbProtUser: u32,
    pub lcbSttbProtUser: u32,
    pub fcUnused: u32,
    pub lcbUnused: u32,
    pub fcPlcfpmiOld: u32,
    pub lcbPlcfpmiOld: u32,
    pub fcPlcfpmiOldInline: u32,
    pub lcbPlcfpmiOldInline: u32,
    pub fcPlcfpmiNew: u32,
    pub lcbPlcfpmiNew: u32,
    pub fcPlcfpmiNewInline: u32,
    pub lcbPlcfpmiNewInline: u32,
    pub fcPlcflvcOld: u32,
    pub lcbPlcflvcOld: u32,
    pub fcPlcflvcOldInline: u32,
    pub lcbPlcflvcOldInline: u32,
    pub fcPlcflvcNew: u32,
    pub lcbPlcflvcNew: u32,
    pub fcPlcflvcNewInline: u32,
    pub lcbPlcflvcNewInline: u32,
    pub fcPgdMother: u32,
    pub lcbPgdMother: u32,
    pub fcBkdMother: u32,
    pub lcbBkdMother: u32,
    pub fcAfdMother: u32,
    pub lcbAfdMother: u32,
    pub fcPgdFtn: u32,
    pub lcbPgdFtn: u32,
    pub fcBkdFtn: u32,
    pub lcbBkdFtn: u32,
    pub fcAfdFtn: u32,
    pub lcbAfdFtn: u32,
    pub fcPgdEdn: u32,
    pub lcbPgdEdn: u32,
    pub fcBkdEdn: u32,
    pub lcbBkdEdn: u32,
    pub fcAfdEdn: u32,
    pub lcbAfdEdn: u32,
    pub fcAfd: u32,
    pub lcbAfd: u32,
}

impl FibRgFcLcb2003 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 224];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            fcHplxsdr: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            lcbHplxsdr: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            fcSttbfBkmkSdt: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            lcbSttbfBkmkSdt: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            fcPlcfBkfSdt: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            lcbPlcfBkfSdt: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            fcPlcfBklSdt: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            lcbPlcfBklSdt: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            fcCustomXForm: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
            lcbCustomXForm: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            fcSttbfBkmkProt: u32::from_le_bytes(buf[40..44].try_into().unwrap()),
            lcbSttbfBkmkProt: u32::from_le_bytes(buf[44..48].try_into().unwrap()),
            fcPlcfBkfProt: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
            lcbPlcfBkfProt: u32::from_le_bytes(buf[52..56].try_into().unwrap()),
            fcPlcfBklProt: u32::from_le_bytes(buf[56..60].try_into().unwrap()),
            lcbPlcfBklProt: u32::from_le_bytes(buf[60..64].try_into().unwrap()),
            fcSttbProtUser: u32::from_le_bytes(buf[64..68].try_into().unwrap()),
            lcbSttbProtUser: u32::from_le_bytes(buf[68..72].try_into().unwrap()),
            fcUnused: u32::from_le_bytes(buf[72..76].try_into().unwrap()),
            lcbUnused: u32::from_le_bytes(buf[76..80].try_into().unwrap()),
            fcPlcfpmiOld: u32::from_le_bytes(buf[80..84].try_into().unwrap()),
            lcbPlcfpmiOld: u32::from_le_bytes(buf[84..88].try_into().unwrap()),
            fcPlcfpmiOldInline: u32::from_le_bytes(buf[88..92].try_into().unwrap()),
            lcbPlcfpmiOldInline: u32::from_le_bytes(buf[92..96].try_into().unwrap()),
            fcPlcfpmiNew: u32::from_le_bytes(buf[96..100].try_into().unwrap()),
            lcbPlcfpmiNew: u32::from_le_bytes(buf[100..104].try_into().unwrap()),
            fcPlcfpmiNewInline: u32::from_le_bytes(buf[104..108].try_into().unwrap()),
            lcbPlcfpmiNewInline: u32::from_le_bytes(buf[108..112].try_into().unwrap()),
            fcPlcflvcOld: u32::from_le_bytes(buf[112..116].try_into().unwrap()),
            lcbPlcflvcOld: u32::from_le_bytes(buf[116..120].try_into().unwrap()),
            fcPlcflvcOldInline: u32::from_le_bytes(buf[120..124].try_into().unwrap()),
            lcbPlcflvcOldInline: u32::from_le_bytes(buf[124..128].try_into().unwrap()),
            fcPlcflvcNew: u32::from_le_bytes(buf[128..132].try_into().unwrap()),
            lcbPlcflvcNew: u32::from_le_bytes(buf[132..136].try_into().unwrap()),
            fcPlcflvcNewInline: u32::from_le_bytes(buf[136..140].try_into().unwrap()),
            lcbPlcflvcNewInline: u32::from_le_bytes(buf[140..144].try_into().unwrap()),
            fcPgdMother: u32::from_le_bytes(buf[144..148].try_into().unwrap()),
            lcbPgdMother: u32::from_le_bytes(buf[148..152].try_into().unwrap()),
            fcBkdMother: u32::from_le_bytes(buf[152..156].try_into().unwrap()),
            lcbBkdMother: u32::from_le_bytes(buf[156..160].try_into().unwrap()),
            fcAfdMother: u32::from_le_bytes(buf[160..164].try_into().unwrap()),
            lcbAfdMother: u32::from_le_bytes(buf[164..168].try_into().unwrap()),
            fcPgdFtn: u32::from_le_bytes(buf[168..172].try_into().unwrap()),
            lcbPgdFtn: u32::from_le_bytes(buf[172..176].try_into().unwrap()),
            fcBkdFtn: u32::from_le_bytes(buf[176..180].try_into().unwrap()),
            lcbBkdFtn: u32::from_le_bytes(buf[180..184].try_into().unwrap()),
            fcAfdFtn: u32::from_le_bytes(buf[184..188].try_into().unwrap()),
            lcbAfdFtn: u32::from_le_bytes(buf[188..192].try_into().unwrap()),
            fcPgdEdn: u32::from_le_bytes(buf[192..196].try_into().unwrap()),
            lcbPgdEdn: u32::from_le_bytes(buf[196..200].try_into().unwrap()),
            fcBkdEdn: u32::from_le_bytes(buf[200..204].try_into().unwrap()),
            lcbBkdEdn: u32::from_le_bytes(buf[204..208].try_into().unwrap()),
            fcAfdEdn: u32::from_le_bytes(buf[208..212].try_into().unwrap()),
            lcbAfdEdn: u32::from_le_bytes(buf[212..216].try_into().unwrap()),
            fcAfd: u32::from_le_bytes(buf[216..220].try_into().unwrap()),
            lcbAfd: u32::from_le_bytes(buf[220..224].try_into().unwrap()),
        })
    }
}

#[allow(non_snake_case, dead_code)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
pub struct FibRgFcLcb2007 {
    pub fcPlcfmthd: u32,
    pub lcbPlcfmthd: u32,
    pub fcSttbfBkmkMoveFrom: u32,
    pub lcbSttbfBkmkMoveFrom: u32,
    pub fcPlcfBkfMoveFrom: u32,
    pub lcbPlcfBkfMoveFrom: u32,
    pub fcPlcfBklMoveFrom: u32,
    pub lcbPlcfBklMoveFrom: u32,
    pub fcSttbfBkmkMoveTo: u32,
    pub lcbSttbfBkmkMoveTo: u32,
    pub fcPlcfBkfMoveTo: u32,
    pub lcbPlcfBkfMoveTo: u32,
    pub fcPlcfBklMoveTo: u32,
    pub lcbPlcfBklMoveTo: u32,
    pub fcUnused1: u32,
    pub lcbUnused1: u32,
    pub fcUnused2: u32,
    pub lcbUnused2: u32,
    pub fcUnused3: u32,
    pub lcbUnused3: u32,
    pub fcSttbfBkmkArto: u32,
    pub lcbSttbfBkmkArto: u32,
    pub fcPlcfBkfArto: u32,
    pub lcbPlcfBkfArto: u32,
    pub fcPlcfBklArto: u32,
    pub lcbPlcfBklArto: u32,
    pub fcArtoData: u32,
    pub lcbArtoData: u32,
    pub fcUnused4: u32,
    pub lcbUnused4: u32,
    pub fcUnused5: u32,
    pub lcbUnused5: u32,
    pub fcUnused6: u32,
    pub lcbUnused6: u32,
    pub fcOssTheme: u32,
    pub lcbOssTheme: u32,
    pub fcColorSchemeMapping: u32,
    pub lcbColorSchemeMapping: u32,
}

impl FibRgFcLcb2007 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 152];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            fcPlcfmthd: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            lcbPlcfmthd: u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            fcSttbfBkmkMoveFrom: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            lcbSttbfBkmkMoveFrom: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            fcPlcfBkfMoveFrom: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            lcbPlcfBkfMoveFrom: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            fcPlcfBklMoveFrom: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            lcbPlcfBklMoveFrom: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            fcSttbfBkmkMoveTo: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
            lcbSttbfBkmkMoveTo: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            fcPlcfBkfMoveTo: u32::from_le_bytes(buf[40..44].try_into().unwrap()),
            lcbPlcfBkfMoveTo: u32::from_le_bytes(buf[44..48].try_into().unwrap()),
            fcPlcfBklMoveTo: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
            lcbPlcfBklMoveTo: u32::from_le_bytes(buf[52..56].try_into().unwrap()),
            fcUnused1: u32::from_le_bytes(buf[56..60].try_into().unwrap()),
            lcbUnused1: u32::from_le_bytes(buf[60..64].try_into().unwrap()),
            fcUnused2: u32::from_le_bytes(buf[64..68].try_into().unwrap()),
            lcbUnused2: u32::from_le_bytes(buf[68..72].try_into().unwrap()),
            fcUnused3: u32::from_le_bytes(buf[72..76].try_into().unwrap()),
            lcbUnused3: u32::from_le_bytes(buf[76..80].try_into().unwrap()),
            fcSttbfBkmkArto: u32::from_le_bytes(buf[80..84].try_into().unwrap()),
            lcbSttbfBkmkArto: u32::from_le_bytes(buf[84..88].try_into().unwrap()),
            fcPlcfBkfArto: u32::from_le_bytes(buf[88..92].try_into().unwrap()),
            lcbPlcfBkfArto: u32::from_le_bytes(buf[92..96].try_into().unwrap()),
            fcPlcfBklArto: u32::from_le_bytes(buf[96..100].try_into().unwrap()),
            lcbPlcfBklArto: u32::from_le_bytes(buf[100..104].try_into().unwrap()),
            fcArtoData: u32::from_le_bytes(buf[104..108].try_into().unwrap()),
            lcbArtoData: u32::from_le_bytes(buf[108..112].try_into().unwrap()),
            fcUnused4: u32::from_le_bytes(buf[112..116].try_into().unwrap()),
            lcbUnused4: u32::from_le_bytes(buf[116..120].try_into().unwrap()),
            fcUnused5: u32::from_le_bytes(buf[120..124].try_into().unwrap()),
            lcbUnused5: u32::from_le_bytes(buf[124..128].try_into().unwrap()),
            fcUnused6: u32::from_le_bytes(buf[128..132].try_into().unwrap()),
            lcbUnused6: u32::from_le_bytes(buf[132..136].try_into().unwrap()),
            fcOssTheme: u32::from_le_bytes(buf[136..140].try_into().unwrap()),
            lcbOssTheme: u32::from_le_bytes(buf[140..144].try_into().unwrap()),
            fcColorSchemeMapping: u32::from_le_bytes(buf[144..148].try_into().unwrap()),
            lcbColorSchemeMapping: u32::from_le_bytes(buf[148..152].try_into().unwrap()),
        })
    }
}

#[derive(Default, Debug)]
pub struct SttbfAssoc {
    pub strings: [String; 18],
}

impl SttbfAssoc {
    fn new<R: Read + Seek>(reader: &mut R, len: u32) -> Result<Self, io::Error> {
        let mut r = SeekTake::new(reader, len.into());
        let mut ret = Self::default();
        let f_extend = rdu16le(&mut r)?;
        if f_extend != 0xffff {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid fExtend ({:x})", f_extend),
            ));
        }
        let c_data = rdu16le(&mut r)?;
        if c_data != 18 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid fExtend ({:x})", f_extend),
            ));
        }
        let mut buf = [0u8; 255 * 2];
        for idx in 0usize..18 {
            let cch_data = rdu16le(&mut r)?;
            let s_len = umin(usize::from(cch_data) * 2, 255usize * 2);
            r.read_exact(&mut buf[0..s_len])?;
            ret.strings[idx] = utf8dec_rs::decode_utf16le_str(&buf[0..s_len]);
            if cch_data > 255 {
                r.seek(io::SeekFrom::Current(i64::from((cch_data - 255) * 2)))?;
            }
        }
        Ok(ret)
    }
}

/// Information about the document
///
/// See \[MS-DOC\]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug)]
pub struct Fib {
    pub fibbase: FibBase,
    pub nfib: u16,
    pub rglw: FibRgLw97,
    pub rgfclcb: FibRgFcLcb,
}

impl Fib {
    fn new<R: Read + Seek>(reader: &mut R, fibbase: FibBase) -> Result<Self, io::Error> {
        // [MS-DOC] 2.5.15 recommends a loose implementation so we read the least possible
        // amount from each subsequent portion

        let csw = rdu16le(reader)?;
        if csw < 0xe {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid csw ({:x})", csw),
            ));
        }
        // Skip fibRgW as there's nothing interesting there
        reader.seek(io::SeekFrom::Current(i64::from(csw) * 2))?;

        // Read minimal amount of FibRgLw97 and skip the rest
        let fib_rg_lw = FibRgLw97::new(reader)?;

        // Skip fibRgFcLcbBlob for now, to be processed later after the nFib value is determined
        let cb_rg_fc_lcb = rdu16le(reader)?;
        let rg_fc_lcb_blob_pos = reader.stream_position()?;
        reader.seek(io::SeekFrom::Current(i64::from(cb_rg_fc_lcb) * 8))?;

        let csw_new = i64::from(rdu16le(reader)?);
        let n_fib: u16 = if csw_new == 0 {
            // No fibRgCswNew is present: use nFib from FibBase
            fibbase.nFib
        } else {
            // We only care about the first entry of fibRgCswNew (nFibNew), which supersedes
            // the nFib from FibBase
            rdu16le(reader)?
        };

        let mut known_version = false;
        for combo in [
            (0x00c1, 0x005d), // w97
            (0x00d9, 0x006c), // w2000
            (0x0101, 0x0088), // w2002
            (0x010c, 0x00a4), // w2003
            (0x0112, 0x00b7), // 2007
        ] {
            if n_fib == combo.0 {
                known_version = true;
                if cb_rg_fc_lcb != combo.1 {
                    warn!(
                        "Doc: expected fibRgFcLcbBlob size 0x{:x} for nFib 0x{:x} but found 0x{:x}",
                        combo.1, n_fib, cb_rg_fc_lcb
                    );
                }
                break;
            }
        }
        if !known_version {
            warn!("Doc: Unknown nFib version 0x{:x}", n_fib);
        }

        // Back to fibRgFcLcbBlob
        reader.seek(io::SeekFrom::Start(rg_fc_lcb_blob_pos))?;
        let fib_rg_fc_lcb = FibRgFcLcb::new(reader, cb_rg_fc_lcb)?;

        Ok(Fib {
            fibbase,
            nfib: n_fib,
            rglw: fib_rg_lw,
            rgfclcb: fib_rg_fc_lcb,
        })
    }
}

#[derive(Debug)]
struct FcCompressed {
    fc: u32,
}

impl FcCompressed {
    fn is_compressed(&self) -> bool {
        self.fc & (1 << 30) != 0
    }

    fn get_offset(&self) -> u64 {
        let ret = self.fc & 0x3fffffff;
        if self.is_compressed() {
            (ret / 2).into()
        } else {
            ret.into()
        }
    }
}

#[allow(non_snake_case)]
#[derive(Debug)]
struct Pcd {
    _fNoParaLast: bool,
    fc: FcCompressed,
    _prm: u16,
}

impl Pcd {
    fn new(data: &[u8; 8]) -> Self {
        Self {
            _fNoParaLast: data[0] & 1 != 0,
            fc: FcCompressed {
                fc: u32::from_le_bytes(data[2..6].try_into().unwrap()),
            },
            _prm: u16::from_le_bytes(data[6..8].try_into().unwrap()),
        }
    }
}

#[derive(Debug)]
struct TextRange {
    cp_range: Range<u32>,
    compressed: bool,
    start_offset: u64,
}

#[allow(non_snake_case)]
#[derive(Debug)]
struct PlcPcd {
    aCP: Vec<u32>,
    aPcd: Vec<Pcd>,
}

impl PlcPcd {
    fn new(data: &[u8]) -> Result<Self, io::Error> {
        if data.len() < 4 || (data.len() - 4) % (4 + 8) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid PlcPcd size",
            ));
        }
        let n_of_pcds = (data.len() - 4) / (4 + 8);
        if n_of_pcds == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "No Pcd's found"));
        }
        let mut cps: Vec<u32> = Vec::with_capacity(n_of_pcds + 1);
        let mut prev_cp = 0i32;
        for i in 0..(n_of_pcds + 1) {
            let cp = i32::from_le_bytes(data[(i * 4)..(i * 4 + 4)].try_into().unwrap());
            if cp < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Found invalid CP ({}) at PlcPcd position {}", cp, i),
                ));
            }
            if i == 0 {
                if cp != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("PlcPcd starts with invalid CP ({}) rather than zero", cp),
                    ));
                }
            } else if cp <= prev_cp {
                // FIXME: should be contiguous?
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "PlcPcd contains non contiguous CP ({}) at position {}, expected {}",
                        cp,
                        i,
                        prev_cp + 1
                    ),
                ));
            }
            prev_cp = cp;
            cps.push(cp.try_into().unwrap());
        }
        let mut pcds: Vec<Pcd> = Vec::with_capacity(n_of_pcds);
        let pcd_start = (n_of_pcds + 1) * 4;
        for i in 0..(n_of_pcds) {
            pcds.push(Pcd::new(
                data[(pcd_start + i * 8)..(pcd_start + i * 8 + 8)]
                    .try_into()
                    .unwrap(),
            ));
        }
        Ok(Self {
            aCP: cps,
            aPcd: pcds,
        })
    }
}

impl<'a> IntoIterator for &'a PlcPcd {
    type Item = TextRange;
    type IntoIter = PlcPcdIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        PlcPcdIter {
            plcpcd: self,
            cur: 0,
        }
    }
}

struct PlcPcdIter<'a> {
    plcpcd: &'a PlcPcd,
    cur: usize,
}

impl Iterator for PlcPcdIter<'_> {
    type Item = TextRange;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur >= self.plcpcd.aPcd.len() {
            None
        } else {
            let ret = TextRange {
                cp_range: (self.plcpcd.aCP[self.cur]..self.plcpcd.aCP[self.cur + 1]),
                compressed: self.plcpcd.aPcd[self.cur].fc.is_compressed(),
                start_offset: self.plcpcd.aPcd[self.cur].fc.get_offset(),
            };
            self.cur += 1;
            Some(ret)
        }
    }
}

#[allow(non_snake_case)]
#[derive(Debug)]
struct Clx {
    _RgPrc: Vec<Vec<u8>>,
    Pcdt: PlcPcd,
}

impl Clx {
    fn new<R: Read + Seek>(reader: &mut R, len: u32) -> Result<Self, io::Error> {
        let mut avail: u32 = len;
        let mut prc: Vec<Vec<u8>> = Vec::new();
        // FIXME: check in winword if the following overflows are fatal
        loop {
            if avail < 3 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid RgPrc"));
            }
            let clxt = rdu8(reader)?;
            avail -= 1;
            if clxt == 2 {
                break;
            }
            if clxt != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid RgPrc type ({})", clxt),
                ));
            }
            let sz = rdi16le(reader)?;
            avail -= 2;
            if !(0..=0x3fa2).contains(&sz) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid PrcData len ({})", sz),
                ));
            }
            let sz = sz as u16;
            if u32::from(sz) > avail {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("PrcData overflow (size {}, available {})", sz, avail),
                ));
            }
            let mut buf = vec![0u8; usize::from(sz)];
            reader.read_exact(&mut buf)?;
            prc.push(buf);
            avail -= u32::from(sz);
        }
        if avail < 4 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Pcdt overflow"));
        }
        avail -= 4;
        let plc_pcd_len = rdu32le(reader)?;
        if avail < plc_pcd_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Pcdt overflow (size {}, available {})", plc_pcd_len, avail),
            ));
        }
        // FIXME check size too large
        let mut buf: Vec<u8> = vec![0u8; plc_pcd_len.try_into().unwrap()];
        reader.read_exact(&mut buf)?;
        let pcdt = PlcPcd::new(&buf)?;

        Ok(Self {
            _RgPrc: prc,
            Pcdt: pcdt,
        })
    }
}

#[allow(non_snake_case)]
#[derive(Debug)]
struct PlcBteChpx {
    aFC: Vec<u32>,
    aPnBteChpx: Vec<PnFkpChpx>,
}

impl PlcBteChpx {
    #[allow(non_snake_case)]
    fn new<R: Read + Seek>(reader: &mut R, len: u32) -> Result<Self, io::Error> {
        if len < 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid len argument",
            ));
        }
        let n = usize::try_from((len - 4) / 8)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut aFC = Vec::<u32>::new();
        let mut aPnBteChpx = Vec::<PnFkpChpx>::new();

        while aFC.len() < n + 1 {
            aFC.push(rdu32le(reader)?);
        }
        while aPnBteChpx.len() < n {
            aPnBteChpx.push(PnFkpChpx::new(reader)?);
        }
        Ok(Self { aFC, aPnBteChpx })
    }
}

#[derive(Debug)]
struct PnFkpChpx {
    pn: u32,
}

impl PnFkpChpx {
    fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, io::Error> {
        let misc = rdu32le(reader)?;
        let pn = misc & 0x3FFFFF;
        Ok(Self { pn })
    }
    fn offset(&self) -> u32 {
        self.pn * 512
    }
}

#[derive(Debug)]
struct ChpxFkp {
    rgfc: Vec<u32>,
    rgb: Vec<u8>,
    _crun: u8,
}

impl ChpxFkp {
    fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 512];
        reader.read_exact(&mut buf)?;
        let crun = buf[511];
        let mut reader = buf.as_slice();
        let mut rgfc = Vec::<u32>::new();
        while rgfc.len() < usize::from(crun) + 1 {
            rgfc.push(rdu32le(&mut reader)?);
        }
        let mut rgb = Vec::<u8>::new();
        while rgb.len() < usize::from(crun) {
            rgb.push(rdu8(&mut reader)?);
        }
        Ok(Self {
            rgfc,
            rgb,
            _crun: crun,
        })
    }
}

#[derive(Debug)]
struct Chpx {
    grpprl: Vec<Prl>,
}
impl Chpx {
    fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, io::Error> {
        let cb = rdu8(reader)?;
        let mut grpprl = Vec::<Prl>::new();
        let mut limited = SeekTake::new(reader, u64::from(cb));
        while limited.limit() != 0 {
            let prl = Prl::new(&mut limited)?;
            grpprl.push(prl);
        }
        Ok(Self { grpprl })
    }
}

#[derive(Debug)]
struct Prl {
    sprm: Sprm,
    operand: Operand,
}

impl Prl {
    fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, io::Error> {
        let sprm = Sprm::new(reader)?;
        let size: Option<usize> = match sprm.spra() {
            0 | 1 => Some(1),
            2 | 4 | 5 => Some(2),
            7 => Some(3),
            3 => Some(4),
            6 => None,
            _ => unreachable!(),
        };
        let data = match size {
            Some(size) => {
                let mut data = vec![0; size];
                reader.read_exact(&mut data)?;
                data
            }
            None => {
                let size = rdu8(reader)?;
                let mut data = vec![0; usize::from(size) + 1];
                data[0] = size;
                let slice = &mut data[1..];
                reader.read_exact(slice)?;
                data
            }
        };
        let operand = Operand(data);
        Ok(Self { sprm, operand })
    }
}

struct Sprm {
    sprm: u16,
}

impl Sprm {
    fn new<R: Read + Seek>(reader: &mut R) -> Result<Self, io::Error> {
        let sprm = rdu16le(reader)?;
        Ok(Self { sprm })
    }
    fn ispmd(&self) -> u16 {
        self.sprm & 0x01FF
    }
    fn f_spec(&self) -> bool {
        (self.sprm / 512) & 0x0001 != 0
    }
    fn sgc(&self) -> u8 {
        u8::try_from((self.sprm / 1024) & 0x0007).unwrap_or_default()
    }
    fn spra(&self) -> u8 {
        u8::try_from(self.sprm / 8192).unwrap_or_default()
    }
}

impl Debug for Sprm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sprm")
            .field("sprm", &self.sprm)
            .field("ispmd", &self.ispmd())
            .field("fSpec", &self.f_spec())
            .field("sgc", &self.sgc())
            .field("spra", &self.spra())
            .finish()
    }
}

#[derive(Debug)]
struct Operand(Vec<u8>);

impl Operand {
    fn to_u32(&self) -> Option<u32> {
        if self.0.len() == 4 {
            let mut buf = [0; 4];
            buf.copy_from_slice(&self.0);
            Some(u32::from_le_bytes(buf))
        } else {
            None
        }
    }
    fn to_bool(&self) -> Option<bool> {
        if self.0.len() == 1 {
            Some(self.0[0] != 0)
        } else {
            None
        }
    }
}

/// Specifies a part of the Word Document
pub enum DocPart {
    /// The *Main Document* part
    MainDocument,
    /// The *Footnotes* part
    Footnotes,
    /// The *Headers* part
    Headers,
    /// The *Comments* part
    Comments,
    /// The *Endnotes* part
    Endnotes,
    /// The *Textboxes* part
    Textboxes,
    /// The *Header Textboxes* part
    HeaderTextboxes,
}

/// The parser for *Word Binary File Format* (.doc)
///
/// # Examples
/// ```no_run
/// use ctxole::Ole;
/// use doc::*;
/// use std::fs::File;
/// use std::io::BufReader;
///
/// let f = File::open("MyDocument.doc").unwrap();
/// let ole = Ole::new(BufReader::new(f)).unwrap();
/// let mut doc = Doc::new(ole, &[]).unwrap();
/// let maindoc = &mut doc.char_iter(DocPart::MainDocument).unwrap().filter_map(|wc| {if let WordChar::Char(c) = wc {Some(c)} else {None}}).collect::<String>();
/// println!("{}", maindoc);
/// ```
///
/// # Errors
/// Most fuctions return a [`Result<T, std::io::Error>`]
/// * Errors from the IO layer are bubbled
/// * Errors generated in the parser are reported with [`ErrorKind`](std::io::ErrorKind)
///   set to [`InvalidData`](std::io::ErrorKind#variant.InvalidData)
///
pub struct Doc<R: Read + Seek> {
    ole: Ole<R>,
    pub fib: Fib,
    clx: Clx,
    plc_bte_chpx: PlcBteChpx,
    last_cp: u32,
    encryption_key: Option<crypto::LegacyKey>,
    tb_unencrypted_header_size: u64,
    encryption: Option<Encryption>,
}

impl<R: Read + Seek> Doc<R> {
    /// Parses a Word Binary File Format from an [ctxole::Ole] struct
    pub fn new(ole: Ole<R>, passwords: &[&str]) -> Result<Self, io::Error> {
        let mut wd = ole.get_stream_reader(&ole.get_entry_by_name("WordDocument")?);
        let mut encryption_key: Option<crypto::LegacyKey> = None;
        let fibbase = FibBase::new(&mut wd)?;
        let mut tb = ole.get_stream_reader(&ole.get_entry_by_name(fibbase.table_name())?);
        let mut tb_unencrypted_header_size = 0u64;
        let mut encryption = None;
        if fibbase.fEncrypted {
            if fibbase.fObfuscated {
                // Xor obfuscation
                for pass in passwords {
                    encryption_key = crypto::XorKey::method2(pass, fibbase.lKey);
                    if encryption_key.is_some() {
                        encryption = Some(Encryption {
                            algorithm: "XOR obfuscation".to_string(),
                            password: pass.to_string(),
                        });
                        debug!("Found correct XOR obfuscation password {pass}");
                        break;
                    }
                }
                if encryption_key.is_none() {
                    return Err(NoValidPasswordError::new_io_error("XOR obfuscation"));
                }
            } else {
                // Legacy encryption
                let crypto_version = crypto::Version::new(&mut tb)?;
                if crypto_version.minor == 1 && crypto_version.major == 1 {
                    // Office binary document RC4 encryption
                    // FIXME: the read size should be constrained by lKey
                    let encr = crypto::BinaryRc4Encryption::new(&mut tb)?;
                    for pass in passwords {
                        encryption_key = encr.get_key(pass, 512);
                        if encryption_key.is_some() {
                            debug!("Found correct Office Binary Document Rc4 password {pass}");
                            encryption = Some(Encryption {
                                algorithm: "RC4".to_string(),
                                password: pass.to_string(),
                            });
                            break;
                        }
                    }
                    if encryption_key.is_none() {
                        return Err(NoValidPasswordError::new_io_error(
                            "Office binary document RC4 encryption",
                        ));
                    }
                } else if crypto_version.minor == 2 && [2, 3, 4].contains(&crypto_version.major) {
                    // RC4 CryptoAPI Encryption
                    // FIXME: the read size should be constrained by lKey
                    let encr = crypto::Rc4CryptoApiEncryption::new(&mut tb)?;
                    for pass in passwords {
                        encryption_key = encr.get_key(pass, 512);
                        if encryption_key.is_some() {
                            debug!(
                                "Found correct Office Binary Document Rc4 CryptoApi password {pass}"
                            );
                            encryption = Some(Encryption {
                                algorithm: "RC4 CryptoApi".to_string(),
                                password: pass.to_string(),
                            });
                            break;
                        }
                    }
                    if encryption_key.is_none() {
                        return Err(NoValidPasswordError::new_io_error(
                            "Office binary document RC4 CryptoApi encryption",
                        ));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unsupported/Invalid EncryptionInfo version ({crypto_version})"),
                    ));
                }
                tb_unencrypted_header_size = u64::from(fibbase.lKey);
            }
        }

        let mut wd = if let Some(ref key) = encryption_key {
            crypto::LegacyDecryptor::new(wd, key, 68)
        } else {
            crypto::LegacyDecryptor::new_no_op(wd)
        };
        let fib = Fib::new(&mut wd, fibbase)?;
        let mut tb = if let Some(ref key) = encryption_key {
            crypto::LegacyDecryptor::new(tb, key, tb_unencrypted_header_size)
        } else {
            crypto::LegacyDecryptor::new_no_op(tb)
        };
        tb.seek(io::SeekFrom::Start(u64::from(fib.rgfclcb.fcClx)))?;

        // Important note:
        // The Clx is mandated by [MS-DOC] and is generally available even when !fComplex
        // However Word appears to fallback to the undocumented fcMin and fcMac values for legacy purposes
        //
        // This code rely on the Clx whenever it's present (even when fComplex == false)
        // A fallback is employed when the Clx is damaged/unreadable but only if fcMin and fcMac make sense
        // and only when !fComplex
        let clx = match Clx::new(&mut tb, fib.rgfclcb.lcbClx) {
            Ok(clx) => clx,
            Err(e) => {
                if !fib.fibbase.fComplex && fib.fibbase.fcMin < fib.fibbase.fcMac {
                    let wide = if fib.fibbase.fExtChar { 1u32 } else { 0 };
                    Clx {
                        _RgPrc: Vec::new(),
                        Pcdt: PlcPcd {
                            aCP: vec![0, fib.fibbase.fcMac - fib.fibbase.fcMin],
                            aPcd: vec![Pcd {
                                _fNoParaLast: false,
                                fc: FcCompressed {
                                    fc: (fib.fibbase.fcMin << wide) | (wide << 30),
                                },
                                _prm: 0,
                            }],
                        },
                    }
                } else {
                    return Err(e);
                }
            }
        };
        let last_cp = *clx.Pcdt.aCP.last().unwrap();

        tb.seek(io::SeekFrom::Start(u64::from(fib.rgfclcb.fcPlcfBteChpx)))?;
        let plc_bte_chpx = PlcBteChpx::new(&mut tb, fib.rgfclcb.lcbPlcfBteChpx)?;
        if checked_sum(&[
            fib.rglw.ccpText,
            fib.rglw.ccpFtn,
            fib.rglw.ccpHdd,
            fib.rglw.ccpAtn,
            fib.rglw.ccpEdn,
            fib.rglw.ccpTxbx,
            fib.rglw.ccpHdrTxbx,
        ])
        .map(|sum| sum > last_cp)
        .unwrap_or(true)
        {
            warn!("Doc: some document parts are outside of the stream");
        }
        Ok(Self {
            ole,
            fib,
            clx,
            plc_bte_chpx,
            last_cp,
            encryption_key,
            tb_unencrypted_header_size,
            encryption,
        })
    }

    fn get_wd_stream(&self) -> Result<crypto::LegacyDecryptor<OleStreamReader<R>>, io::Error> {
        let wd = self
            .ole
            .get_stream_reader(&self.ole.get_entry_by_name("WordDocument")?);
        let wd = if let Some(ref key) = self.encryption_key {
            crypto::LegacyDecryptor::new(wd, key, 68)
        } else {
            crypto::LegacyDecryptor::new_no_op(wd)
        };
        Ok(wd)
    }

    fn get_table_stream(&self) -> Result<crypto::LegacyDecryptor<OleStreamReader<R>>, io::Error> {
        let tb = self
            .ole
            .get_stream_reader(&self.ole.get_entry_by_name(self.fib.fibbase.table_name())?);
        let tb = if let Some(ref key) = self.encryption_key {
            crypto::LegacyDecryptor::new(tb, key, self.tb_unencrypted_header_size)
        } else {
            crypto::LegacyDecryptor::new_no_op(tb)
        };
        Ok(tb)
    }

    fn cp_range(&self, part: DocPart) -> Option<Range<u32>> {
        let (start_cp, range_len) = match part {
            DocPart::MainDocument => (Some(0), self.fib.rglw.ccpText),
            DocPart::Footnotes => (Some(self.fib.rglw.ccpText), self.fib.rglw.ccpFtn),
            DocPart::Headers => (
                checked_sum(&[self.fib.rglw.ccpText, self.fib.rglw.ccpFtn]),
                self.fib.rglw.ccpHdd,
            ),
            DocPart::Comments => (
                checked_sum(&[self.fib.rglw.ccpText + self.fib.rglw.ccpFtn + self.fib.rglw.ccpHdd]),
                self.fib.rglw.ccpAtn,
            ),
            DocPart::Endnotes => (
                checked_sum(&[
                    self.fib.rglw.ccpText,
                    self.fib.rglw.ccpFtn,
                    self.fib.rglw.ccpHdd,
                    self.fib.rglw.ccpAtn,
                ]),
                self.fib.rglw.ccpEdn,
            ),
            DocPart::Textboxes => (
                checked_sum(&[
                    self.fib.rglw.ccpText,
                    self.fib.rglw.ccpFtn,
                    self.fib.rglw.ccpHdd,
                    self.fib.rglw.ccpAtn,
                    self.fib.rglw.ccpEdn,
                ]),
                self.fib.rglw.ccpTxbx,
            ),
            DocPart::HeaderTextboxes => (
                checked_sum(&[
                    self.fib.rglw.ccpText,
                    self.fib.rglw.ccpFtn,
                    self.fib.rglw.ccpHdd,
                    self.fib.rglw.ccpAtn,
                    self.fib.rglw.ccpEdn,
                    self.fib.rglw.ccpTxbx,
                ]),
                self.fib.rglw.ccpHdrTxbx,
            ),
        };

        Some(start_cp?..start_cp.unwrap().checked_add(range_len)?)
    }

    fn chars(&mut self, start_cp: u32, end_cp: u32) -> Result<CharIterator<R>, io::Error> {
        if start_cp > end_cp || start_cp >= self.last_cp || end_cp > self.last_cp {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid character range requested",
            ))
        } else {
            CharIterator::new(self, start_cp, end_cp)
        }
    }

    /// Returns the document and compatibility settings
    pub fn get_dop(&self) -> Option<dop::Dop> {
        let mut tb = self.get_table_stream().ok()?;
        tb.seek(io::SeekFrom::Start(u64::from(self.fib.rgfclcb.fcDop)))
            .ok()?;
        dop::Dop::new(&mut tb, self.fib.nfib, self.fib.rgfclcb.lcbDop).ok()
    }

    /// Returns document associations (if present and parsable)
    pub fn get_associations(&self) -> Option<SttbfAssoc> {
        if self.fib.rgfclcb.lcbSttbfAssoc == 0 {
            return None;
        }
        let mut tb = self.get_table_stream().ok()?;
        tb.seek(io::SeekFrom::Start(u64::from(
            self.fib.rgfclcb.fcSttbfAssoc,
        )))
        .ok()?;
        SttbfAssoc::new(&mut tb, self.fib.rgfclcb.lcbSttbfAssoc).ok()
    }

    /// Returns a [char] iterator over the text of the specified [part](DocPart)
    pub fn char_iter(&mut self, part: DocPart) -> Result<CharIterator<R>, io::Error> {
        let cprange = self.cp_range(part).unwrap_or(0..0);
        self.chars(cprange.start, cprange.end)
    }

    /// Returns encryption information
    pub fn encryption(&self) -> Option<&Encryption> {
        self.encryption.as_ref()
    }
}

impl<'a, R: Read + Seek> VbaDocument<'a, R> for &'a Doc<R> {
    fn vba(self) -> Option<Result<Vba<'a, R>, io::Error>> {
        match Vba::new(&self.ole, "Macros") {
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            res => Some(res),
        }
    }
}

/// An iterator over the [char]s of the text in a specific document portion
pub struct CharIterator<'a, R: Read + Seek> {
    cur_cp: u32,
    end_cp: u32,
    cur_range: usize,
    ranges: Vec<TextRange>,
    wd: crypto::LegacyDecryptor<OleStreamReader<'a, R>>,
    wd2: crypto::LegacyDecryptor<OleStreamReader<'a, R>>,
    chpxfkp_cache: Option<ChpxFkpCache>,
    doc: &'a Doc<R>,
    last_char: Option<u32>,
}

impl<'a, R: Read + Seek> CharIterator<'a, R> {
    #[rustfmt::skip]
    const COMP_LUT: [char; 256] = [
        '\u{00}', '\u{01}', '\u{02}', '\u{03}', '\u{04}', '\u{05}', '\u{06}', '\u{07}',
        '\u{08}', '\u{09}', '\u{0a}', '\u{0b}', '\u{0c}', '\u{0d}', '\u{0e}', '\u{0f}',
        '\u{10}', '\u{11}', '\u{12}', '\u{13}', '\u{14}', '\u{15}', '\u{16}', '\u{17}',
        '\u{18}', '\u{19}', '\u{1a}', '\u{1b}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}',
        '\u{20}', '\u{21}', '\u{22}', '\u{23}', '\u{24}', '\u{25}', '\u{26}', '\u{27}',
        '\u{28}', '\u{29}', '\u{2a}', '\u{2b}', '\u{2c}', '\u{2d}', '\u{2e}', '\u{2f}',
        '\u{30}', '\u{31}', '\u{32}', '\u{33}', '\u{34}', '\u{35}', '\u{36}', '\u{37}',
        '\u{38}', '\u{39}', '\u{3a}', '\u{3b}', '\u{3c}', '\u{3d}', '\u{3e}', '\u{3f}',
        '\u{40}', '\u{41}', '\u{42}', '\u{43}', '\u{44}', '\u{45}', '\u{46}', '\u{47}',
        '\u{48}', '\u{49}', '\u{4a}', '\u{4b}', '\u{4c}', '\u{4d}', '\u{4e}', '\u{4f}',
        '\u{50}', '\u{51}', '\u{52}', '\u{53}', '\u{54}', '\u{55}', '\u{56}', '\u{57}',
        '\u{58}', '\u{59}', '\u{5a}', '\u{5b}', '\u{5c}', '\u{5d}', '\u{5e}', '\u{5f}',
        '\u{60}', '\u{61}', '\u{62}', '\u{63}', '\u{64}', '\u{65}', '\u{66}', '\u{67}',
        '\u{68}', '\u{69}', '\u{6a}', '\u{6b}', '\u{6c}', '\u{6d}', '\u{6e}', '\u{6f}',
        '\u{70}', '\u{71}', '\u{72}', '\u{73}', '\u{74}', '\u{75}', '\u{76}', '\u{77}',
        '\u{78}', '\u{79}', '\u{7a}', '\u{7b}', '\u{7c}', '\u{7d}', '\u{7e}', '\u{7f}',
        '\u{80}', '\u{81}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
        '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{8d}', '\u{8e}', '\u{8f}',
        '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
        '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9d}', '\u{9e}', '\u{0178}',
        '\u{a0}', '\u{a1}', '\u{a2}', '\u{a3}', '\u{a4}', '\u{a5}', '\u{a6}', '\u{a7}',
        '\u{a8}', '\u{a9}', '\u{aa}', '\u{ab}', '\u{ac}', '\u{ad}', '\u{ae}', '\u{af}',
        '\u{b0}', '\u{b1}', '\u{b2}', '\u{b3}', '\u{b4}', '\u{b5}', '\u{b6}', '\u{b7}',
        '\u{b8}', '\u{b9}', '\u{ba}', '\u{bb}', '\u{bc}', '\u{bd}', '\u{be}', '\u{bf}',
        '\u{c0}', '\u{c1}', '\u{c2}', '\u{c3}', '\u{c4}', '\u{c5}', '\u{c6}', '\u{c7}',
        '\u{c8}', '\u{c9}', '\u{ca}', '\u{cb}', '\u{cc}', '\u{cd}', '\u{ce}', '\u{cf}',
        '\u{d0}', '\u{d1}', '\u{d2}', '\u{d3}', '\u{d4}', '\u{d5}', '\u{d6}', '\u{d7}',
        '\u{d8}', '\u{d9}', '\u{da}', '\u{db}', '\u{dc}', '\u{dd}', '\u{de}', '\u{df}',
        '\u{e0}', '\u{e1}', '\u{e2}', '\u{e3}', '\u{e4}', '\u{e5}', '\u{e6}', '\u{e7}',
        '\u{e8}', '\u{e9}', '\u{ea}', '\u{eb}', '\u{ec}', '\u{ed}', '\u{ee}', '\u{ef}',
        '\u{f0}', '\u{f1}', '\u{f2}', '\u{f3}', '\u{f4}', '\u{f5}', '\u{f6}', '\u{f7}',
        '\u{f8}', '\u{f9}', '\u{fa}', '\u{fb}', '\u{fc}', '\u{fd}', '\u{fe}', '\u{ff}',
    ];

    fn new(doc: &'a Doc<R>, start_cp: u32, end_cp: u32) -> Result<Self, io::Error> {
        let mut ret = Self {
            cur_cp: start_cp,
            end_cp,
            cur_range: 0,
            ranges: doc
                .clx
                .Pcdt
                .into_iter()
                .filter(|range| range.cp_range.overlaps_with(&(start_cp..end_cp)))
                .collect(),
            wd: doc.get_wd_stream()?,
            wd2: doc.get_wd_stream()?,
            chpxfkp_cache: None,
            doc,
            last_char: None,
        };
        ret.seek_cur_cp()?;
        Ok(ret)
    }

    fn seek_cur_cp(&mut self) -> Result<bool, io::Error> {
        while self.cur_range < self.ranges.len() {
            let cur_range = &self.ranges[self.cur_range];
            if !cur_range.cp_range.contains(&self.cur_cp) {
                self.cur_range += 1;
                continue;
            }
            if cur_range.compressed {
                let off: u64 = (self.cur_cp - cur_range.cp_range.start).into();
                self.wd
                    .seek(io::SeekFrom::Start(cur_range.start_offset + off))?;
            } else {
                let off: u64 = ((self.cur_cp - cur_range.cp_range.start) * 2).into();
                self.wd
                    .seek(io::SeekFrom::Start(cur_range.start_offset + off))?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn get_fc_chpx(&mut self, fc: u32) -> Result<Option<&Chpx>, io::Error> {
        let plc_bte_chpx = &self.doc.plc_bte_chpx;
        let a_fc = &plc_bte_chpx.aFC;
        if plc_bte_chpx.aFC.is_empty() || *a_fc.first().unwrap() > fc || *a_fc.last().unwrap() <= fc
        {
            return Ok(None);
        }
        let mut index = 0;
        for (i, v) in a_fc.iter().enumerate() {
            if *v > fc {
                break;
            }
            index = i;
        }
        let offset = plc_bte_chpx.aPnBteChpx[index].offset();

        let cache = {
            if self.chpxfkp_cache.as_ref().map(|cache| cache.offset) == Some(offset) {
                self.chpxfkp_cache.as_ref().unwrap()
            } else {
                let reader = &mut self.wd2;
                reader.seek(io::SeekFrom::Start(u64::from(offset)))?;
                let chpxfkp = ChpxFkp::new(reader)?;
                let mut rgb = chpxfkp.rgb.clone();
                rgb.sort();
                let mut chcxcache = Vec::<ChpxCache>::new();
                let mut last_value = None;
                for rgb_value in rgb {
                    if Some(rgb_value) == last_value {
                        continue;
                    }
                    last_value = Some(rgb_value);
                    let offset = u64::from(offset) + u64::from(rgb_value) * 2;
                    reader.seek(io::SeekFrom::Start(offset))?;
                    let chpx = Chpx::new(reader)?;
                    chcxcache.push(ChpxCache { rgb_value, chpx });
                }

                self.chpxfkp_cache = Some(ChpxFkpCache {
                    chcxcache,
                    chpxfkp,
                    offset,
                });
                self.chpxfkp_cache.as_ref().unwrap()
            }
        };

        let chpxfkp = &cache.chpxfkp;
        if chpxfkp.rgfc.is_empty()
            || *chpxfkp.rgfc.first().unwrap() > fc
            || *chpxfkp.rgfc.last().unwrap() <= fc
        {
            return Ok(None);
        }
        index = 0;
        for (i, v) in chpxfkp.rgfc.iter().enumerate() {
            if *v > fc {
                break;
            }
            index = i;
        }

        let rgb_value = if let Some(value) = chpxfkp.rgb.get(index) {
            *value
        } else {
            return Ok(None);
        };

        let chpx = cache.chcxcache.iter().find_map(|chpx_cache| {
            if chpx_cache.rgb_value == rgb_value {
                Some(&chpx_cache.chpx)
            } else {
                None
            }
        });

        Ok(chpx)
    }
}

struct ChpxFkpCache {
    offset: u32,
    chpxfkp: ChpxFkp,
    chcxcache: Vec<ChpxCache>,
}

struct ChpxCache {
    rgb_value: u8,
    chpx: Chpx,
}

#[derive(Debug)]
pub enum DataLocation {
    PICFAndOfficeArtData(u32),
    NilPICFAndBinData(u32),
    ObjectPool(String),
    Unknown,
}

#[derive(Debug)]
pub enum WordChar {
    Char(char),
    Picture(DataLocation),
    ComplexField {
        key: String,
        value: String,
        extra_data: Vec<DataLocation>,
    },
    Hyperlink {
        text: String,
        uri: String,
        extra_data: Vec<DataLocation>,
    },
    Other(&'static str),
}

#[derive(PartialEq, Default)]
enum CurrentFieldPart {
    #[default]
    Key,
    Value,
}

#[derive(Default)]
struct Field {
    key: String,
    value: String,
    extra: Vec<DataLocation>,
    current_part: CurrentFieldPart,
}

impl<R: Read + Seek> Iterator for CharIterator<'_, R> {
    type Item = WordChar;

    fn next(&mut self) -> Option<Self::Item> {
        let mut field: Option<Field> = None;

        loop {
            let (c, fc) = self.next_char()?;
            let code = c as u32; //safe

            match code {
                // inline image
                0x01 => {
                    let chpx = match self.get_fc_chpx(fc).ok()? {
                        Some(chpx) => chpx,
                        None => continue,
                    };
                    let prl = chpx.grpprl.iter().find(|prl| prl.sprm.sprm == 0x6A03)?;
                    let data_location = prl.operand.to_u32()?;
                    let binary_data = chpx
                        .grpprl
                        .iter()
                        .find(|prl| prl.sprm.sprm == 0x0806)
                        .map(|prl| prl.operand.to_bool().unwrap_or_default())
                        .unwrap_or_default();

                    let picture_info = if binary_data {
                        DataLocation::NilPICFAndBinData(data_location)
                    } else {
                        DataLocation::PICFAndOfficeArtData(data_location)
                    };

                    if let Some(field) = &mut field {
                        field.extra.push(picture_info);
                        continue;
                    }

                    return Some(WordChar::Picture(picture_info));
                }
                // floating image
                0x08 => {
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    let picture_info = DataLocation::Unknown;
                    //TODO get picture information. See plcfSpa.
                    return Some(WordChar::Picture(picture_info));
                }
                0x13 => {
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    field = Some(Field::default());
                }
                0x14 => {
                    if let Some(field) = &mut field {
                        field.current_part = CurrentFieldPart::Value;
                        field.key = field.key.trim().to_string();

                        let chpx = self.get_fc_chpx(fc).ok()?;
                        if let Some(chpx) = chpx {
                            let Some(prl) = chpx.grpprl.iter().find(|prl| prl.sprm.sprm == 0x6A03)
                            else {
                                continue;
                            };
                            let id = prl.operand.to_u32()?;
                            let storage = format!("ObjectPool/_{id}");
                            field.extra.push(DataLocation::ObjectPool(storage));
                        }
                    }
                }
                0x15 => {
                    let f = match field {
                        Some(f) => f,
                        None => continue,
                    };

                    if let Some(s) = f.key.strip_prefix("HYPERLINK ") {
                        let s = s.trim_start();
                        if let Some(s) = s.strip_prefix('"') {
                            if let Some(index) = s.find('"') {
                                let s = &s[0..index];
                                let uri = s.to_string();
                                let text = f.value;
                                let extra_data = f.extra;
                                return Some(WordChar::Hyperlink {
                                    text,
                                    uri,
                                    extra_data,
                                });
                            }
                        }
                    }
                    let key = f.key;
                    let value = f.value;
                    let extra_data = f.extra;
                    return Some(WordChar::ComplexField {
                        key,
                        value,
                        extra_data,
                    });
                }
                0x02 => {
                    // U+0002 - An auto-numbered footnote reference. See plcffndRef.
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    return Some(WordChar::Other("[Auto-numbered footnote]"));
                }
                0x03 => {
                    // U+0003 - A short horizontal line.
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    return Some(WordChar::Other("[Short horizontal line]"));
                }
                0x04 => {
                    // U+0004 - A long horizontal line that is the width of the content area of the page.
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    return Some(WordChar::Other("[Long horizontal line]"));
                }
                0x05 => {
                    // U+0005 - An annotation reference character. See PlcfandRef.
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    return Some(WordChar::Other("[Annotation]"));
                }
                0x28 => {
                    // U+0028 - A symbol. See sprmCSymbol.
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    return Some(WordChar::Other("[Symbol]"));
                }
                0x3C => {
                    // U+003C - The start of a structured document tag bookmark range. See FibRgFcLcb2003.fcPlcfBkfSdt.
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    return Some(WordChar::Other("[Start Tag Bookmark]"));
                }
                0x3E => {
                    // U+003E - The end of a structured document tag bookmark range. See FibRgFcLcb2003.fcPlcfBklSdt.
                    if field.is_some() {
                        // ignore special character inside field
                        continue;
                    }
                    return Some(WordChar::Other("[End Tag Bookmark]"));
                }
                code => {
                    let c = match code {
                        0x2002 | 0x2003 => ' ',
                        0x0D => '\n',
                        0x07 => {
                            if self.last_char == Some(0x07) {
                                '\n'
                            } else {
                                '\t'
                            }
                        }
                        _ => c,
                    };

                    self.last_char = Some(code);

                    if let Some(field) = &mut field {
                        match field.current_part {
                            CurrentFieldPart::Key => {
                                field.key.push(c);
                            }
                            CurrentFieldPart::Value => {
                                field.value.push(c);
                            }
                        }
                    } else {
                        return Some(WordChar::Char(c));
                    }
                }
            }
        }
    }
}

impl<R: Read + Seek> CharIterator<'_, R> {
    fn next_char(&mut self) -> Option<(char, u32)> {
        if self.cur_cp >= self.end_cp {
            return None;
        }
        if !self.ranges[self.cur_range].cp_range.contains(&self.cur_cp)
            && !self.seek_cur_cp().ok()?
        {
            return None;
        }
        let fc = u32::try_from(self.wd.stream_position().ok()?).ok()?;
        let cur_range = &self.ranges[self.cur_range];
        if cur_range.compressed {
            self.cur_cp += 1;
            let c = Self::COMP_LUT[usize::from(rdu8(&mut self.wd).ok()?)];
            return Some((c, fc));
        }
        let v1 = rdu16le(&mut self.wd).ok()?;
        self.cur_cp += 1;
        if !(0xd800..0xdc00).contains(&v1) {
            let c = char::from_u32(v1.into()).unwrap_or(char::REPLACEMENT_CHARACTER);
            return Some((c, fc));
        }
        if self.cur_cp >= self.end_cp || self.cur_cp >= cur_range.cp_range.end {
            // Surrogate found at end of text or end of CP range
            return Some((char::REPLACEMENT_CHARACTER, fc));
        }
        let v2 = rdu16le(&mut self.wd).ok()?;
        if !(0xdc00..0xe000).contains(&v2) {
            // Bad low surrogate
            self.wd.seek(io::SeekFrom::Current(-2)).ok()?;
            return Some((char::REPLACEMENT_CHARACTER, fc));
        }
        self.cur_cp += 1;
        let c = char::from_u32(((u32::from(v1) - 0xd800) << 10) | (u32::from(v2) - 0xdc00))
            .unwrap_or(char::REPLACEMENT_CHARACTER);
        Some((c, fc))
    }
}
