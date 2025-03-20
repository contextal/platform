//! *Workbook* associated structures
use crate::{
    ExcelError, RecordStream, SubstreamType,
    macrosheet::MacroSheet,
    records::{
        Bof, BoundSheet8, CodePage, Password, Protect, RecordType, SST, WsBool,
        from_record::{Anomalies, FromRecordStream},
    },
    structures::BofType,
    worksheet::Worksheet,
};
use ctxole::{Ole, OleEntry, crypto};
use std::{
    fmt,
    io::{Read, Seek},
};

/// Sheet locator
#[derive(Debug, Clone)]
pub struct SheetInfo {
    pub(crate) offset: u64,
    /// Name of this Sheet
    pub name: String,
    /// Sheet hidden state
    pub state: String,
    /// Sheet type
    pub sheet_type: SubstreamType,
}

/// *Workbook* protection and encryption data
#[derive(Debug, Default)]
pub struct Protection {
    /// Legacy encryption/obfuscation key
    pub encryption_key: Option<crypto::LegacyKey>,
    /// Write-protection flag
    pub write_protect: bool,
    /// *Workbook* protection flag
    pub workbook_protected: bool,
    password_verifier: u16,
}

/// *Workbook*
pub struct Workbook<'a, R: Read + Seek> {
    pub(crate) ole: &'a Ole<R>,
    pub(crate) entry: OleEntry,
    /// Offset of the *Workbook* within the containing stream
    pub offset: u64,
    /// Code page in use in the *Workbook*
    pub codepage: Option<CodePage>,
    pub(crate) shared_strings: Option<SST>,
    worksheet_info: Vec<SheetInfo>,
    macrosheet_info: Vec<SheetInfo>,
    other_sheet_info: Vec<SheetInfo>,
    /// Protection rules applied to the *Workbook*
    pub protection: Protection,
    /// Problems encountered while parsing
    pub anomalies: Anomalies,
}

impl<R: Read + Seek> fmt::Debug for Workbook<'_, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Workbook")
            .field("offset", &self.offset)
            .field("codepage", &self.codepage)
            .field("shared_strings", &self.shared_strings)
            .field("worksheet_info", &self.worksheet_info)
            .field("protection", &self.protection)
            .field("anomalies", &self.anomalies)
            .finish()
    }
}

