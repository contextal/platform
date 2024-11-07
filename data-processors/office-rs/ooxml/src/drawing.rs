use crate::{
    archive::{self, Archive},
    relationship::{FileToProcess, Relationship, TargetMode},
    xml::{self, reader::OwnedAttribute},
    OoxmlError, ProcessingSummary, RelationshipType,
};
use std::{
    io::{Read, Seek, Write},
    rc::Rc,
};
use tracing::{debug, warn};
use xml::reader::{EventReader, XmlEvent};

pub(crate) struct Drawing {
    parser: EventReader<archive::Entry>,
    relationships: Vec<Relationship>,
}

impl Drawing {
    pub(crate) fn open<R: Read + Seek>(
        archive: &Rc<Archive<R>>,
        path: &str,
    ) -> Result<Drawing, OoxmlError> {
        let relationships =
            (Relationship::load_relationships_for(archive, path)?).unwrap_or_default();
        let entry = archive.find_entry(path, true)?;
        let parser = EventReader::new(entry);
        Ok(Drawing {
            parser,
            relationships,
        })
    }

    pub(crate) fn process<W: Write>(
        &mut self,
        writer: &mut W,
        result: &mut ProcessingSummary,
    ) -> Result<(), OoxmlError> {
        let mut stack = Vec::<String>::new();
        let mut preserve_spaces = false;
        let mut new_line_required = false;

        loop {
            let e = self.parser.next();
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
                        "externalData" | "hlinkClick" => {
                            if let Some(value) = find_attribute(&attributes, "id") {
                                self.process_realationship(result, value);
                            }
                        }
                        "blip" => {
                            if let Some(value) = find_attribute(&attributes, "embed") {
                                self.process_realationship(result, value);
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
                    if stack.last().map(String::as_str) == Some("t") {
                        let bytes = match preserve_spaces {
                            true => s.into_bytes(),
                            false => s.trim().to_string().into_bytes(),
                        };
                        writer.write_all(&bytes)?
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

    fn find_relationship(&self, id: &str) -> Option<&Relationship> {
        self.relationships
            .iter()
            .find(|&relationship| relationship.id == id)
    }

    fn process_realationship(&self, result: &mut ProcessingSummary, relation_id: &str) {
        if let Some(rel) = self.find_relationship(relation_id) {
            match &rel.target {
                TargetMode::Internal(path) => {
                    result.files_to_process.push(FileToProcess {
                        path: path.clone(),
                        rel_type: rel.rel_type.clone(),
                    });
                }
                TargetMode::External(path) => {
                    if matches!(rel.rel_type, RelationshipType::Hyperlink) {
                        if !result.hyperlinks.contains(path) {
                            result.hyperlinks.push(path.clone())
                        }
                    } else if !result.external_resources.contains(path) {
                        result.external_resources.push(path.clone())
                    };
                }
            };
        }
    }
}

fn find_attribute<'a>(attributes: &'a [OwnedAttribute], key: &str) -> Option<&'a str> {
    for attribute in attributes {
        if attribute.name.local_name == key {
            return Some(&attribute.value);
        }
    }
    None
}
