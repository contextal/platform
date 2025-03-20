//! *Macro Sheet* associated structures
use ctxole::crypto;

use crate::{
    Bof,
    from_record::{Anomalies, FromRecordStream},
    structures::BofType,
    workbook::{SheetInfo, Workbook},
    worksheet::{CellReader, ProcessingResult, find_regions, find_rows},
};

use super::{ExcelError, RecordStream};
use std::io::{Read, Seek, Write};

/// A single, logical container that is used to store and run Excel 4.0 macro formulas
#[derive(Debug)]
pub struct MacroSheet<'a, R: Read + Seek> {
    workbook: &'a Workbook<'a, R>,
    /// *Worksheet* locator
    pub worksheet_info: SheetInfo,
    /// Problems encountered while parsing
    pub anomalies: Anomalies,
}

impl<'a, R: Read + Seek> MacroSheet<'a, R> {
    pub(crate) fn new(
        workbook: &'a Workbook<'a, R>,
        worksheet_info: SheetInfo,
    ) -> Result<Self, ExcelError> {
        let anomalies = Anomalies::new();
        Ok(MacroSheet {
            workbook,
            worksheet_info,
            anomalies,
        })
    }

    /// Process the sheet and write its body to the given `writer`
    ///
    /// processing_result reference allow to store partial result in case of processing Error
    pub fn process<W: Write>(
        &self,
        mut writer: W,
        processing_result: &mut ProcessingResult,
    ) -> Result<(), ExcelError> {
        let ole = self.workbook.ole;
        let entry = &self.workbook.entry;
        let reader = ole.get_stream_reader(entry);
        let mut wb = if let Some(key) = &self.workbook.protection.encryption_key {
            crypto::LegacyDecryptor::new(reader, key, 0)
        } else {
            crypto::LegacyDecryptor::new_no_op(reader)
        };
        let mut stream = RecordStream::new(&mut wb)?;
        stream.seek(self.worksheet_info.offset)?;
        let mut anomalies = Anomalies::new();

        let bof = Bof::from_record(&mut stream, &mut anomalies)?;
        if bof.dt != BofType::MacroSheet {
            return Err(format!("Invalid BOF type: {:?}", bof.dt).into());
        }

        let rows = find_rows(
            &mut stream,
            &mut anomalies,
            &mut processing_result.extra_objects,
            &mut processing_result.num_cells_detected,
        )?;
        let regions = find_regions(&rows);
        //debug!("{:#?}", regions);
        let mut cell_reader = CellReader {
            sst: self.workbook.shared_strings.as_ref(),
            stream: &mut stream,
            rows,
            current_mulrk: None,
        };
        for region in &regions {
            for row in region.top..=region.bottom {
                for column in region.left..=region.right {
                    let value = cell_reader.get_cell_value(row, column)?;
                    if column != region.left {
                        writer.write_all(b",")?;
                    }
                    if let Some(value) = &value {
                        writer.write_all(value.as_bytes())?;
                    }
                    processing_result.num_cells_processed += 1;
                }
                writer.write_all(b"\n")?;
            }
        }
        Ok(())
    }
}
