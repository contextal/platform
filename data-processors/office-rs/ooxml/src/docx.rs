use crate::{
    archive::{self, Archive},
    drawing::Drawing,
    relationship::{FileToProcess, Relationship, RelationshipType, TargetMode},
    xml, OoxmlError, ParserState, ProcessingSummary,
};
use convert_case::{Case, Casing};
use std::{
    collections::HashMap,
    io::{Read, Seek, Write},
    rc::Rc,
};
use tracing::{debug, warn};
use xml::reader::{EventReader, XmlEvent};

/// The parser for Word files
pub struct Wordprocessing<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    parser: EventReader<archive::Entry>,
    parser_state: ParserState,
    relationships: Vec<Relationship>,
    protection: HashMap<String, String>,
    path: String,
}

impl<R: Read + Seek> Wordprocessing<R> {
    /// Returns reference to document relationships
    pub fn relationships(&self) -> &Vec<Relationship> {
        &self.relationships
    }

    pub(crate) fn open(
        archive: &Rc<Archive<R>>,
        path: &str,
    ) -> Result<Wordprocessing<R>, OoxmlError> {
        let relationships =
            (Relationship::load_relationships_for(archive, path)?).unwrap_or_default();
        let protection = Wordprocessing::load_document_protection(archive, &relationships)?;
        let entry = archive.find_entry(path, true)?;
        let parser = EventReader::new(entry);
        Ok(Wordprocessing {
            archive: Rc::clone(archive),
            parser,
            parser_state: ParserState::Begin,
            relationships,
            protection,
            path: path.to_owned(),
        })
    }

