use crate::{
    archive::{self, Archive, Entry},
    drawing::Drawing,
    relationship::{FileToProcess, Relationship, RelationshipType, TargetMode},
    xml, OoxmlError, ParserState, ProcessingSummary,
};
use convert_case::{Case, Casing};
use ctxutils::cmp::Unsigned;
use std::{
    borrow::Borrow,
    char,
    collections::{HashMap, LinkedList, VecDeque},
    fmt::Debug,
    io::{self, BufRead, BufReader, Read, Seek, Write},
    num::TryFromIntError,
    rc::Rc,
    str::FromStr,
};
use tracing::{debug, warn};
use xml::reader::{EventReader, OwnedAttribute, XmlEvent};

/// Parser for Excel files
pub struct Workbook<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    sheets: Vec<SheetInfo>,
    shared_strings: Option<Rc<SharedStrings<R>>>,
    files_to_process: Vec<FileToProcess>,
    protection: HashMap<String, String>,
    relationships: Vec<Relationship>,
}

#[derive(Debug)]
pub(crate) enum SharedStringEntry {
    Cached(String),
    NotCached { offset: u64, size: usize },
}

struct SharedStrings<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    path: String,
    entries: Vec<SharedStringEntry>,
}

/// Sheet type
#[derive(Debug, Clone, PartialEq)]
pub enum SheetType {
    /// Worksheet
    Worksheet,
    ///  macrosheet
    Macrosheet,
    /// Dialogsheet
    Dialogsheet,
    /// Chartsheet
    Chartsheet,
}

impl SheetType {
    /// Returns sheet type name
    pub fn name(&self) -> &str {
        match &self {
            SheetType::Worksheet => "worksheet",
            SheetType::Macrosheet => "macrosheet",
            SheetType::Dialogsheet => "dialogsheet",
            SheetType::Chartsheet => "chartsheet",
        }
    }
}

/// Basic information about Sheet
#[derive(Clone)]
pub struct SheetInfo {
    /// Sheet identifier
    pub id: String,
    /// Sheet name
    pub name: String,
    /// Sheet state
    pub state: String,
    /// Sheet path inside ZIP archive
    pub path: String,
    /// Sheet type
    pub sheet_type: SheetType,
}

/// Parser for Sheet inside Excel file
pub struct Sheet<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    path: String,
    relationships: Vec<Relationship>,
    sheet_info: SheetInfo,
    parser: EventReader<archive::Entry>,
    parser_state: ParserState,
    shared_strings: Option<Rc<SharedStrings<R>>>,
    chunk_start: Option<OffsetReaderChunk>,
    chunk_end: Option<OffsetReaderChunk>,
}

