use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Read, Seek, Write},
    rc::Rc,
};

use ctxutils::io::{rdf64le, rdu8, rdu16le, rdu32le};
use record::{BeginSst, BundleSh, OleObject, RecordReader, SSTItem};
use record_type::RecordType;
use structs::{FromReader, RkNumber, XLNullableWideString, XLWideString};
use tracing::warn;

use crate::{
    OoxmlError, ProcessingSummary, Relationship, RelationshipType,
    archive::{Archive, Entry},
    drawing::Drawing,
    relationship::{FileToProcess, TargetMode},
    xlsx::{RowInfo, SharedStringEntry, SheetInfo, find_regions},
};

mod record;
mod record_type;
mod structs;

/// Parser for Excel files (binary format)
pub struct BinaryWorkbook<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    sheets: Vec<SheetInfo>,
    shared_strings: Option<Rc<RefCell<BinarySharedStrings>>>,
    files_to_process: Vec<FileToProcess>,
    protection: HashMap<String, String>,
    relationships: Vec<Relationship>,
}

struct BinarySharedStrings {
    record_reader: RecordReader<Entry>,
    entries: Vec<SharedStringEntry>,
}

impl BinarySharedStrings {
    fn open<R: Read + Seek>(
        archive: &Rc<Archive<R>>,
        path: &str,
        mut shared_strings_cache_limit: u64,
    ) -> Result<BinarySharedStrings, OoxmlError> {
        let entry = archive.find_entry(path, false)?;
        let mut record_reader = RecordReader::new(entry)?;
        let mut entries = Vec::<SharedStringEntry>::new();

        record_reader
            .next_record::<BeginSst>()?
            .ok_or("Unexpected EOF")?;
        while let Some(record_header) = record_reader.next_record_header()? {
            if record_header.record_type != RecordType::SSTItem {
                break;
            }
            let entry = if shared_strings_cache_limit == 0 {
                SharedStringEntry::NotCached {
                    offset: record_header.data_offset,
                    size: record_header.data_size,
                }
            } else {
                let sst_item = record_reader.read_record::<SSTItem>(&record_header)?;
                let size = u64::try_from(sst_item.value.len())?;
                shared_strings_cache_limit = shared_strings_cache_limit.saturating_sub(size);
                SharedStringEntry::Cached(sst_item.value)
            };
            entries.push(entry);
        }

        Ok(BinarySharedStrings {
            record_reader,
            entries,
        })
    }

    pub(crate) fn get(&mut self, index: usize) -> Result<String, OoxmlError> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| format!("Invalid index {index}"))?;
        let string = match entry {
            SharedStringEntry::Cached(string) => string.to_string(),
            SharedStringEntry::NotCached { offset, size } => {
                let mut reader = self.record_reader.read_raw_data(*offset, *size)?;
                let sst_item = SSTItem::from_reader(&mut reader)?;
                sst_item.value
            }
        };
        Ok(string)
    }
}

impl<R: Read + Seek> BinaryWorkbook<R> {
    pub(crate) fn open(
        archive: &Rc<Archive<R>>,
        path: &str,
        shared_strings_cache_limit: u64,
    ) -> Result<BinaryWorkbook<R>, OoxmlError> {
        let mut sheets = Vec::<SheetInfo>::new();
        let relationships =
            Relationship::load_relationships_for(archive, path)?.unwrap_or_default();
        let files_to_process = Vec::<FileToProcess>::new();
        let entry = archive.find_entry(path, false)?;
        let mut record_reader = RecordReader::new(entry)?;
        let mut protection = HashMap::<String, String>::new();

        while let Some(record_header) = record_reader.next_record_header()? {
            if record_header.record_type == RecordType::BundleSh {
                let mut slice = record_reader.read_record_data(&record_header)?;
                let record = BundleSh::from_reader(&mut slice)?;
                if let Some(sheet_info) = record.convert_to_sheet_info(&relationships) {
                    sheets.push(sheet_info);
                }
            };
            if record_header.record_type == RecordType::BookProtection {
                let mut slice = record_reader.read_record_data(&record_header)?;
                let protpwd_book = rdu16le(&mut slice)?;
                let protpwd_rev = rdu16le(&mut slice)?;
                let w_flags = rdu16le(&mut slice)?;
                let f_lock_structure = w_flags & 1 != 0;
                let f_lock_window = w_flags & 2 != 0;
                let f_lock_revision = w_flags & 4 != 0;
                protection.insert("workbook_password".to_string(), protpwd_book.to_string());
                protection.insert("revisions_password".to_string(), protpwd_rev.to_string());
                protection.insert("lock_sructure".to_string(), f_lock_structure.to_string());
                protection.insert("lock_windows".to_string(), f_lock_window.to_string());
                protection.insert("lock_revision".to_string(), f_lock_revision.to_string());
            }
        }

        let shared_strings = match relationships
            .iter()
            .find(|&relationship| relationship.rel_type == RelationshipType::SharedStrings)
        {
            Some(relationship) => match &relationship.target {
                crate::relationship::TargetMode::Internal(target) => {
                    Some(Rc::new(RefCell::new(BinarySharedStrings::open(
                        archive,
                        target.as_str(),
                        shared_strings_cache_limit,
                    )?)))
                }
                crate::relationship::TargetMode::External(_) => None,
            },
            None => None,
        };

        Ok(Self {
            archive: archive.clone(),
            files_to_process,
            protection,
            sheets,
            shared_strings,
            relationships,
        })
    }

