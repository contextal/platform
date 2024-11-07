//! Word document property structures
//!
//! # Note #
//! All the structs therein as well as their fields are intended for inspection and forensic
//! analysis. They are exact maps of those defined and documented in \[MS-DOC\].
//!
//! For the reasons above:
//! - they retain their original names (and don't follow rust naming conventions)
//! - they are all `pub`
//! - they are not documented here
//! - their type is mapped into the closest available rust type

#![allow(non_snake_case, missing_docs)]

use ctxutils::io::*;
#[cfg(feature = "serde")]
use serde::{
    ser::{SerializeStruct, Serializer},
    Serialize,
};
use std::io::{self, Read, Seek};

#[derive(Debug)]
pub struct DTTM {
    pub mint: u8,
    pub hr: u8,
    pub dom: u8,
    pub mon: u8,
    pub yr: u16,
    pub wdy: u8,
}

impl TryFrom<&DTTM> for time::PrimitiveDateTime {
    type Error = time::Error;

    fn try_from(value: &DTTM) -> Result<Self, Self::Error> {
        if value.wdy > 6 {
            return Err(time::Error::ConversionRange(time::error::ConversionRange));
        }
        Ok(time::PrimitiveDateTime::new(
            time::Date::from_calendar_date(
                1900i32 + i32::from(value.yr),
                time::Month::try_from(value.mon)?,
                value.dom,
            )?,
            time::Time::from_hms(value.hr, value.mint, 0)?,
        ))
    }
}

impl From<u32> for DTTM {
    fn from(v: u32) -> Self {
        Self {
            mint: (v & 0b11_1111) as u8,            // 6 bits
            hr: ((v >> 6) & 0b1_1111) as u8,        // 5 bits
            dom: ((v >> 11) & 0b1_1111) as u8,      // 5 bits
            mon: ((v >> 16) & 0b1111) as u8,        // 4 bits
            yr: ((v >> 20) & 0b1_1111_1111) as u16, // 9 bits
            wdy: (v >> 29) as u8,                   // 3 bits
        }
    }
}

#[cfg(feature = "serde")]
impl Serialize for DTTM {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("DTTM", 7)?;
        state.serialize_field("mint", &self.mint)?;
        state.serialize_field("hr", &self.hr)?;
        state.serialize_field("dom", &self.dom)?;
        state.serialize_field("mon", &self.mon)?;
        state.serialize_field("yr", &self.yr)?;
        state.serialize_field("wdy", &self.wdy)?;
        let as_dt: Option<time::PrimitiveDateTime> = self.try_into().ok();
        state.serialize_field("as_dt", &as_dt)?;
        state.end()
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Copts60 {
    pub fNoTabForInd: bool,
    pub fNoSpaceRaiseLower: bool,
    pub fSuppressSpBfAfterPgBrk: bool,
    pub fWrapTrailSpaces: bool,
    pub fMapPrintTextColor: bool,
    pub fNoColumnBalance: bool,
    pub fConvMailMergeEsc: bool,
    pub fSuppressTopSpacing: bool,
    pub fOrigWordTableRules: bool,
    pub fShowBreaksInFrames: bool,
    pub fSwapBordersFacingPgs: bool,
    pub fLeaveBackslashAlone: bool,
    pub fExpShRtn: bool,
    pub fDntULTrlSpc: bool,
    pub fDntBlnSbDbWid: bool,
}

impl From<u16> for Copts60 {
    fn from(v: u16) -> Self {
        Self {
            fNoTabForInd: (v & (1 << 0)) != 0,
            fNoSpaceRaiseLower: (v & (1 << 1)) != 0,
            fSuppressSpBfAfterPgBrk: (v & (1 << 2)) != 0,
            fWrapTrailSpaces: (v & (1 << 3)) != 0,
            fMapPrintTextColor: (v & (1 << 4)) != 0,
            fNoColumnBalance: (v & (1 << 5)) != 0,
            fConvMailMergeEsc: (v & (1 << 6)) != 0,
            fSuppressTopSpacing: (v & (1 << 7)) != 0,
            fOrigWordTableRules: (v & (1 << 8)) != 0,
            // unused14
            fShowBreaksInFrames: (v & (1 << 10)) != 0,
            fSwapBordersFacingPgs: (v & (1 << 11)) != 0,
            fLeaveBackslashAlone: (v & (1 << 12)) != 0,
            fExpShRtn: (v & (1 << 13)) != 0,
            fDntULTrlSpc: (v & (1 << 14)) != 0,
            fDntBlnSbDbWid: (v & (1 << 15)) != 0,
        }
    }
}

#[allow(non_snake_case)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Copts80 {
    pub copts60: Copts60,
    pub fSuppressTopSpacingMac5: bool,
    pub fTruncDxaExpand: bool,
    pub fPrintBodyBeforeHdr: bool,
    pub fNoExtLeading: bool,
    pub fDontMakeSpaceForUL: bool,
    pub fMWSmallCaps: bool,
    pub f2ptExtLeadingOnly: bool,
    pub fTruncFontHeight: bool,
    pub fSubOnSize: bool,
    pub fLineWrapLikeWord6: bool,
    pub fWW6BorderRules: bool,
    pub fExactOnTop: bool,
    pub fExtraAfter: bool,
    pub fWPSpace: bool,
    pub fWPJust: bool,
    pub fPrintMet: bool,
}

