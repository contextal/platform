//! *Worksheet* associated structures
use crate::{
    DataLocation, ExcelError, RecordStream,
    records::{
        from_record::{Anomalies, FromRecordStream},
        *,
    },
    structures::BofType,
    workbook::{SheetInfo, Workbook},
};
use ctxole::crypto;
use std::{
    collections::{BTreeMap, LinkedList},
    fmt,
    io::{Read, Seek, Write},
};

/// *Worksheet*
#[derive(Debug)]
pub struct Worksheet<'a, R: Read + Seek> {
    workbook: &'a Workbook<'a, R>,
    /// *Worksheet* locator
    pub worksheet_info: SheetInfo,
    /// Problems encountered while parsing
    pub anomalies: Anomalies,
}

/// Sheet processing result
#[derive(Debug, Default)]
pub struct ProcessingResult {
    /// Non empty cells detected
    pub num_cells_detected: u64,
    /// Non empty cells processed
    pub num_cells_processed: u64,
    /// Extra objects encountered during the processing of a sheet
    pub extra_objects: Vec<DataLocation>,
}

impl<'a, R: Read + Seek> Worksheet<'a, R> {
    pub(crate) fn new(
        workbook: &'a Workbook<'a, R>,
        worksheet_info: SheetInfo,
    ) -> Result<Self, ExcelError> {
        let anomalies = Anomalies::new();
        Ok(Worksheet {
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
        if bof.dt != BofType::DialogOrWorksheet {
            return Err(format!("Invalid BOF type: {:?}", bof.dt).into());
        }
        loop {
            if !stream.next()? {
                break;
            }
            match stream.ty {
                RecordType::EOF => return Err("Unable to find WsBool record".into()),
                RecordType::BOF => stream.skip_subtream()?,
                RecordType::WsBool => {
                    let wsbool = WsBool::from_record(&mut stream, &mut anomalies)?;
                    if wsbool.f_dialog {
                        return Err("Current substream is dialog, expacting worksheet".into());
                    }
                    break;
                }
                _ => {}
            };
        }

        stream.seek(self.worksheet_info.offset)?;
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

pub(crate) fn find_rows<R: Read + Seek>(
    stream: &mut RecordStream<R>,
    anomalies: &mut Anomalies,
    data_to_process: &mut Vec<DataLocation>,
    num_cells_detected: &mut u64,
) -> Result<BTreeMap<u16, RowInfo>, ExcelError> {
    let mut rows = BTreeMap::<u16, RowInfo>::new();
    loop {
        if !stream.next()? {
            break;
        }
        let offset = stream.offset;
        match &stream.ty {
            RecordType::EOF => break,
            RecordType::BOF => {
                // TODO: recognize and process nested substream
                stream.skip_subtream()?;
                continue;
            }
            RecordType::Obj => {
                match Obj::from_record(stream, anomalies) {
                    Ok(obj) => {
                        if let Some(data_location) = obj.data_location() {
                            data_to_process.push(data_location);
                        }
                    }
                    Err(err) => anomalies.push(format!(
                        "Failed to parse Obj record at 0x{offset:x} offset: {err:?}"
                    )),
                };
                continue;
            }
            ty if !CELL_RECORDS.contains(ty) => {
                continue;
            }
            _ => {}
        }

        let record_type = stream.ty;
        let (row, first_column, last_column): (u16, u16, u16) = match &stream.ty {
            RecordType::RK => {
                let v = RK::from_record(stream, anomalies)?;
                (v.rw, v.col, v.col)
            }
            RecordType::MulRk => {
                let v = MulRk::from_record(stream, anomalies)?;
                (v.rw, v.col_first, v.col_last)
            }
            RecordType::BoolErr => {
                let v = BoolErr::from_record(stream, anomalies)?;
                (v.cell.rw, v.cell.col, v.cell.col)
            }
            RecordType::Number => {
                let v = Number::from_record(stream, anomalies)?;
                (v.cell.rw, v.cell.col, v.cell.col)
            }
            RecordType::Formula => {
                let v = Formula::from_record(stream, anomalies)?;
                (v.cell.rw, v.cell.col, v.cell.col)
            }
            RecordType::LabelSst => {
                let v = LabelSst::from_record(stream, anomalies)?;
                (v.cell.rw, v.cell.col, v.cell.col)
            }
            _ => unreachable!(),
        };
        *num_cells_detected += u64::from(last_column.saturating_sub(first_column)) + 1;
        let row_info = match rows.get_mut(&row) {
            Some(r) => r,
            None => {
                let r = RowInfo {
                    index: row,
                    columns: BTreeMap::new(),
                };
                rows.insert(row, r);
                //safe, element with key was just inserted
                rows.get_mut(&row).unwrap()
            }
        };
        let column_info = ColumnInfo {
            first: first_column,
            last: last_column,
            offset,
            record_type,
        };
        row_info.insert(column_info);
    }
    Ok(rows)
}

pub(crate) fn find_regions(rows: &BTreeMap<u16, RowInfo>) -> Vec<Region> {
    let mut result = Vec::<Region>::new();
    if rows.is_empty() {
        return result;
    }
    let mut current_regions = LinkedList::<Region>::new();
    let mut last_row: Option<u16> = None;

    for row in rows.values() {
        // First find continous areas in row
        let mut current_line_regions = LinkedList::<Region>::new();
        for column in row.columns.values() {
            if let Some(region) = current_line_regions.back_mut() {
                if region.right + 1 == column.first {
                    // Resize last region
                    region.right = column.last;
                    continue;
                }
            }
            // Create new region
            current_line_regions.push_back(Region {
                top: row.index,
                bottom: row.index,
                left: column.first,
                right: column.last,
            });
        }

        if let Some(last_row) = last_row {
            if last_row + 1 != row.index {
                // Move regions from current_regions to result
                while let Some(region) = current_regions.pop_front() {
                    result.push(region);
                }
            } else {
                // Merge adjesting regions, store them in current_line_regions
                'outer: while let Some(previous_region) = current_regions.pop_front() {
                    for region in &mut current_line_regions {
                        if region.adjacent_to_region(&previous_region) {
                            region.merge_region(&previous_region);
                            continue 'outer;
                        }
                    }
                    result.push(previous_region);
                }
                // Move regions from current_regions to result
                while let Some(region) = current_regions.pop_front() {
                    result.push(region);
                }
                // Check does new regions are adjacent
                let mut tmp = current_line_regions;
                current_line_regions = LinkedList::<Region>::new();

                while let Some(region) = tmp.pop_front() {
                    if let Some(last_region) = current_line_regions.back_mut() {
                        if last_region.adjacent_to_region(&region) {
                            last_region.merge_region(&region);
                            continue;
                        }
                    }
                    current_line_regions.push_back(region);
                }
            }
        }
        current_regions = current_line_regions;

        last_row = Some(row.index);
    }

    // Store remaining regions from last row in result
    while let Some(region) = current_regions.pop_front() {
        result.push(region);
    }
    use std::cmp::Ordering;
    result.sort_by(|a, b| {
        match a.top.cmp(&b.top) {
            Ordering::Equal => {}
            ord => return ord,
        }
        a.left.cmp(&b.left)
    });
    result
}

pub(crate) struct Region {
    pub(crate) top: u16,
    pub(crate) bottom: u16,
    pub(crate) left: u16,
    pub(crate) right: u16,
}

pub(crate) const CELL_RECORDS: &[RecordType] = &[
    RecordType::RK,
    RecordType::MulRk,
    RecordType::BoolErr,
    RecordType::Number,
    RecordType::Formula,
    RecordType::LabelSst,
];

#[derive(Debug)]
pub(crate) struct ColumnInfo {
    record_type: RecordType,
    offset: u64,
    first: u16,
    last: u16,
}

#[derive(Debug)]
pub(crate) struct RowInfo {
    index: u16,
    columns: BTreeMap<u16, ColumnInfo>,
}

impl RowInfo {
    fn insert(&mut self, column_info: ColumnInfo) {
        self.columns.insert(column_info.first, column_info);
    }
}

impl Region {
    fn adjacent_to_region(&self, region: &Region) -> bool {
        if (region.bottom + 1 == self.top || self.bottom + 1 == region.top)
            && (region.right + 1 == self.left || self.right + 1 == region.left)
        {
            return false;
        }
        region.bottom + 1 >= self.top
            && self.bottom + 1 >= region.top
            && region.right + 1 >= self.left
            && self.right + 1 >= region.left
    }

    fn merge_region(&mut self, region: &Region) {
        self.top = std::cmp::min(self.top, region.top);
        self.left = std::cmp::min(self.left, region.left);
        self.bottom = std::cmp::max(self.bottom, region.bottom);
        self.right = std::cmp::max(self.right, region.right);
    }
}

impl fmt::Debug for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = to_cell_ref(self.top, self.left);
        let y = to_cell_ref(self.bottom, self.right);
        write!(f, "Region {x}-{y}")
    }
}

fn to_cell_ref(row: u16, column: u16) -> String {
    let mut column_str = String::new();
    let mut column = u32::from(column);
    loop {
        // unwrap is safe
        let c = b'A' + u8::try_from(column % 26).unwrap();
        column_str.push(c.into());
        column /= 26;
        if column == 0 {
            break;
        }
    }
    let result = format!(
        "{}{}",
        column_str.chars().rev().collect::<String>(),
        u32::from(row) + 1
    );
    result
}

pub(crate) struct CellReader<'a, R: Read + Seek> {
    pub(crate) sst: Option<&'a SST>,
    pub(crate) stream: &'a mut RecordStream<'a, R>,
    pub(crate) rows: BTreeMap<u16, RowInfo>,
    pub(crate) current_mulrk: Option<MulRk>,
}