    /// Returns a list of interesting files which might not be referenced on sheets (e.g. VBA macros).
    pub fn files_to_process(&self) -> &Vec<FileToProcess> {
        &self.files_to_process
    }

    /// Returns iterator over document Sheets
    pub fn iter(&self) -> SheetIterator<R> {
        SheetIterator {
            workbook: self,
            index: 0,
        }
    }

    /// Returns reference to hashmap containing document protection information.
    pub fn protection(&self) -> &HashMap<String, String> {
        &self.protection
    }

    /// Returns reference to workbook relationships
    pub fn relationships(&self) -> &Vec<Relationship> {
        &self.relationships
    }

    /// Returns path of vba project
    pub fn get_vba_path(&self) -> Option<String> {
        Relationship::list_vba(&self.relationships)
            .iter()
            .find_map(|r| {
                if let TargetMode::Internal(target) = &r.target {
                    Some(target.clone())
                } else {
                    None
                }
            })
    }
}

/// Iterator over document Sheets
pub struct SheetIterator<'a, R: Read + Seek> {
    workbook: &'a BinaryWorkbook<R>,
    index: usize,
}

impl<'a, R: Read + Seek> Iterator for SheetIterator<'a, R> {
    type Item = BinarySheet<R>;

    fn next(&mut self) -> Option<Self::Item> {
        let sheet_info = self.workbook.sheets.get(self.index)?;
        let sheet = BinarySheet::new(self.workbook, sheet_info).ok()?;
        self.index += 1;
        Some(sheet)
    }
}

/// Parser for Sheet inside Excel file
pub struct BinarySheet<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    path: String,
    relationships: Vec<Relationship>,
    sheet_info: SheetInfo,
    shared_strings: Option<Rc<RefCell<BinarySharedStrings>>>,
}

impl<R: Read + Seek> BinarySheet<R> {
    pub(crate) fn new(workbook: &BinaryWorkbook<R>, info: &SheetInfo) -> Result<Self, OoxmlError> {
        let archive = workbook.archive.clone();
        let relationships =
            (Relationship::load_relationships_for(&archive, &info.path)?).unwrap_or_default();

        Ok(Self {
            archive,
            path: info.path.to_string(),
            sheet_info: info.clone(),
            relationships,
            shared_strings: workbook.shared_strings.clone(),
        })
    }

    /// Returns sheet info
    pub fn info(&self) -> &SheetInfo {
        &self.sheet_info
    }

    /// Parses sheet. Document content is extracted to writer argument.
    /// Returns ProessingSummary struct containing list of files referenced in document and detected sheet protection.
    pub fn process<W: Write>(
        &mut self,
        writer: &mut W,
        processing_summary: &mut ProcessingSummary,
    ) -> Result<(), OoxmlError> {
        let entry = self.archive.find_entry(&self.path, false)?;
        let mut reader = RecordReader::new(entry)?;

        let rows = self.process_scann(&mut reader, processing_summary, writer)?;
        let regions = find_regions(&rows);
        let row_reader = RowReader {
            rows,
            record_reader: &mut reader,
            shared_strings: self.shared_strings.clone(),
            current_cell: None,
        };
        process_regions(regions, row_reader, writer, processing_summary)?;
        Ok(())
    }