impl<'a, R: Read + Seek> Workbook<'a, R> {
    pub(crate) fn new(
        ole: &'a Ole<R>,
        entry: OleEntry,
        encryption_key: Option<crypto::LegacyKey>,
    ) -> Result<Self, ExcelError> {
        let ole_stream = ole.get_stream_reader(&entry);
        let mut wb = if let Some(ref key) = encryption_key {
            crypto::LegacyDecryptor::new(ole_stream, key, 0)
        } else {
            crypto::LegacyDecryptor::new_no_op(ole_stream)
        };
        let mut record_stream = RecordStream::new(&mut wb)?;
        let stream = &mut record_stream;
        let mut protection = Protection {
            encryption_key,
            ..Protection::default()
        };
        let mut anomalies = Anomalies::new();
        let offset = stream.offset;
        let bof = Bof::from_record(stream, &mut anomalies)?;
        if bof.dt != BofType::Workbook {
            return Err(format!("Unexpected BOF type {:?}", bof.dt).into());
        }
        let mut codepage = None;
        let mut shared_strings = None;
        let mut boundsheet8 = Vec::<BoundSheet8>::new();
        loop {
            if !stream.next()? {
                break;
            }
            match stream.ty {
                RecordType::EOF | RecordType::BOF => break,
                RecordType::CodePage => {
                    codepage = Some(CodePage::from_record(stream, &mut anomalies)?);
                }
                RecordType::SST => {
                    shared_strings = Some(SST::from_record(stream, &mut anomalies)?);
                }
                RecordType::BoundSheet8 => {
                    let v = BoundSheet8::from_record(stream, &mut anomalies)?;
                    boundsheet8.push(v);
                }
                RecordType::WriteProtect => {
                    protection.write_protect = true;
                }
                RecordType::Protect => {
                    let v = Protect::from_record(stream, &mut anomalies)?;
                    protection.workbook_protected = v.f_lock != 0;
                }
                RecordType::Password => {
                    let v = Password::from_record(stream, &mut anomalies)?;
                    protection.password_verifier = v.w_password;
                }
                _ => {}
            }
        }
        let mut worksheet_info = Vec::<SheetInfo>::new();
        let mut macrosheet_info = Vec::<SheetInfo>::new();
        let mut other_sheet_info = Vec::<SheetInfo>::new();
        'outer: loop {
            while stream.ty != RecordType::BOF {
                if !stream.can_read_next() || !stream.next()? {
                    break 'outer;
                }
            }
            let offset = stream.offset;
            let bof = Bof::from_record(stream, &mut anomalies)?;
            let sheet_type = match bof.dt {
                BofType::DialogOrWorksheet => {
                    let mut wsbool = None;
                    loop {
                        if !stream.next()? {
                            break;
                        }
                        match stream.ty {
                            RecordType::EOF => break,
                            RecordType::BOF => stream.skip_subtream()?,
                            RecordType::WsBool => {
                                wsbool = Some(WsBool::from_record(stream, &mut anomalies)?);
                            }
                            _ => continue,
                        }
                    }
                    if wsbool.is_none() {
                        // Unable to recognize sheet type
                        continue;
                    }
                    if wsbool.unwrap().f_dialog {
                        SubstreamType::Dialogsheet
                    } else {
                        SubstreamType::Worksheet
                    }
                }
                BofType::MacroSheet => {
                    if !stream.next()? {
                        break;
                    }
                    SubstreamType::Macrosheet
                }
                BofType::ChartSheet => {
                    if !stream.next()? {
                        break;
                    }
                    SubstreamType::Chartsheet
                }
                _ => {
                    stream.skip_subtream()?;
                    continue;
                }
            };

            let boundsheet8 = boundsheet8
                .iter()
                .find(|b| u64::from(b.lb_ply_pos) == offset);

            let (name, state) = if let Some(boundsheet8) = boundsheet8 {
                let name = boundsheet8.st_name.to_string();
                let state = match boundsheet8.hs_state {
                    crate::structures::HSState::Visible => "visible",
                    crate::structures::HSState::Hidden => "hidden",
                    crate::structures::HSState::VeryHidden => "veryHidden",
                    crate::structures::HSState::Invalid(_) => "Unknown",
                }
                .to_string();
                // TODO: Should we compare substream type with boundsheet8 type?
                (name, state)
            } else {
                (String::new(), String::from("Unknown"))
            };

            let sheet_info = SheetInfo {
                offset,
                name,
                state,
                sheet_type,
            };

            match &sheet_info.sheet_type {
                SubstreamType::Worksheet => worksheet_info.push(sheet_info),
                SubstreamType::Macrosheet => macrosheet_info.push(sheet_info),
                _ => other_sheet_info.push(sheet_info),
            };
        }

        Ok(Self {
            ole,
            entry,
            offset,
            codepage,
            shared_strings,
            worksheet_info,
            macrosheet_info,
            other_sheet_info,
            protection,
            anomalies,
        })
    }

    /// Return an iterator over document Worksheets
    pub fn worksheets(&'a self) -> WorksheetIterator<'a, R> {
        WorksheetIterator {
            workbook: self,
            index: 0,
        }
    }

    /// Return an iterator over document Macrosheets
    pub fn macrosheets(&'a self) -> MacrosheetIterator<'a, R> {
        MacrosheetIterator {
            workbook: self,
            index: 0,
        }
    }

    /// Returs information about additional sheets detected in document (dialogsheet, chartsheet)
    pub fn additional_sheets(&self) -> &[SheetInfo] {
        &self.other_sheet_info
    }
}

/// Iterator over the *Worksheets* in a *Workbook*
pub struct WorksheetIterator<'a, R: Read + Seek> {
    workbook: &'a Workbook<'a, R>,
    index: usize,
}

impl<'a, R: Read + Seek> Iterator for WorksheetIterator<'a, R> {
    type Item = Worksheet<'a, R>;

    fn next(&mut self) -> Option<Self::Item> {
        let worksheet_info = self.workbook.worksheet_info.get(self.index)?.clone();
        let worksheet = Worksheet::new(self.workbook, worksheet_info).ok()?;
        self.index += 1;
        Some(worksheet)
    }
}

/// Iterator over the *Macrosheets* in a *Workbook*
pub struct MacrosheetIterator<'a, R: Read + Seek> {
    workbook: &'a Workbook<'a, R>,
    index: usize,
}

impl<'a, R: Read + Seek> Iterator for MacrosheetIterator<'a, R> {
    type Item = MacroSheet<'a, R>;

    fn next(&mut self) -> Option<Self::Item> {
        let macrosheet_info = self.workbook.macrosheet_info.get(self.index)?.clone();
        let macrosheet = MacroSheet::new(self.workbook, macrosheet_info).ok()?;
        self.index += 1;
        Some(macrosheet)
    }
}