impl From<u32> for Copts80 {
    fn from(v: u32) -> Self {
        Self {
            copts60: Copts60::from(v as u16),
            fSuppressTopSpacingMac5: (v & (1 << 16)) != 0,
            fTruncDxaExpand: (v & (1 << 17)) != 0,
            fPrintBodyBeforeHdr: (v & (1 << 18)) != 0,
            fNoExtLeading: (v & (1 << 19)) != 0,
            fDontMakeSpaceForUL: (v & (1 << 20)) != 0,
            fMWSmallCaps: (v & (1 << 21)) != 0,
            f2ptExtLeadingOnly: (v & (1 << 22)) != 0,
            fTruncFontHeight: (v & (1 << 23)) != 0,
            fSubOnSize: (v & (1 << 24)) != 0,
            fLineWrapLikeWord6: (v & (1 << 25)) != 0,
            fWW6BorderRules: (v & (1 << 26)) != 0,
            fExactOnTop: (v & (1 << 27)) != 0,
            fExtraAfter: (v & (1 << 28)) != 0,
            fWPSpace: (v & (1 << 29)) != 0,
            fWPJust: (v & (1 << 30)) != 0,
            fPrintMet: (v & (1 << 31)) != 0,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Copts {
    pub copts80: Copts80,
    pub fSpLayoutLikeWW8: bool,
    pub fFtnLayoutLikeWW8: bool,
    pub fDontUseHTMLParagraphAutoSpacing: bool,
    pub fDontAdjustLineHeightInTable: bool,
    pub fForgetLastTabAlign: bool,
    pub fUseAutospaceForFullWidthAlpha: bool,
    pub fAlignTablesRowByRow: bool,
    pub fLayoutRawTableWidth: bool,
    pub fLayoutTableRowsApart: bool,
    pub fUseWord97LineBreakingRules: bool,
    pub fDontBreakWrappedTables: bool,
    pub fDontSnapToGridInCell: bool,
    pub fDontAllowFieldEndSelect: bool,
    pub fApplyBreakingRules: bool,
    pub fDontWrapTextWithPunct: bool,
    pub fDontUseAsianBreakRules: bool,
    pub fUseWord2002TableStyleRules: bool,
    pub fGrowAutoFit: bool,
    pub fUseNormalStyleForList: bool,
    pub fDontUseIndentAsNumberingTabStop: bool,
    pub fFELineBreak11: bool,
    pub fAllowSpaceOfSameStyleInTable: bool,
    pub fWW11IndentRules: bool,
    pub fDontAutofitConstrainedTables: bool,
    pub fAutofitLikeWW11: bool,
    pub fUnderlineTabInNumList: bool,
    pub fHangulWidthLikeWW11: bool,
    pub fSplitPgBreakAndParaMark: bool,
    pub fDontVertAlignCellWithSp: bool,
    pub fDontBreakConstrainedForcedTables: bool,
    pub fDontVertAlignInTxbx: bool,
    pub fWord11KerningPairs: bool,
    pub fCachedColBalance: bool,
}

impl Copts {
    fn from_le_bytes(v: [u8; 32]) -> Self {
        Self {
            copts80: Copts80::from(u32::from_le_bytes(v[0..4].try_into().unwrap())),
            fSpLayoutLikeWW8: (v[4] & 0b1) != 0,
            fFtnLayoutLikeWW8: (v[4] & 0b10) != 0,
            fDontUseHTMLParagraphAutoSpacing: (v[4] & 0b100) != 0,
            fDontAdjustLineHeightInTable: (v[4] & 0b1000) != 0,
            fForgetLastTabAlign: (v[4] & 0b1_0000) != 0,
            fUseAutospaceForFullWidthAlpha: (v[4] & 0b10_0000) != 0,
            fAlignTablesRowByRow: (v[4] & 0b100_0000) != 0,
            fLayoutRawTableWidth: (v[4] & 0b1000_0000) != 0,
            fLayoutTableRowsApart: (v[5] & 0b1) != 0,
            fUseWord97LineBreakingRules: (v[5] & 0b10) != 0,
            fDontBreakWrappedTables: (v[5] & 0b100) != 0,
            fDontSnapToGridInCell: (v[5] & 0b1000) != 0,
            fDontAllowFieldEndSelect: (v[5] & 0b1_0000) != 0,
            fApplyBreakingRules: (v[5] & 0b10_0000) != 0,
            fDontWrapTextWithPunct: (v[5] & 0b100_0000) != 0,
            fDontUseAsianBreakRules: (v[5] & 0b1000_0000) != 0,
            fUseWord2002TableStyleRules: (v[6] & 0b1) != 0,
            fGrowAutoFit: (v[6] & 0b10) != 0,
            fUseNormalStyleForList: (v[6] & 0b100) != 0,
            fDontUseIndentAsNumberingTabStop: (v[6] & 0b1000) != 0,
            fFELineBreak11: (v[6] & 0b1_0000) != 0,
            fAllowSpaceOfSameStyleInTable: (v[6] & 0b10_0000) != 0,
            fWW11IndentRules: (v[6] & 0b100_0000) != 0,
            fDontAutofitConstrainedTables: (v[6] & 0b1000_0000) != 0,
            fAutofitLikeWW11: (v[7] & 0b1) != 0,
            fUnderlineTabInNumList: (v[7] & 0b10) != 0,
            fHangulWidthLikeWW11: (v[7] & 0b100) != 0,
            fSplitPgBreakAndParaMark: (v[7] & 0b1000) != 0,
            fDontVertAlignCellWithSp: (v[7] & 0b1_0000) != 0,
            fDontBreakConstrainedForcedTables: (v[7] & 0b10_0000) != 0,
            fDontVertAlignInTxbx: (v[7] & 0b100_0000) != 0,
            fWord11KerningPairs: (v[7] & 0b1000_0000) != 0,
            fCachedColBalance: (v[8] & 0b1) != 0,
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct DopBase {
    pub fFacingPages: bool,
    pub fPMHMainDoc: bool,
    pub fpc: u8,
    pub rncFtn: u8,
    pub nFtn: u16,
    pub fSplAllDone: bool,
    pub fSplAllClean: bool,
    pub fSplHideErrors: bool,
    pub fGramHideErrors: bool,
    pub fLabelDoc: bool,
    pub fHyphCapitals: bool,
    pub fAutoHyphen: bool,
    pub fFormNoFields: bool,
    pub fLinkStyles: bool,
    pub fRevMarking: bool,
    pub fExactCWords: bool,
    pub fPagHidden: bool,
    pub fPagResults: bool,
    pub fLockAtn: bool,
    pub fMirrorMargins: bool,
    pub fWord97Compat: bool,
    pub fProtEnabled: bool,
    pub fDispFormFldSel: bool,
    pub fRMView: bool,
    pub fRMPrint: bool,
    pub fLockVbaProj: bool,
    pub fLockRev: bool,
    pub fEmbedFonts: bool,
    pub copts60: Copts60,
    pub dxaTab: u16,
    pub cpgWebOpt: u16,
    pub dxaHotZ: u16,
    pub cConsecHypLim: u16,
    pub dttmCreated: DTTM,
    pub dttmRevised: DTTM,
    pub dttmLastPrint: DTTM,
    pub nRevision: u16,
    pub tmEdited: u32,
    pub cWords: u32,
    pub cCh: u32,
    pub cPg: u16,
    pub cParas: u32,
    pub rncEdn: u8,
    pub nEdn: u16,
    pub epc: u8,
    pub fPrintFormData: bool,
    pub fSaveFormData: bool,
    pub fShadeFormData: bool,
    pub fShadeMergeFields: bool,
    pub fIncludeSubdocsInStats: bool,
    pub cLines: u32,
    pub cWordsWithSubdocs: u32,
    pub cChWithSubdocs: u32,
    pub cPgWithSubdocs: u16,
    pub cParasWithSubdocs: u32,
    pub cLinesWithSubdocs: u32,
    pub lKeyProtDoc: u32,
    pub wvkoSaved: u8,
    pub pctWwdSaved: u16,
    pub zkSaved: u8,
    pub iGutterPos: bool,
}

impl DopBase {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 84];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            fFacingPages: (buf[0] & 0b1) != 0,
            fPMHMainDoc: (buf[0] & 0b100) != 0,
            fpc: (buf[0] >> 5) & 0b11,
            rncFtn: buf[2] & 0b11,
            nFtn: u16::from_le_bytes(buf[2..4].try_into().unwrap()) >> 2,
            fSplAllDone: (buf[4] & 0b100_0000) != 0,
            fSplAllClean: (buf[4] & 0b1000_0000) != 0,
            fSplHideErrors: (buf[5] & 0b1) != 0,
            fGramHideErrors: (buf[5] & 0b10) != 0,
            fLabelDoc: (buf[5] & 0b100) != 0,
            fHyphCapitals: (buf[5] & 0b1000) != 0,
            fAutoHyphen: (buf[5] & 0b1_0000) != 0,
            fFormNoFields: (buf[5] & 0b10_0000) != 0,
            fLinkStyles: (buf[5] & 0b100_0000) != 0,
            fRevMarking: (buf[5] & 0b1000_0000) != 0,
            fExactCWords: (buf[6] & 0b10) != 0,
            fPagHidden: (buf[6] & 0b100) != 0,
            fPagResults: (buf[6] & 0b1000) != 0,
            fLockAtn: (buf[6] & 0b1_0000) != 0,
            fMirrorMargins: (buf[6] & 0b10_0000) != 0,
            fWord97Compat: (buf[6] & 0b100_0000) != 0,
            fProtEnabled: (buf[7] & 0b10) != 0,
            fDispFormFldSel: (buf[7] & 0b100) != 0,
            fRMView: (buf[7] & 0b1000) != 0,
            fRMPrint: (buf[7] & 0b1_0000) != 0,
            fLockVbaProj: (buf[7] & 0b10_0000) != 0,
            fLockRev: (buf[7] & 0b100_0000) != 0,
            fEmbedFonts: (buf[7] & 0b1000_0000) != 0,
            copts60: Copts60::from(u16::from_le_bytes(buf[8..10].try_into().unwrap())),
            dxaTab: u16::from_le_bytes(buf[10..12].try_into().unwrap()),
            cpgWebOpt: u16::from_le_bytes(buf[12..14].try_into().unwrap()),
            dxaHotZ: u16::from_le_bytes(buf[14..16].try_into().unwrap()),
            cConsecHypLim: u16::from_le_bytes(buf[16..18].try_into().unwrap()),
            // wSpare2 : u16
            dttmCreated: DTTM::from(u32::from_le_bytes(buf[20..24].try_into().unwrap())),
            dttmRevised: DTTM::from(u32::from_le_bytes(buf[24..28].try_into().unwrap())),
            dttmLastPrint: DTTM::from(u32::from_le_bytes(buf[28..32].try_into().unwrap())),
            nRevision: u16::from_le_bytes(buf[32..34].try_into().unwrap()),
            tmEdited: u32::from_le_bytes(buf[34..38].try_into().unwrap()),
            cWords: u32::from_le_bytes(buf[38..42].try_into().unwrap()),
            cCh: u32::from_le_bytes(buf[42..46].try_into().unwrap()),
            cPg: u16::from_le_bytes(buf[46..48].try_into().unwrap()),
            cParas: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
            rncEdn: buf[52] & 0b11,
            nEdn: u16::from_le_bytes(buf[52..54].try_into().unwrap()) >> 2,
            epc: buf[54] & 0b11,
            fPrintFormData: (buf[55] & 0b100) != 0,
            fSaveFormData: (buf[55] & 0b1000) != 0,
            fShadeFormData: (buf[55] & 0b1_0000) != 0,
            fShadeMergeFields: (buf[55] & 0b10_0000) != 0,
            fIncludeSubdocsInStats: (buf[55] & 0b1000_0000) != 0,
            cLines: u32::from_le_bytes(buf[56..60].try_into().unwrap()),
            cWordsWithSubdocs: u32::from_le_bytes(buf[60..64].try_into().unwrap()),
            cChWithSubdocs: u32::from_le_bytes(buf[64..68].try_into().unwrap()),
            cPgWithSubdocs: u16::from_le_bytes(buf[68..70].try_into().unwrap()),
            cParasWithSubdocs: u32::from_le_bytes(buf[70..74].try_into().unwrap()),
            cLinesWithSubdocs: u32::from_le_bytes(buf[74..78].try_into().unwrap()),
            lKeyProtDoc: u32::from_le_bytes(buf[78..82].try_into().unwrap()),
            wvkoSaved: buf[82] & 0b111,
            pctWwdSaved: (u16::from_le_bytes(buf[68..70].try_into().unwrap()) >> 3) & 0b1_1111_1111,
            zkSaved: (buf[83] >> 4) & 0b11,
            iGutterPos: (buf[83] & 0b1000) != 0,
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop95 {
    pub copts80: Copts80,
}

impl Dop95 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let c80 = rdu32le(reader)?;
        Ok(Self {
            copts80: Copts80::from(c80),
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop97 {
    pub adt: u16,
    pub lvlDop: u8,
    pub fGramAllDone: bool,
    pub fGramAllClean: bool,
    pub fSubsetFonts: bool,
    pub fHtmlDoc: bool,
    pub fDiskLvcInvalid: bool,
    pub fSnapBorder: bool,
    pub fIncludeHeader: bool,
    pub fIncludeFooter: bool,
    pub cChWS: u32,
    pub cChWSWithSubdocs: u32,
    pub grfDocEvents: u32,
    pub fVirusPrompted: bool,
    pub fVirusLoadSafe: bool,
    pub KeyVirusSession30: u32,
    pub cpMaxListCacheMainDoc: u32,
    pub ilfoLastBulletMain: u16,
    pub ilfoLastNumberMain: u16,
    pub cDBC: u32,
    pub cDBCWithSubdocs: u32,
    pub nfcFtnRef: u16,
    pub nfcEdnRef: u16,
    pub hpsZoomFontPag: u16,
    pub dywDispPag: u16,
}

impl Dop97 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 412];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            adt: u16::from_le_bytes(buf[0..2].try_into().unwrap()),
            // doptypography 2..312 bytes
            // dogrid 312..322
            // unused1 1 bit
            lvlDop: (buf[322] >> 1) & 0b1111,
            fGramAllDone: (buf[322] & 0b10_0000) != 0,
            fGramAllClean: (buf[322] & 0b100_0000) != 0,
            fSubsetFonts: (buf[322] & 0b1000_0000) != 0,
            // unused2 1 bit
            fHtmlDoc: (buf[323] & 0b10) != 0,
            fDiskLvcInvalid: (buf[323] & 0b100) != 0,
            fSnapBorder: (buf[323] & 0b1000) != 0,
            fIncludeHeader: (buf[323] & 0b1_0000) != 0,
            fIncludeFooter: (buf[323] & 0b10_0000) != 0,
            // unused3 1 bit
            // unused4 1 bit
            // unused5 324..326
            // asumyi 326..338
            cChWS: u32::from_le_bytes(buf[338..342].try_into().unwrap()),
            cChWSWithSubdocs: u32::from_le_bytes(buf[342..346].try_into().unwrap()),
            grfDocEvents: u32::from_le_bytes(buf[346..350].try_into().unwrap()),
            fVirusPrompted: (buf[350] & 0b1) != 0,
            fVirusLoadSafe: (buf[350] & 0b10) != 0,
            KeyVirusSession30: u32::from_le_bytes(buf[350..354].try_into().unwrap()) >> 2,
            // space 354..384
            cpMaxListCacheMainDoc: u32::from_le_bytes(buf[384..388].try_into().unwrap()),
            ilfoLastBulletMain: u16::from_le_bytes(buf[388..390].try_into().unwrap()),
            ilfoLastNumberMain: u16::from_le_bytes(buf[390..392].try_into().unwrap()),
            cDBC: u32::from_le_bytes(buf[392..396].try_into().unwrap()),
            cDBCWithSubdocs: u32::from_le_bytes(buf[396..400].try_into().unwrap()),
            // reserved3a 400..404
            nfcFtnRef: u16::from_le_bytes(buf[404..406].try_into().unwrap()),
            nfcEdnRef: u16::from_le_bytes(buf[406..408].try_into().unwrap()),
            hpsZoomFontPag: u16::from_le_bytes(buf[408..410].try_into().unwrap()),
            dywDispPag: u16::from_le_bytes(buf[410..412].try_into().unwrap()),
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop2000 {
    pub ilvlLastBulletMain: u8,
    pub ilvlLastNumberMain: u8,
    pub istdClickParaType: u16,
    pub fLADAllDone: bool,
    pub fEnvelopeVis: bool,
    pub fMaybeTentativeListInDoc: bool,
    pub fMaybeFitText: bool,
    pub fFCCAllDone: bool,
    pub fRelyOnCSS_WebOpt: bool,
    pub fRelyOnVML_WebOpt: bool,
    pub fAllowPNG_WebOpt: bool,
    pub screenSize_WebOpt: u8,
    pub fOrganizeInFolder_WebOpt: bool,
    pub fUseLongFileNames_WebOpt: bool,
    pub iPixelsPerInch_WebOpt: u16,
    pub fWebOptionsInit: bool,
    pub fMaybeFEL: bool,
    pub fCharLineUnits: bool,
    pub copts: Copts,
    pub verCompatPre10: u16,
    pub fNoMargPgvwSaved: bool,
    pub fBulletProofed: bool,
    pub fSaveUim: bool,
    pub fFilterPrivacy: bool,
    pub fSeenRepairs: bool,
    pub fHasXML: bool,
    pub fValidateXML: bool,
    pub fSaveInvalidXML: bool,
    pub fShowXMLErrors: bool,
    pub fAlwaysMergeEmptyNamespace: bool,
}

impl Dop2000 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 44];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            ilvlLastBulletMain: buf[0],
            ilvlLastNumberMain: buf[1],
            istdClickParaType: u16::from_le_bytes(buf[2..4].try_into().unwrap()),
            fLADAllDone: (buf[4] & 0b1) != 0,
            fEnvelopeVis: (buf[4] & 0b10) != 0,
            fMaybeTentativeListInDoc: (buf[4] & 0b100) != 0,
            fMaybeFitText: (buf[4] & 0b1000) != 0,
            // empty1 4 bits
            fFCCAllDone: (buf[5] & 0b1) != 0,
            fRelyOnCSS_WebOpt: (buf[5] & 0b10) != 0,
            fRelyOnVML_WebOpt: (buf[5] & 0b100) != 0,
            fAllowPNG_WebOpt: (buf[5] & 0b1000) != 0,
            screenSize_WebOpt: buf[5] >> 4,
            fOrganizeInFolder_WebOpt: (buf[6] & 0b1) != 0,
            fUseLongFileNames_WebOpt: (buf[6] & 0b10) != 0,
            iPixelsPerInch_WebOpt: (u16::from_le_bytes(buf[6..8].try_into().unwrap()) >> 2)
                & 0b11_1111_1111,
            fWebOptionsInit: (buf[7] & 0b1_0000) != 0,
            fMaybeFEL: (buf[7] & 0b10_0000) != 0,
            fCharLineUnits: (buf[7] & 0b100_0000) != 0,
            // unused1 1 bit
            copts: Copts::from_le_bytes(buf[8..40].try_into().unwrap()),
            verCompatPre10: u16::from_le_bytes(buf[40..42].try_into().unwrap()),
            fNoMargPgvwSaved: (buf[42] & 0b1) != 0,
            // unused2, unused3, unused4 - 3 bits
            fBulletProofed: (buf[42] & 0b1_0000) != 0,
            // empty2 1 bit
            fSaveUim: (buf[42] & 0b100_0000) != 0,
            fFilterPrivacy: (buf[42] & 0b1000_0000) != 0,
            // empty3 1 bit
            fSeenRepairs: (buf[43] & 0b10) != 0,
            fHasXML: (buf[43] & 0b100) != 0,
            // unused5 1 bit
            fValidateXML: (buf[43] & 0b1_0000) != 0,
            fSaveInvalidXML: (buf[43] & 0b10_0000) != 0,
            fShowXMLErrors: (buf[43] & 0b100_0000) != 0,
            fAlwaysMergeEmptyNamespace: (buf[43] & 0b1000_0000) != 0,
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop2002 {
    pub fDoNotEmbedSystemFont: bool,
    pub fWordCompat: bool,
    pub fLiveRecover: bool,
    pub fEmbedFactoids: bool,
    pub fFactoidXML: bool,
    pub fFactoidAllDone: bool,
    pub fFolioPrint: bool,
    pub fReverseFolio: bool,
    pub iTextLineEnding: u8,
    pub fHideFcc: bool,
    pub fAcetateShowMarkup: bool,
    pub fAcetateShowAtn: bool,
    pub fAcetateShowInsDel: bool,
    pub fAcetateShowProps: bool,
    pub istdTableDflt: u16,
    pub verCompat: u16,
    pub grfFmtFilter: u16,
    pub iFolioPages: u16,
    pub cpgText: u32,
    pub cpMinRMText: u32,
    pub cpMinRMFtn: u32,
    pub cpMinRMHdd: u32,
    pub cpMinRMAtn: u32,
    pub cpMinRMEdn: u32,
    pub cpMinRmTxbx: u32,
    pub cpMinRmHdrTxbx: u32,
    pub rsidRoot: u32,
}

impl Dop2002 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 50];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            // unused 4 bytes
            fDoNotEmbedSystemFont: (buf[4] & 0b1) != 0,
            fWordCompat: (buf[4] & 0b10) != 0,
            fLiveRecover: (buf[4] & 0b100) != 0,
            fEmbedFactoids: (buf[4] & 0b1000) != 0,
            fFactoidXML: (buf[4] & 0b1_0000) != 0,
            fFactoidAllDone: (buf[4] & 0b10_0000) != 0,
            fFolioPrint: (buf[4] & 0b100_0000) != 0,
            fReverseFolio: (buf[4] & 0b1000_0000) != 0,
            iTextLineEnding: buf[5] & 0b111,
            fHideFcc: (buf[5] & 0b1000) != 0,
            fAcetateShowMarkup: (buf[5] & 0b1_0000) != 0,
            fAcetateShowAtn: (buf[5] & 0b10_0000) != 0,
            fAcetateShowInsDel: (buf[5] & 0b100_0000) != 0,
            fAcetateShowProps: (buf[5] & 0b1000_0000) != 0,
            istdTableDflt: u16::from_le_bytes(buf[6..8].try_into().unwrap()),
            verCompat: u16::from_le_bytes(buf[8..10].try_into().unwrap()),
            grfFmtFilter: u16::from_le_bytes(buf[10..12].try_into().unwrap()),
            iFolioPages: u16::from_le_bytes(buf[12..14].try_into().unwrap()),
            cpgText: u32::from_le_bytes(buf[14..18].try_into().unwrap()),
            cpMinRMText: u32::from_le_bytes(buf[18..22].try_into().unwrap()),
            cpMinRMFtn: u32::from_le_bytes(buf[22..26].try_into().unwrap()),
            cpMinRMHdd: u32::from_le_bytes(buf[26..30].try_into().unwrap()),
            cpMinRMAtn: u32::from_le_bytes(buf[30..34].try_into().unwrap()),
            cpMinRMEdn: u32::from_le_bytes(buf[34..38].try_into().unwrap()),
            cpMinRmTxbx: u32::from_le_bytes(buf[38..42].try_into().unwrap()),
            cpMinRmHdrTxbx: u32::from_le_bytes(buf[42..46].try_into().unwrap()),
            rsidRoot: u32::from_le_bytes(buf[46..50].try_into().unwrap()),
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop2003 {
    pub fTreatLockAtnAsReadOnly: bool,
    pub fStyleLock: bool,
    pub fAutoFmtOverride: bool,
    pub fRemoveWordML: bool,
    pub fApplyCustomXForm: bool,
    pub fStyleLockEnforced: bool,
    pub fFakeLockAtn: bool,
    pub fIgnoreMixedContent: bool,
    pub fShowPlaceholderText: bool,
    pub fWord97Doc: bool,
    pub fStyleLockTheme: bool,
    pub fStyleLockQFSet: bool,
    pub fReadingModeInkLockDown: bool,
    pub fAcetateShowInkAtn: bool,
    pub fFilterDttm: bool,
    pub fEnforceDocProt: bool,
    pub iDocProtCur: u8,
    pub fDispBkSpSaved: bool,
    pub dxaPageLock: u32,
    pub dyaPageLock: u32,
    pub pctFontLock: u32,
    pub grfitbid: u8,
    pub ilfoMacAtCleanup: u16,
}

impl Dop2003 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 22];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            fTreatLockAtnAsReadOnly: (buf[0] & 0b1) != 0,
            fStyleLock: (buf[0] & 0b10) != 0,
            fAutoFmtOverride: (buf[0] & 0b100) != 0,
            fRemoveWordML: (buf[0] & 0b1000) != 0,
            fApplyCustomXForm: (buf[0] & 0b1_0000) != 0,
            fStyleLockEnforced: (buf[0] & 0b10_0000) != 0,
            fFakeLockAtn: (buf[0] & 0b100_0000) != 0,
            fIgnoreMixedContent: (buf[0] & 0b1000_0000) != 0,
            fShowPlaceholderText: (buf[1] & 0b1) != 0,
            // unused 1 bit
            fWord97Doc: (buf[1] & 0b100) != 0,
            fStyleLockTheme: (buf[1] & 0b1000) != 0,
            fStyleLockQFSet: (buf[1] & 0b1_0000) != 0,
            // empty1 - 19 bits
            fReadingModeInkLockDown: (buf[4] & 0b1) != 0,
            fAcetateShowInkAtn: (buf[4] & 0b10) != 0,
            fFilterDttm: (buf[4] & 0b100) != 0,
            fEnforceDocProt: (buf[4] & 0b1000) != 0,
            iDocProtCur: (buf[4] >> 4) & 0b111,
            fDispBkSpSaved: (buf[4] & 0b1000_0000) != 0,
            // empty2 - 8 bits
            dxaPageLock: u32::from_le_bytes(buf[6..10].try_into().unwrap()),
            dyaPageLock: u32::from_le_bytes(buf[10..14].try_into().unwrap()),
            pctFontLock: u32::from_le_bytes(buf[14..18].try_into().unwrap()),
            grfitbid: buf[18],
            // empy3 - 1 byte
            ilfoMacAtCleanup: u16::from_le_bytes(buf[20..22].try_into().unwrap()),
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop2007 {
    pub fRMTrackFormatting: bool,
    pub fRMTrackMoves: bool,
    pub ssm: u8,
    pub fReadingModeInkLockDownActualPage: bool,
    pub fAutoCompressPictures: bool,
}

impl Dop2007 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 58];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            // reserved1 4 bytes
            fRMTrackFormatting: (buf[4] & 0b1) != 0,
            fRMTrackMoves: (buf[4] & 0b10) != 0,
            // reserved2, empty1, empty2 3 bits
            ssm: (buf[4] >> 5) | ((buf[5] & 0b1) << 3),
            fReadingModeInkLockDownActualPage: (buf[5] & 0b10) != 0,
            fAutoCompressPictures: (buf[5] & 0b100) != 0,
            // reserved3, empty[3-6]
            // dopMth
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop2010 {
    pub docid: u32,
    pub fDiscardImageData: bool,
    pub iImageDPI: u32,
}

impl Dop2010 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 16];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            docid: u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            // reserved 4 bytes
            fDiscardImageData: (buf[8] & 0b1) != 0,
            // empty 31 bits
            iImageDPI: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop2013 {
    pub fChartTrackingRefBased: bool,
}

impl Dop2013 {
    fn new<R: Read>(reader: &mut R) -> Result<Self, io::Error> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(Self {
            fChartTrackingRefBased: (buf[0] & 0b1) != 0,
            // empty 31 bits
        })
    }
}

#[cfg_attr(feature = "serde", derive(Serialize))]
#[derive(Debug)]
pub struct Dop {
    pub dopbase: DopBase,
    pub dop95: Option<Dop95>,
    pub dop97: Option<Dop97>,
    pub dop2000: Option<Dop2000>,
    pub dop2002: Option<Dop2002>,
    pub dop2003: Option<Dop2003>,
    pub dop2007: Option<Dop2007>,
    pub dop2010: Option<Dop2010>,
    pub dop2013: Option<Dop2013>,
    /// Indicates whether the Dop is congruent with nFib
    pub conformant: bool,
}

impl Dop {
    pub(super) fn new<R: Read + Seek>(
        reader: &mut R,
        n_fib: u16,
        lcb_dop: u32,
    ) -> Result<Self, io::Error> {
        let mut r = SeekTake::new(reader, lcb_dop.into());
        let dopbase = DopBase::new(&mut r)?;
        let dop95 = Dop95::new(&mut r).ok();
        let dop97 = dop95.as_ref().and_then(|_| Dop97::new(&mut r).ok());
        let dop2000 = dop97.as_ref().and_then(|_| Dop2000::new(&mut r).ok());
        let dop2002 = dop2000.as_ref().and_then(|_| Dop2002::new(&mut r).ok());
        let dop2003 = dop2002.as_ref().and_then(|_| Dop2003::new(&mut r).ok());
        let dop2007 = dop2003.as_ref().and_then(|_| Dop2007::new(&mut r).ok());
        let dop2010 = dop2007.as_ref().and_then(|_| Dop2010::new(&mut r).ok());
        let dop2013 = dop2010.as_ref().and_then(|_| Dop2013::new(&mut r).ok());
        let conformant = match n_fib {
            0 => dop97.is_some() && dop2000.is_none(),
            0x00d9 => dop2000.is_some() && dop2002.is_none(),
            0x0101 => dop2002.is_some() && dop2003.is_none(),
            0x010c => dop2003.is_some() && dop2007.is_none(),
            0x112 => match lcb_dop {
                674 => dop2007.is_some(),
                690 => dop2010.is_some(),
                694 => dop2013.is_some(),
                _ => false,
            },
            _ => false,
        };
        Ok(Self {
            dopbase,
            dop95,
            dop97,
            dop2000,
            dop2002,
            dop2003,
            dop2007,
            dop2010,
            dop2013,
            conformant,
        })
    }
}