    fn process_scann<W: Write>(
        &mut self,
        reader: &mut RecordReader<Entry>,
        processing_summary: &mut ProcessingSummary,
        writer: &mut W,
    ) -> Result<Vec<RowInfo>, OoxmlError> {
        let mut last_row: Option<u32> = None;
        let mut current_row: Option<RowInfo> = None;
        let mut skip_row = false;
        let mut rows = Vec::<RowInfo>::new();
        while let Some(header) = reader.next_record_header()? {
            if skip_row
                && ![RecordType::RowHdr, RecordType::EndSheetData].contains(&header.record_type)
            {
                continue;
            }
            skip_row = false;
            match header.record_type {
                RecordType::EndSheetData => {
                    if let Some(row) = current_row.take() {
                        rows.push(row);
                    }
                }
                RecordType::RowHdr => {
                    let mut slice = reader.read_record_data(&header)?;
                    let row_index = rdu32le(&mut slice)?;

                    if let Some(last_row) = &last_row {
                        if row_index <= *last_row {
                            // Detected row with invalid index. Skip it.
                            skip_row = true;
                            continue;
                        }
                    }
                    last_row = Some(row_index);
                    if let Some(row) = current_row.take() {
                        rows.push(row);
                    }
                    current_row = Some(RowInfo {
                        offset: header.offset,
                        size: 0,
                        index: row_index,
                        columns: Vec::new(),
                        regions: 0,
                    });
                }
                RecordType::CellBool
                | RecordType::CellError
                | RecordType::CellIsst
                | RecordType::CellRString
                | RecordType::CellReal
                | RecordType::CellRk
                | RecordType::CellSt
                | RecordType::FmlaBool
                | RecordType::FmlaError
                | RecordType::FmlaNum
                | RecordType::FmlaString => {
                    let current_row = match &mut current_row {
                        Some(row) => row,
                        None => {
                            warn!("current row in null");
                            continue;
                        }
                    };
                    let mut slice = reader.read_record_data(&header)?;
                    let column_index = rdu32le(&mut slice)?;
                    if Some(&column_index) <= current_row.columns.last() {
                        // Detected invalid column index. Skip it.
                        continue;
                    }
                    current_row.columns.push(column_index);
                    processing_summary.num_cells_detected += 1;
                }
                RecordType::Drawing
                | RecordType::LegacyDrawing
                | RecordType::LegacyDrawingHF
                | RecordType::OleObject => {
                    let mut slice = reader.read_record_data(&header)?;
                    let rel_id = if matches!(header.record_type, RecordType::OleObject) {
                        let ole_object = OleObject::from_reader(&mut slice)?;
                        ole_object.str_rel_id
                    } else {
                        XLNullableWideString::from_reader(&mut slice)?
                    };
                    if let Some(rel_id) = rel_id.data {
                        let pair = self
                            .find_relationship(&rel_id)
                            .map(|r| {
                                if let TargetMode::Internal(target) = &r.target {
                                    Some((r.rel_type.clone(), target.clone()))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_default();
                        if let Some((rel_type, path)) = pair {
                            if rel_type == RelationshipType::Drawing {
                                let mut drawing = Drawing::open(&self.archive, &path)?;
                                drawing.process(writer, processing_summary)?;
                            } else if !processing_summary.contains(&path) {
                                processing_summary
                                    .files_to_process
                                    .push(FileToProcess { path, rel_type })
                            }
                        }
                    }
                }
                RecordType::SheetProtection => {
                    let mut slice = reader.read_record_data(&header)?;
                    processing_summary
                        .protection
                        .insert("password".to_string(), rdu16le(&mut slice)?.to_string());
                    for key in [
                        "sheet",
                        "objects",
                        "scenarios",
                        "format_cells",
                        "format_columns",
                        "format_rows",
                        "insert_columns",
                        "insert_rows",
                        "insert_hyperlinks",
                        "delete_columns",
                        "delete_rows",
                        "select_locked_cells",
                        "sort",
                        "auto_filter",
                        "pivot_tables",
                        "select_unlocked_cells",
                    ] {
                        processing_summary
                            .protection
                            .insert(key.to_string(), rdu32le(&mut slice)?.to_string());
                    }
                }
                _ => {}
            }
        }
        Ok(rows)
    }

    fn find_relationship(&self, id: &str) -> Option<&Relationship> {
        self.relationships
            .iter()
            .find(|&relationship| relationship.id == id)
    }

    /// Returns sheet path in archive
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns reference to sheet relationships
    pub fn relationships(&self) -> &Vec<Relationship> {
        &self.relationships
    }
}

fn process_regions<W: Write>(
    regions: Vec<crate::xlsx::Region>,
    mut row_reader: RowReader<Entry>,
    writer: &mut W,
    processing_summary: &mut ProcessingSummary,
) -> Result<(), OoxmlError> {
    for region in &regions {
        for row in region.top..=region.bottom {
            for column in region.left..=region.right {
                let value = row_reader.get_cell_value(row, column)?;
                if column != region.left {
                    writer.write_all(b",")?;
                }
                if let Some(value) = &value {
                    writer.write_all(value.as_bytes())?;
                }
                processing_summary.num_cells_processed += 1;
            }
            writer.write_all(b"\n")?;
        }
    }
    Ok(())
}

struct RowReader<'a, R: Read + Seek> {
    record_reader: &'a mut RecordReader<R>,
    rows: Vec<RowInfo>,
    shared_strings: Option<Rc<RefCell<BinarySharedStrings>>>,
    current_cell: Option<(u32, u32)>,
}

impl<'a, R: Read + Seek> RowReader<'a, R> {
    fn get_cell_value(&mut self, row: u32, column: u32) -> Result<Option<String>, OoxmlError> {
        let need_seek = if let Some(current_cell) = &self.current_cell {
            current_cell.0 != row || current_cell.1 > column
        } else {
            true
        };
        if need_seek {
            let row = match self.rows.binary_search_by(|r| r.index.cmp(&row)) {
                Ok(row) => &self.rows[row],
                Err(_) => return Ok(None),
            };
            if row.columns.binary_search(&column).is_err() {
                return Ok(None);
            }
            self.record_reader.seek(row.offset)?;
            self.record_reader.next_record_header()?;
        }
        while let Some(header) = self.record_reader.next_record_header()? {
            match &header.record_type {
                RecordType::EndSheetData | RecordType::RowHdr => {
                    break;
                }
                RecordType::CellBool
                | RecordType::CellError
                | RecordType::CellIsst
                | RecordType::CellRString
                | RecordType::CellReal
                | RecordType::CellRk
                | RecordType::CellSt
                | RecordType::FmlaBool
                | RecordType::FmlaError
                | RecordType::FmlaNum
                | RecordType::FmlaString => {
                    let mut slice = self.record_reader.read_record_data(&header)?;
                    let column_index = rdu32le(&mut slice)?;
                    self.current_cell = Some((row, column_index));
                    if column_index > column {
                        break;
                    }
                    if column_index != column {
                        continue;
                    }
                    rdu32le(&mut slice)?;
                    let result = match &header.record_type {
                        RecordType::CellBool | RecordType::FmlaBool => {
                            let byte = rdu8(&mut slice)?;
                            if byte == 0 { "false" } else { "true" }.to_string()
                        }
                        RecordType::CellError | RecordType::FmlaError => {
                            let byte = rdu8(&mut slice)?;
                            match byte {
                                0x00 => "#NULL!",
                                0x07 => "#DIV/0!",
                                0x0F => "#VALUE!",
                                0x17 => "#REF!",
                                0x1D => "#NAME?",
                                0x24 => "#NUM!",
                                0x2A => "#N/A",
                                0x2B => "#GETTING_DATA",
                                _ => "#INVALID_ERROR_CODE",
                            }
                            .to_string()
                        }
                        RecordType::CellSt | RecordType::FmlaString => {
                            let string = XLWideString::from_reader(&mut slice)?;
                            string.data
                        }
                        RecordType::CellReal | RecordType::FmlaNum => {
                            let real = rdf64le(&mut slice)?;
                            format!("{real}")
                        }
                        RecordType::CellIsst => {
                            let index = usize::try_from(rdu32le(&mut slice)?)?;
                            let mut sst = match &self.shared_strings {
                                Some(sst) => sst.borrow_mut(),
                                None => return Ok(None),
                            };
                            sst.get(index)?
                        }
                        RecordType::CellRString => {
                            rdu8(&mut slice)?;
                            let string = XLWideString::from_reader(&mut slice)?;
                            string.data
                        }
                        RecordType::CellRk => {
                            let rknum = RkNumber::from_reader(&mut slice)?;
                            rknum.to_string()
                        }
                        _ => unreachable!(),
                    };
                    return Ok(Some(result));
                }
                _ => (),
            }
        }
        Ok(None)
    }
}