impl<R: Read + Seek> Workbook<R> {
    pub(crate) fn open(
        archive: &Rc<Archive<R>>,
        path: &str,
        shared_strings_cache_limit: u64,
    ) -> Result<Workbook<R>, OoxmlError> {
        let mut sheets = Vec::<SheetInfo>::new();
        let relationships =
            (Relationship::load_relationships_for(archive, path)?).unwrap_or_default();
        let files_to_process = Vec::<FileToProcess>::new();
        let mut protection = HashMap::<String, String>::new();

        let entry = archive.find_entry(path, true)?;
        let mut parser = EventReader::new(entry);
        let event = match parser.next()? {
            XmlEvent::StartDocument { .. } => parser.next()?,
            event => event,
        };
        match event {
            XmlEvent::StartElement { name, .. } if name.local_name == "workbook" => {}
            _ => return Err("expecting: StartElement <workbook>".into()),
        }
        loop {
            match parser.next()? {
                XmlEvent::EndDocument | XmlEvent::EndElement { .. } => {
                    return Err("<sheets> element not found".into());
                }
                XmlEvent::StartElement { name, .. } if name.local_name.as_str() == "sheets" => {
                    break;
                }
                XmlEvent::StartElement {
                    name, attributes, ..
                } if name.local_name.as_str() == "workbookProtection" => {
                    for attribute in attributes {
                        let key = attribute.name.local_name.as_str().to_case(Case::Snake);
                        protection.insert(key, attribute.value);
                    }
                    parser.skip()?;
                }
                XmlEvent::StartElement { .. } => parser.skip()?,
                e => {
                    debug!("XML EVENT: {e:?}")
                }
            }
        }
        loop {
            match parser.next()? {
                XmlEvent::EndDocument => return Err("Unexpected end of xml file".into()),
                XmlEvent::StartElement {
                    name, attributes, ..
                } if name.local_name.as_str() == "sheet" => {
                    let mut name: Option<String> = None;
                    let mut sheet_id: Option<String> = None;
                    let mut state: Option<String> = None;
                    let mut relation_id: Option<String> = None;

                    for attribute in attributes {
                        match attribute.name.local_name.as_str() {
                            "name" => {
                                name = Some(attribute.value);
                            }
                            "sheetId" => {
                                sheet_id = Some(attribute.value);
                            }
                            "state" => {
                                state = Some(attribute.value);
                            }
                            "id" => relation_id = Some(attribute.value),
                            _ => {}
                        }
                    }
                    if name.is_none() || sheet_id.is_none() || relation_id.is_none() {
                        warn!("Missing one or more <sheet> attributes");
                        parser.skip()?;
                        continue;
                    }

                    let relation = match Workbook::<R>::find_relationship(
                        &relationships,
                        relation_id.as_ref().unwrap(),
                    ) {
                        Some(relation) => relation,
                        None => {
                            warn!("Invalid relation {relation_id:?}");
                            parser.skip()?;
                            continue;
                        }
                    };

                    let target = match &relation.target {
                        crate::relationship::TargetMode::Internal(target) => target,
                        crate::relationship::TargetMode::External(target) => {
                            warn!("External sheet '{target}' referenced");
                            parser.skip()?;
                            continue;
                        }
                    };

                    let sheet_type = match &relation.rel_type {
                        RelationshipType::Worksheet => SheetType::Worksheet,
                        RelationshipType::Macrosheet => SheetType::Macrosheet,
                        RelationshipType::Dialogsheet => SheetType::Dialogsheet,
                        RelationshipType::Chartsheet => SheetType::Chartsheet,
                        rel_type => {
                            warn!("Unexpected relation type {:?}", rel_type);
                            parser.skip()?;
                            continue;
                        }
                    };

                    sheets.push(SheetInfo {
                        id: sheet_id.unwrap(),
                        name: name.unwrap(),
                        state: state.unwrap_or_else(|| "visible".to_string()),
                        path: target.clone(),
                        sheet_type,
                    });
                    parser.skip()?;
                }
                XmlEvent::StartElement { .. } => parser.skip()?,
                XmlEvent::EndElement { .. } => break,
                _ => {}
            }
        }
        loop {
            match parser.next()? {
                XmlEvent::EndDocument => break,
                XmlEvent::StartElement {
                    name, attributes, ..
                } if name.local_name.as_str() == "workbookProtection" => {
                    for attribute in attributes {
                        let key = attribute.name.local_name.as_str().to_case(Case::Snake);
                        protection.insert(key, attribute.value);
                    }
                }
                XmlEvent::StartElement { .. } => {
                    parser.skip()?;
                }
                _ => {}
            }
        }

        let shared_strings = match relationships
            .iter()
            .find(|&relationship| relationship.rel_type == RelationshipType::SharedStrings)
        {
            Some(relationship) => match &relationship.target {
                crate::relationship::TargetMode::Internal(target) => Some(Rc::new(
                    SharedStrings::open(archive, target.as_str(), shared_strings_cache_limit)?,
                )),
                crate::relationship::TargetMode::External(_) => None,
            },
            None => None,
        };

        Ok(Workbook {
            archive: archive.clone(),
            sheets,
            shared_strings,
            files_to_process,
            protection,
            relationships,
        })
    }

    fn find_relationship<'a>(
        relationships: &'a [Relationship],
        id: &str,
    ) -> Option<&'a Relationship> {
        relationships
            .iter()
            .find(|&relationship| relationship.id == id)
    }

    /// Returns iterator over document Sheets
    pub fn iter(&self) -> SheetIterator<R> {
        SheetIterator {
            workbook: self,
            index: 0,
        }
    }

    /// Returns a list of interesting files which might not be referenced on sheets (e.g. VBA macros).
    pub fn files_to_process(&self) -> &Vec<FileToProcess> {
        &self.files_to_process
    }

    /// Returns reference to hashmap containing document protection information.
    pub fn protection(&self) -> &HashMap<String, String> {
        &self.protection
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

    /// Returns reference to workbook relationships
    pub fn relationships(&self) -> &Vec<Relationship> {
        &self.relationships
    }
}