impl<R: Read + Seek> CellReader<'_, R> {
    pub(crate) fn get_cell_value(
        &mut self,
        row: u16,
        column: u16,
    ) -> Result<Option<String>, ExcelError> {
        if let Some(mulrk) = &self.current_mulrk {
            if mulrk.rw == row && column >= mulrk.col_first && column <= mulrk.col_last {
                let index = mulrk.col_last - mulrk.col_first;
                let value = mulrk.rgrkrec.get(usize::from(index));
                if let Some(value) = value {
                    return Ok(Some(value.to_string()));
                } else {
                    // Should I return error instead of None?
                    return Ok(None);
                }
            }
        }
        self.current_mulrk = None;
        let row_info = match self.rows.get(&row) {
            Some(r) => r,
            None => return Ok(None),
        };
        let mut column_info = None;
        for entry in row_info.columns.values() {
            if column >= entry.first && column <= entry.last {
                column_info = Some(entry);
                break;
            }
        }
        let column_info = match column_info {
            Some(c) => c,
            None => return Ok(None),
        };
        self.stream.seek(column_info.offset)?;
        let anomalies = &mut Anomalies::new();
        let result = match column_info.record_type {
            RecordType::MulRk => {
                let mulrk = MulRk::from_record(self.stream, anomalies)?;
                if mulrk.col_last < mulrk.col_first {
                    return Err("MulRk: col_last is lower than col_first".into());
                }
                let index = mulrk.col_last - mulrk.col_first;
                let value = mulrk.rgrkrec.get(usize::from(index));
                let result = value.map(|value| value.to_string());
                self.current_mulrk = Some(mulrk);
                result
            }
            RecordType::RK => {
                let v = RK::from_record(self.stream, anomalies)?;
                Some(v.rkrec.to_string())
            }
            RecordType::BoolErr => {
                let v = BoolErr::from_record(self.stream, anomalies)?;
                Some(v.bes.to_string())
            }
            RecordType::Number => {
                let v = Number::from_record(self.stream, anomalies)?;
                Some(v.num.to_string())
            }
            RecordType::Formula => {
                let v = Formula::from_record(self.stream, anomalies)?;
                match v.val {
                    super::structures::FormulaValue::Xnum(v) => Some(v.to_string()),
                    super::structures::FormulaValue::String(v) => Some(v),
                    super::structures::FormulaValue::Boolean(v) => Some(v.to_string()),
                    super::structures::FormulaValue::Error(v) => Some(v),
                    super::structures::FormulaValue::Blank => None,
                }
            }
            RecordType::LabelSst => {
                let sst = match self.sst {
                    Some(sst) => sst,
                    None => return Ok(None),
                };
                let v = LabelSst::from_record(self.stream, anomalies)?;
                let index = usize::try_from(v.isst)
                    .map_err(|_| "Failed to cast LabelSst::isst to usize")?;
                sst.rgb.get(index).map(|v| v.to_string())
            }
            _ => unreachable!(),
        };
        Ok(result)
    }
}