    /// Parses main document. Document content is extracted to writer argument.
    /// Returns ProessingSummary struct containing list of files referenced in document and list of hyperlinks.
    pub fn process<W: Write>(
        &mut self,
        writer: &mut W,
        result: &mut ProcessingSummary,
    ) -> Result<(), OoxmlError> {
        let mut stack = Vec::<String>::new();
        let mut preserve_spaces = false;
        let mut new_line_required = false;

        loop {
            let e = self.next();
            match e {
                Ok(XmlEvent::StartElement {
                    name, attributes, ..
                }) => {
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
                    match name.as_str() {
                        "p" => {
                            if new_line_required {
                                writer.write_all(b"\n")?;
                                new_line_required = false;
                            }
                        }
                        "t" => {
                            for a in &attributes {
                                if a.name.local_name == "space" && a.value == "preserve" {
                                    preserve_spaces = true;
                                }
                            }
                        }
                        "blip" => {
                            for attribute in attributes {
                                let attr_name = attribute.name.local_name.as_str();
                                if attr_name == "link" || attr_name == "embed" {
                                    if let Some(rel) = self.find_relationship(&attribute.value) {
                                        match attr_name {
                                            "link" => {
                                                if let TargetMode::External(target) = &rel.target {
                                                    result.hyperlinks.push(target.clone());
                                                }
                                            }
                                            "embed" => {
                                                if let TargetMode::Internal(target) = &rel.target {
                                                    result.files_to_process.push(FileToProcess {
                                                        path: target.clone(),
                                                        rel_type: rel.rel_type.clone(),
                                                    });
                                                }
                                            }
                                            _ => unreachable!(),
                                        }
                                    }
                                }
                            }
                        }
                        "imagedata" | "OLEObject" => {
                            for attribute in attributes {
                                if attribute.name.local_name.as_str() == "id" {
                                    if let Some(rel) = self.find_relationship(&attribute.value) {
                                        match &rel.target {
                                            TargetMode::Internal(target) => {
                                                result.files_to_process.push(FileToProcess {
                                                    path: target.clone(),
                                                    rel_type: rel.rel_type.clone(),
                                                });
                                            }
                                            TargetMode::External(_) => {}
                                        }
                                    }
                                }
                            }
                        }
                        "chart" => {
                            for attribute in attributes {
                                if attribute.name.local_name.as_str() == "id" {
                                    match self.find_relationship(&attribute.value) {
                                        Some(rel) if rel.rel_type == RelationshipType::Chart => {
                                            match &rel.target {
                                                TargetMode::Internal(target) => {
                                                    let mut diagram_data =
                                                        Drawing::open(&self.archive, target)?;
                                                    diagram_data.process(writer, result)?;
                                                }
                                                TargetMode::External(_) => {}
                                            }
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        "relIds" => {
                            for attribute in attributes {
                                if attribute.name.local_name.as_str() == "dm" {
                                    match self.find_relationship(&attribute.value) {
                                        Some(rel)
                                            if rel.rel_type == RelationshipType::DiagramData =>
                                        {
                                            match &rel.target {
                                                TargetMode::Internal(target) => {
                                                    let mut diagram_data =
                                                        Drawing::open(&self.archive, target)?;
                                                    diagram_data.process(writer, result)?;
                                                }
                                                TargetMode::External(_) => {}
                                            }

                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        "hyperlink" => {
                            for attribute in attributes {
                                if attribute.name.local_name.as_str() == "id" {
                                    if let Some(rel) = self.find_relationship(&attribute.value) {
                                        if let TargetMode::External(target) = &rel.target {
                                            result.hyperlinks.push(target.clone());
                                        }
                                    }
                                }
                            }
                        }

                        _ => {}
                    }
                    stack.push(name);
                }
                Ok(XmlEvent::EndElement { name }) => {
                    let name = name.local_name;
                    stack.pop();
                    match name.as_str() {
                        "p" => new_line_required = true,
                        "t" => preserve_spaces = false,
                        "br" => writer.write_all(b"\n")?,
                        "tab" => writer.write_all(b"\t")?,
                        _ => {}
                    }
                    debug!("{:spaces$}</{name}>", "", spaces = stack.len() * 2);
                }
                Ok(XmlEvent::Whitespace(s)) => {
                    debug!("{:spaces$}@Whitespace({s})", "", spaces = stack.len() * 2);
                    if stack.last().map(String::as_str) == Some("t") && preserve_spaces {
                        let bytes = s.into_bytes();
                        writer.write_all(&bytes)?
                    }
                }
                Ok(XmlEvent::Characters(s)) => {
                    debug!("{:spaces$}@Characters({s})", "", spaces = stack.len() * 2);
                    match stack.last().map(String::as_str) {
                        Some("t") => {
                            let bytes = match preserve_spaces {
                                true => s.into_bytes(),
                                false => s.trim().to_string().into_bytes(),
                            };
                            writer.write_all(&bytes)?
                        }
                        Some("instrText") => {
                            let s = s.as_str().trim_start();
                            if let Some(s) = s.strip_prefix("HYPERLINK ") {
                                let s = s.trim_start();
                                if let Some(s) = s.strip_prefix('"') {
                                    if let Some(index) = s.find('"') {
                                        let s = &s[0..index];
                                        result.hyperlinks.push(s.to_string());
                                    }
                                } else {
                                    let parts: Vec<&str> = s.split(' ').collect();
                                    if Some(&s) == parts.first() {
                                        result.hyperlinks.push(s.to_string());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(XmlEvent::EndDocument) => break,
                Ok(e) => {
                    debug!("{:spaces$}IGNORED => {e:?}", "", spaces = stack.len() * 2);
                }
                Err(e) => {
                    warn!("Error: {e}");
                    break;
                }
            }
        }
        Ok(())
    }

    fn next(&mut self) -> Result<XmlEvent, OoxmlError> {
        loop {
            match &self.parser_state {
                ParserState::Begin => {
                    let evt = self.parser.next().inspect_err(|e| {
                        self.parser_state = ParserState::XmlError(e.clone());
                    })?;
                    let evt = match evt {
                        XmlEvent::StartDocument { .. } => self.parser.next().inspect_err(|e| {
                            self.parser_state = ParserState::XmlError(e.clone());
                        })?,
                        evt => evt,
                    };

                    match evt {
                        XmlEvent::StartElement { name, .. } if name.local_name == "document" => {}
                        _ => return Err("expecting: StartElement <document>".into()),
                    }
                    loop {
                        let evt = self.parser.next().inspect_err(|e| {
                            self.parser_state = ParserState::XmlError(e.clone());
                        })?;
                        match evt {
                            XmlEvent::StartElement { name, .. } if name.local_name == "body" => {
                                break
                            }
                            XmlEvent::StartElement { .. } => {
                                self.parser.skip()?;
                            }
                            XmlEvent::EndElement { .. } | XmlEvent::EndDocument => {
                                return Err("Element <body> not found".into())
                            }
                            _ => {}
                        }
                    }
                    self.parser_state = ParserState::Middle(0);
                    continue;
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
                            let skip_event = match name.as_str() {
                                str if str.ends_with("Pr") => true,
                                "tblGrid" => true,
                                _ => false,
                            };
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
                ParserState::End => return Ok(XmlEvent::EndDocument),
                ParserState::Error(err) => return Err(err.into()),
                ParserState::XmlError(err) => return Err(err.clone().into()),
            }
        }
    }

    fn find_relationship(&self, id: &str) -> Option<&Relationship> {
        self.relationships
            .iter()
            .find(|&relationship| relationship.id == id)
    }

    fn load_document_protection(
        archive: &Archive<R>,
        relationships: &[Relationship],
    ) -> Result<HashMap<String, String>, OoxmlError> {
        let mut result = HashMap::<String, String>::new();
        if let Some(relation) =
            Relationship::find_relationship(relationships, RelationshipType::Settings)
        {
            if let TargetMode::Internal(target) = &relation.target {
                let entry = archive.find_entry(target, true)?;
                let mut parser = EventReader::new(entry);
                loop {
                    match parser.next()? {
                        XmlEvent::EndDocument => break,
                        XmlEvent::StartElement {
                            name, attributes, ..
                        } if name.local_name.as_str() == "documentProtection" => {
                            for attribute in attributes {
                                let key = attribute.name.local_name.as_str().to_case(Case::Snake);
                                result.insert(key, attribute.value);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(result)
    }

    /// Returns reference to hashmap containing document protection information.
    pub fn protection(&self) -> &HashMap<String, String> {
        &self.protection
    }

    /// Returns document path inside archive
    pub fn path(&self) -> &str {
        &self.path
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