impl<R: Read + Seek> SharedStrings<R> {
    pub(crate) fn open(
        archive: &Rc<Archive<R>>,
        path: &str,
        cache_limit: u64,
    ) -> Result<SharedStrings<R>, OoxmlError> {
        let mut entry = archive.find_entry(path, true)?;
        let file_size = entry.seek(io::SeekFrom::End(0))?;
        entry.seek(io::SeekFrom::Start(0))?;

        let mut parser = EventReader::new(entry);

        let event = match parser.next()? {
            XmlEvent::StartDocument { .. } => parser.next()?,
            event => event,
        };

        match event {
            XmlEvent::StartElement { name, .. } if name.local_name.as_str() == "sst" => {}
            _ => return Err("expecting: StartElement <sst>".into()),
        }
        let cache_entries = file_size < cache_limit;
        let mut entries = Vec::<SharedStringEntry>::new();
        loop {
            let offset = parser.position()?;
            let xml_event = parser.next()?;
            match &xml_event {
                XmlEvent::EndElement { name } if name.local_name.as_str() == "sst" => break,
                XmlEvent::EndDocument => return Err("Unexpected end of document".into()),
                _ => {}
            }

            match &xml_event {
                XmlEvent::StartElement { name, .. } if name.local_name.as_str() == "si" => {
                    if cache_entries {
                        let str = SharedStrings::<R>::extract_text(&mut parser, true)?;
                        entries.push(SharedStringEntry::Cached(str));
                    } else {
                        parser.skip()?;
                        let end_offset = parser.position()?;
                        let size = end_offset.saturating_sub(offset).try_into()?;
                        entries.push(SharedStringEntry::NotCached { offset, size });
                    }
                }
                _ => {}
            }
        }
        Ok(SharedStrings {
            archive: archive.clone(),
            path: path.to_string(),
            entries,
        })
    }

    pub(crate) fn get(&self, index: usize) -> Result<String, OoxmlError> {
        match self.entries.get(index) {
            Some(SharedStringEntry::NotCached { offset, size }) => {
                let mut reader = self.open_offset_reader(*offset, *size)?;
                Ok(SharedStrings::<R>::extract_text(&mut reader, false)?)
            }
            Some(SharedStringEntry::Cached(str)) => Ok(str.to_string()),
            None => Err("Invalid index".into()),
        }
    }

    fn extract_text<T: Read + BufRead>(
        reader: &mut EventReader<T>,
        inside_si: bool,
    ) -> Result<String, OoxmlError> {
        let mut result = String::new();
        if !inside_si {
            loop {
                match reader.next()? {
                    XmlEvent::StartDocument { .. } => {}
                    XmlEvent::StartElement { name, .. } if name.local_name.as_str() == "si" => {
                        break
                    }
                    event => return Err(format!("Unexpected xml event: {event:?}").into()),
                }
            }
        }
        let mut in_text_node = false;
        let mut preserve_spaces = false;
        loop {
            match reader.next()? {
                XmlEvent::EndDocument => return Err("Unexpected end of xml document".into()),
                XmlEvent::StartElement {
                    name, attributes, ..
                } if name.local_name.as_str() == "t" => {
                    if in_text_node {
                        return Err("Nested t node detected!!!".into());
                    }
                    in_text_node = true;
                    for attribute in attributes {
                        if attribute.name.local_name.as_str() == "space"
                            && attribute.value.as_str() == "preserve"
                        {
                            preserve_spaces = true;
                        }
                    }
                }
                XmlEvent::StartElement { .. } => reader.skip()?,
                XmlEvent::EndElement { name } => match name.local_name.as_str() {
                    "si" => break,
                    "t" => {
                        in_text_node = false;
                        preserve_spaces = false;
                    }
                    _ => {}
                },
                XmlEvent::Characters(s) => {
                    if in_text_node {
                        result.push_str(&s);
                    }
                }
                XmlEvent::Whitespace(s) => {
                    if preserve_spaces {
                        result.push_str(&s);
                    }
                }
                _ => {}
            }
        }
        Ok(result)
    }

    fn open_offset_reader(
        &self,
        offset: u64,
        size: usize,
    ) -> Result<EventReader<BufReader<OffsetReader<Entry>>>, OoxmlError> {
        let entry = self.archive.find_entry(&self.path, true)?;
        let size = size.try_into()?;
        let reader = OffsetReader::<Entry>::new(entry, &[OffsetReaderChunk { offset, size }])?;
        // let mut config = ParserConfig2::default();
        // config.override_encoding =
        //     Some(Encoding::from_str(&self.encoding).unwrap_or(Encoding::Utf8));
        //Ok(EventReader::new_with_config(reader, config))
        Ok(EventReader::new(BufReader::new(reader)))
    }
}

#[derive(Clone)]
struct OffsetReaderChunk {
    offset: u64,
    size: u64,
}

struct OffsetReader<R: Read + Seek> {
    reader: R,
    chunks: VecDeque<OffsetReaderChunk>,
    current_offset: Option<u64>,
}

impl<R: Read + Seek> OffsetReader<R> {
    fn new(mut reader: R, chunks: &[OffsetReaderChunk]) -> Result<Self, OoxmlError> {
        if chunks.is_empty() {
            return Err("Chunks list is empty".into());
        }
        let current_offset = Some(reader.seek(io::SeekFrom::Start(chunks[0].offset))?);
        let chunks = VecDeque::from_iter(chunks.to_owned());
        Ok(OffsetReader {
            reader,
            current_offset,
            chunks,
        })
    }
}

fn map_tryfrominterror(e: TryFromIntError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

impl<R: Read + Seek> Read for OffsetReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        let mut result: usize = 0;
        let mut output = buf;
        loop {
            let front = match self.chunks.front() {
                Some(front) => front,
                None => return Ok(result),
            };

            let current_offset = match self.current_offset {
                Some(current_offset) => current_offset,
                None => {
                    self.current_offset =
                        Some(self.reader.seek(io::SeekFrom::Start(front.offset))?);
                    self.current_offset.unwrap()
                }
            };

            let remaining_size = front.offset + front.size - current_offset;
            if remaining_size == 0 {
                self.chunks.pop_front();
                self.current_offset = None;
                continue;
            }

            let to_read = remaining_size
                .umin(output.len())
                .try_into()
                .map_err(map_tryfrominterror)?;
            let buf = &mut output[0..to_read];
            let br = self.reader.read(buf)?;
            result += br;
            self.current_offset =
                Some(current_offset + u64::try_from(br).map_err(map_tryfrominterror)?);

            if br <= to_read {
                break;
            }

            output = &mut output[to_read..];
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Cell {
    row: u32,
    column: u32,
}

impl Cell {
    fn from_cell_ref(cell_ref: &str) -> Result<Self, OoxmlError> {
        let index = match cell_ref.find(char::is_numeric) {
            Some(index) if index > 0 => index,
            _ => return Err(format!("Invalid argument: {cell_ref}").into()),
        };
        let column_str = &cell_ref[0..index];
        let row_str = &cell_ref[index..];

        let mut column: u32 = 0;
        for c in column_str.chars() {
            if !c.is_ascii_alphabetic() || !c.is_uppercase() {
                return Err(format!("Invalid argument: {cell_ref}").into());
            }
            let val = c as u32 - 'A' as u32 + 1;
            column *= 26;
            column += val;
        }
        let row = u32::from_str(row_str)?;
        Ok(Cell { row, column })
    }

    fn to_cell_ref(&self) -> Result<String, OoxmlError> {
        if self.row < 1 || self.column < 1 {
            return Err(format!("Invalid properties: {self:?}").into());
        }
        let mut column_str = String::new();
        let mut column = self.column;
        loop {
            column -= 1;
            let c = b'A' + (column % 26) as u8;
            column_str.push(c.into());
            column /= 26;
            if column == 0 {
                break;
            }
        }
        let result = format!(
            "{}{}",
            column_str.chars().rev().collect::<String>(),
            self.row
        );
        Ok(result)
    }
}

#[derive(Debug)]
pub(crate) struct RowInfo {
    pub(crate) offset: u64,
    pub(crate) size: u32,
    pub(crate) index: u32,
    pub(crate) columns: Vec<u32>,
}

impl<R: Read + Seek> Sheet<R> {
    pub(crate) fn new(workbook: &Workbook<R>, info: &SheetInfo) -> Result<Self, OoxmlError> {
        let archive = workbook.archive.clone();

        let relationships =
            (Relationship::load_relationships_for(&archive, &info.path)?).unwrap_or_default();

        let entry = archive.find_entry(&info.path, true)?;
        let parser = EventReader::new(entry);

        Ok(Sheet {
            archive,
            path: info.path.to_string(),
            sheet_info: info.clone(),
            relationships,
            parser,
            parser_state: ParserState::Begin,
            shared_strings: workbook.shared_strings.clone(),
            chunk_start: None,
            chunk_end: None,
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
        let mut row_number: Option<i32> = None;
        let mut stack = Vec::<String>::new();
        let mut rows = Vec::<RowInfo>::new();

        loop {
            let offset = self.parser.position()?;
            match self.next()? {
                XmlEvent::StartElement {
                    name, attributes, ..
                } => {
                    let name = name.local_name;
                    debug!(
                        "{:>spaces$}<{name}{attrs}>",
                        "",
                        spaces = stack.len() * 2,
                        attrs = attributes.iter().fold(String::new(), |s, a| format!(
                            "{s} {}={}",
                            &a.name.local_name, &a.value
                        ))
                    );
                    if name.as_str() == "row" {
                        let row = self.process_row(
                            &mut row_number,
                            offset,
                            attributes,
                            &mut processing_summary.num_cells_detected,
                        )?;
                        rows.push(row);
                    } else {
                        match name.as_str() {
                            "drawing" | "legacyDrawing" | "oleObject" | "objectPr" => {
                                let mut rel_id: Option<String> = None;
                                for attribute in attributes {
                                    if attribute.name.local_name.as_str() == "id" {
                                        rel_id = Some(attribute.value);
                                        break;
                                    }
                                }
                                if let Some(rel_id) = rel_id {
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
                            "sheetProtection" => {
                                for attribute in attributes {
                                    let key =
                                        attribute.name.local_name.as_str().to_case(Case::Snake);
                                    processing_summary.protection.insert(key, attribute.value);
                                }
                            }
                            _ => {}
                        }
                        stack.push(name);
                    }
                }
                XmlEvent::EndElement { name } => {
                    let name = name.local_name;
                    stack.pop();
                    debug!("{:spaces$}</{name}>", "", spaces = stack.len() * 2);
                }
                XmlEvent::EndDocument => {
                    let end_offset = self.parser.position()?;
                    let size = end_offset - offset;
                    self.chunk_end = Some(OffsetReaderChunk { offset, size });
                    break;
                }
                _ => {}
            }
        }

        let regions = find_regions(&rows);
        let mut row_reader = RowReader::new(
            self.archive.clone(),
            self.path.clone(),
            rows,
            self.chunk_start.clone().unwrap(),
            self.chunk_end.clone().unwrap(),
            self.shared_strings.clone(),
        );

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

    fn process_row(
        &mut self,
        last_row: &mut Option<i32>,
        offset: u64,
        attributes: Vec<OwnedAttribute>,
        num_cells_detected: &mut u64,
    ) -> Result<RowInfo, OoxmlError> {
        let mut row_number: Option<i32> = None;
        let mut columns = Vec::<u32>::new();

        for a in attributes {
            if a.name.local_name == "r" {
                row_number = Some(a.value.parse()?);
            }
        }
        if row_number.is_none() {
            return Err("Missing r attribute".into());
        }
        let row_number = row_number.unwrap();
        if let Some(last_row) = last_row {
            if *last_row >= row_number {
                return Err("Invalid row number".into());
            }
        }
        *last_row = Some(row_number);
        let mut last_cell: Option<Cell> = None;

        loop {
            match self.next()? {
                XmlEvent::StartElement {
                    name, attributes, ..
                } => match name.local_name.as_str() {
                    "c" => {
                        let mut cell_ref: Option<String> = None;
                        for a in attributes {
                            if a.name.local_name.as_str() == "r" {
                                cell_ref = Some(a.value);
                            }
                        }
                        if cell_ref.is_none() {
                            return Err("Missing r argument in cell".into());
                        }
                        let cell_ref = cell_ref.unwrap();
                        let cell = Cell::from_cell_ref(&cell_ref)?;
                        if Some(cell.row) != u32::try_from(row_number).ok() {
                            return Err("Cell from another row detected".into());
                        }
                        if let Some(last_cell) = &last_cell {
                            if last_cell.column >= cell.column {
                                return Err("Invalid column".into());
                            }
                        }
                        columns.push(cell.column);
                        last_cell = Some(cell);
                        *num_cells_detected += 1;
                    }
                    _ => self.skip()?,
                },
                XmlEvent::EndElement { name } if name.local_name.as_str() == "row" => break,
                XmlEvent::EndDocument => return Err("Unexpected end of document".into()),
                _ => {}
            }
        }

        let end_offset = self.parser.position()?;
        let size = (end_offset.saturating_sub(offset)).try_into()?;

        Ok(RowInfo {
            offset,
            size,
            index: row_number.try_into()?,
            //hidden,
            columns,
        })
    }

    fn skip(&mut self) -> Result<(), OoxmlError> {
        match &self.parser_state {
            ParserState::Begin => Err("Cannot skip on Begin".into()),
            ParserState::End => Err("Cannot skip on End".into()),
            ParserState::Error(e) => Err(e.into()),
            ParserState::XmlError(e) => Err(e.clone().into()),
            ParserState::Middle(depth) => {
                if *depth == 0 {
                    unreachable!("This should not happen");
                };
                self.parser.skip()?;
                self.parser_state = ParserState::Middle(depth - 1);
                Ok(())
            }
        }
    }

    fn next(&mut self) -> Result<XmlEvent, OoxmlError> {
        loop {
            match &self.parser_state {
                ParserState::End => return Ok(XmlEvent::EndDocument),
                ParserState::Error(err) => return Err(err.into()),
                ParserState::XmlError(err) => return Err(err.clone().into()),
                ParserState::Begin => {
                    let parser = &mut self.parser;
                    let evt = parser.next().inspect_err(|e| {
                        self.parser_state = ParserState::XmlError(e.clone());
                    })?;
                    let evt = match evt {
                        XmlEvent::StartDocument { .. } => parser.next().inspect_err(|e| {
                            self.parser_state = ParserState::XmlError(e.clone());
                        })?,
                        event => event,
                    };

                    match evt {
                        XmlEvent::StartElement { name, .. }
                            if name.local_name == self.sheet_info.sheet_type.name() =>
                        {
                            let offset: u64 = 0;
                            let size = parser.position()?;
                            self.chunk_start = Some(OffsetReaderChunk { offset, size })
                        }
                        _ => {
                            let err = format!(
                                "expecting: StartElement <{}>",
                                self.sheet_info.sheet_type.name()
                            );
                            self.parser_state = ParserState::Error(err.to_string());
                            return Err(err.into());
                        }
                    }
                    self.parser_state = ParserState::Middle(0);
                }
                ParserState::Middle(depth) => {
                    let event = self.parser.next();
                    match &event {
                        Err(xml_error) => {
                            self.parser_state = ParserState::XmlError(xml_error.clone());
                            return Err(xml_error.clone().into());
                        }
                        Ok(XmlEvent::StartElement { name, .. }) => {
                            let name = &name.local_name;
                            let skip_event = matches!(
                                name.as_str(),
                                "pageMargins"
                                    | "pageSetup"
                                    | "sheetViews"
                                    | "sheetFormatPr"
                                    | "anchor"
                                    | "sheetPr"
                                    | "printOptions"
                                    | "headerFooter"
                            );
                            if skip_event {
                                if let Err(xml_error) = self.parser.skip() {
                                    self.parser_state = ParserState::XmlError(xml_error.clone());
                                    return Err(xml_error.clone().into());
                                }
                                continue;
                            }
                            self.parser_state = ParserState::Middle(depth + 1);
                        }
                        Ok(XmlEvent::EndElement { .. }) => {
                            if *depth == 0 {
                                self.parser_state = ParserState::End;
                                return Ok(XmlEvent::EndDocument);
                            };
                            self.parser_state = ParserState::Middle(depth - 1);
                        }
                        Ok(_) => {}
                    }
                    return Ok(event.unwrap());
                }
            }
        }
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

pub(crate) fn find_regions(rows: &Vec<RowInfo>) -> Vec<Region> {
    let mut result = Vec::<Region>::new();
    if rows.is_empty() {
        return result;
    }
    let mut current_regions = LinkedList::<Region>::new();
    let mut last_row: Option<u32> = None;

    for row in rows {
        // First find continous areas in row
        let mut current_line_regions = LinkedList::<Region>::new();
        for column in &row.columns {
            if let Some(region) = current_line_regions.back_mut() {
                if region.right + 1 == *column {
                    // Resize last region
                    region.right = *column;
                    continue;
                }
            }
            // Create new region
            current_line_regions.push_back(Region::from_cell(Cell {
                row: row.index,
                column: *column,
            }));
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

pub struct SheetIterator<'a, R: Read + Seek> {
    workbook: &'a Workbook<R>,
    index: usize,
}

impl<'a, R: Read + Seek> Iterator for SheetIterator<'a, R> {
    type Item = Sheet<R>;

    fn next(&mut self) -> Option<Self::Item> {
        let sheet_info = self.workbook.sheets.get(self.index)?;
        let sheet = Sheet::new(self.workbook, sheet_info).ok()?;
        self.index += 1;
        Some(sheet)
    }
}

pub(crate) struct Region {
    pub(crate) top: u32,
    pub(crate) bottom: u32,
    pub(crate) left: u32,
    pub(crate) right: u32,
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

    fn from_cell(cell: Cell) -> Self {
        Region {
            top: cell.row,
            bottom: cell.row,
            left: cell.column,
            right: cell.column,
        }
    }
}

impl Debug for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = Cell {
            row: self.top,
            column: self.left,
        }
        .to_cell_ref()
        .unwrap_or_else(|_| "??".to_string());
        let y = Cell {
            row: self.bottom,
            column: self.right,
        }
        .to_cell_ref()
        .unwrap_or_else(|_| "??".to_string());
        write!(f, "Region {x}-{y}")
    }
}

struct RowReaderCache<R: Read + Seek> {
    shared_strings: Option<Rc<SharedStrings<R>>>,
    parser: EventReader<BufReader<OffsetReader<Entry>>>,
    row: u32,
    column: Option<(u32, CellType)>,
}

struct RowReader<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    path: String,
    chunk_start: OffsetReaderChunk,
    chunk_end: OffsetReaderChunk,
    rows: Vec<RowInfo>,
    shared_strings: Option<Rc<SharedStrings<R>>>,
    cache: Option<RowReaderCache<R>>,
}

#[derive(Debug, Clone)]
enum CellType {
    Number,
    Boolean,
    Date,
    Error,
    InlineString,
    SharedString,
    Formula,
}

impl<R: Read + Seek> RowReader<R> {
    fn new(
        archive: Rc<Archive<R>>,
        path: String,
        rows: Vec<RowInfo>,
        chunk_start: OffsetReaderChunk,
        chunk_end: OffsetReaderChunk,
        shared_strings: Option<Rc<SharedStrings<R>>>,
    ) -> Self {
        RowReader {
            cache: None,
            archive,
            path,
            rows,
            chunk_start,
            chunk_end,
            shared_strings,
        }
    }

    fn get_cell_value(&mut self, row: u32, column: u32) -> Result<Option<String>, OoxmlError> {
        let index = match self
            .rows
            .binary_search_by(|row_info| row_info.index.cmp(&row))
        {
            Ok(index) => index,
            Err(_) => return Ok(None),
        };
        let row_info = self.rows.get(index).unwrap();
        if row_info.columns.binary_search(&column).is_err() {
            return Ok(None);
        }

        if let Some(cache) = &self.cache {
            if cache.row != row
                || cache.column.is_none()
                || cache.column.as_ref().unwrap().0 > column
            {
                self.cache = None;
            }
        }
        if self.cache.is_none() {
            let row_reader = RowReaderCache::new(self, row_info, row)?;
            self.cache = Some(row_reader);
        }

        let cache = self.cache.as_mut().unwrap();
        cache.get_column_value(column)
    }
}

impl<R: Read + Seek> RowReaderCache<R> {
    fn new(row_reader: &RowReader<R>, row_info: &RowInfo, row: u32) -> Result<Self, OoxmlError> {
        let mut entry = row_reader.archive.find_entry(&row_reader.path, true)?;
        let mut chunks = [
            row_reader.chunk_start.clone(),
            OffsetReaderChunk {
                offset: row_info.offset,
                size: row_info.size.into(),
            },
            row_reader.chunk_end.clone(),
        ];

        let mut buf = [0u8; 3];
        entry.read_exact(&mut buf)?;
        if buf == [0xef, 0xbb, 0xbf] {
            for chunk in &mut chunks {
                chunk.offset += 3;
            }
        }

        let reader = OffsetReader::<Entry>::new(entry, &chunks)?;
        let mut parser = EventReader::new(BufReader::new(reader));

        let event = match parser.next()? {
            XmlEvent::StartDocument { .. } => parser.next()?,
            event => event,
        };
        match event {
            XmlEvent::StartElement { name, .. }
                if ["worksheet", "macrosheet", "dialogsheet", "chartsheet"]
                    .contains(&name.local_name.as_str()) => {}
            _ => return Err(
                "expecting: StartElement <worksheet>, <macrosheet>, <dialogsheet> or <chartsheet>"
                    .into(),
            ),
        }
        match parser.next()? {
            XmlEvent::StartElement { name, .. } if name.local_name == "row" => {}
            _ => return Err("expecting: StartElement <row>".into()),
        }

        let column: Option<(u32, CellType)> = RowReaderCache::<R>::next_cell(&mut parser)?;
        Ok(RowReaderCache {
            shared_strings: row_reader.shared_strings.clone(),
            parser,
            row,
            column,
        })
    }

    fn next_cell(
        parser: &mut EventReader<BufReader<OffsetReader<Entry>>>,
    ) -> Result<Option<(u32, CellType)>, OoxmlError> {
        loop {
            match parser.next()? {
                XmlEvent::EndDocument => return Err("Unexpected end of document".into()),
                XmlEvent::EndElement { name } if name.local_name.as_str() == "row" => break,
                XmlEvent::StartElement {
                    name, attributes, ..
                } => {
                    if name.local_name != "c" {
                        parser.skip()?;
                        continue;
                    }
                    let mut cell_ref: Option<String> = None;
                    let mut cell_type: Option<CellType> = None;

                    for attribute in attributes {
                        match attribute.name.local_name.as_str() {
                            "r" => {
                                cell_ref = Some(attribute.value);
                            }
                            "t" => {
                                cell_type = Some(match attribute.value.as_str() {
                                    "b" => CellType::Boolean,
                                    "d" => CellType::Date,
                                    "e" => CellType::Error,
                                    "inlineStr" => CellType::InlineString,
                                    "n" => CellType::Number,
                                    "s" => CellType::SharedString,
                                    "str" => CellType::Formula,
                                    other => return Err(other.into()),
                                });
                            }
                            _ => {}
                        }
                    }
                    if cell_ref.is_none() {
                        return Err("Missing cell reference".into());
                    }
                    let cell = Cell::from_cell_ref(&cell_ref.unwrap())?;
                    let cell_type = cell_type.unwrap_or(CellType::Number);
                    return Ok(Some((cell.column, cell_type)));
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn get_column_value(&mut self, column: u32) -> Result<Option<String>, OoxmlError> {
        loop {
            if self.column.is_none() {
                return Ok(None);
            }
            let current_column = self.column.as_ref().unwrap().0;

            match current_column.cmp(&column) {
                std::cmp::Ordering::Less => self.column = Self::next_cell(&mut self.parser)?,
                std::cmp::Ordering::Equal => break,
                std::cmp::Ordering::Greater => return Ok(None),
            }
        }
        let cell_type = self.column.as_ref().unwrap().1.clone();
        match cell_type {
            CellType::SharedString => {
                let shared_strings = self.shared_strings.borrow();
                if shared_strings.is_none() {
                    return Ok(None);
                }
                let mut v: Option<String> = None;
                let mut inside_v = false;
                loop {
                    match self.parser.next()? {
                        XmlEvent::EndDocument => {
                            return Err("Unexpected end of document".into());
                        }
                        XmlEvent::StartElement { name, .. } => {
                            if name.local_name.as_str() == "v" {
                                inside_v = true;
                            }
                        }
                        XmlEvent::EndElement { name } => match name.local_name.as_str() {
                            "v" => inside_v = false,
                            "c" => break,
                            _ => {}
                        },
                        XmlEvent::Characters(str) if inside_v => {
                            v = Some(str);
                        }
                        _ => {}
                    }
                }
                if v.is_none() {
                    return Ok(None);
                }
                let index = usize::from_str(v.unwrap().as_ref())?;
                let shared_strings = shared_strings.as_ref().unwrap();
                let result = shared_strings.get(index)?;
                Ok(Some(result))
            }
            CellType::InlineString => {
                let mut result = String::new();
                let mut inside_is = false;
                let mut inside_t = false;
                loop {
                    match self.parser.next()? {
                        XmlEvent::EndDocument => {
                            return Err("Unexpected end of document".into());
                        }
                        XmlEvent::StartElement { name, .. } => match name.local_name.as_str() {
                            "is" => inside_is = true,
                            "t" => inside_t = true,
                            _ => {}
                        },
                        XmlEvent::EndElement { name } => match name.local_name.as_str() {
                            "is" => inside_is = false,
                            "t" => inside_t = false,
                            "c" => break,
                            _ => {}
                        },
                        XmlEvent::Characters(str) if inside_is && inside_t => {
                            result.push_str(&str);
                        }
                        _ => {}
                    }
                }
                if result.is_empty() {
                    return Ok(None);
                }
                Ok(Some(result))
            }
            _ => {
                let mut v: Option<String> = None;
                let mut inside_v = false;
                loop {
                    match self.parser.next()? {
                        XmlEvent::EndDocument => {
                            return Err("Unexpected end of document".into());
                        }
                        XmlEvent::StartElement { name, .. } => {
                            if name.local_name.as_str() == "v" {
                                inside_v = true;
                            }
                        }
                        XmlEvent::EndElement { name } => match name.local_name.as_str() {
                            "v" => inside_v = false,
                            "c" => break,
                            _ => {}
                        },
                        XmlEvent::Characters(str) if inside_v => {
                            v = Some(str);
                        }
                        _ => {}
                    }
                }
                Ok(v)
            }
        }
    }
}
