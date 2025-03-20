use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{self, BufRead, Write};
use std::{
    io::{Read, Seek},
    rc::Rc,
};
use tracing::debug;

use crate::archive::Archive;
use crate::error::OdfError;
use crate::manifest::Manifest;
use crate::meta::{Meta, UserProperty};
use crate::xml::{Event, Xml, XmlEventIterator};

pub struct Odf<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    pub manifest: Manifest,
    pub document_type: DocumentType,
    pub properties: HashMap<String, String>,
    pub user_properties: Vec<UserProperty>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum DocumentType {
    Text,
    Spreadsheet,
}

impl DocumentType {
    fn from_mimetype(mimetype: &str) -> Result<DocumentType, OdfError> {
        let result = match mimetype {
            "application/vnd.oasis.opendocument.text"
            | "application/vnd.oasis.opendocument.text-template"
            | "application/vnd.oasis.opendocument.text-master"
            | "application/vnd.oasis.opendocument.text-master-template" => DocumentType::Text,
            "application/vnd.oasis.opendocument.spreadsheet"
            | "application/vnd.oasis.opendocument.spreadsheet-template" => {
                DocumentType::Spreadsheet
            }
            mimetype => return Err(format!("Unsupported mimetype '{mimetype}'").into()),
        };
        Ok(result)
    }
}

impl<R: Read + Seek> Debug for Odf<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ODF")
            .field("document_type", &self.document_type)
            .field("manifest", &self.manifest)
            .finish()
    }
}

#[derive(Debug, Default)]
pub struct ProcessingSummary {
    pub hyperlinks: Vec<String>,
    pub images: Vec<String>,
}

impl<R: Read + Seek> Odf<R> {
    pub fn new(reader: R) -> Result<Self, OdfError> {
        let archive = Rc::new(Archive::new(reader)?);

        let mut entry = archive.find_entry("mimetype", false)?;
        let mut mimetype = String::new();
        entry.read_to_string(&mut mimetype)?;
        let document_type = DocumentType::from_mimetype(&mimetype)?;

        let entry = archive.find_entry("META-INF/manifest.xml", true)?;
        let manifest = Manifest::load_from_xml_parser(entry)?;

        let properties;
        let user_properties;

        if let Some(meta) = Self::load_meta(&archive) {
            properties = meta.properties;
            user_properties = meta.user_properties;
        } else {
            properties = HashMap::new();
            user_properties = Vec::new();
        }

        Ok(Self {
            archive,
            document_type,
            manifest,
            properties,
            user_properties,
        })
    }

    fn load_meta(archive: &Archive<R>) -> Option<Meta> {
        if !archive.contains("meta.xml") {
            return None;
        }
        let entry = archive.find_entry("meta.xml", true).ok()?;
        Meta::load_from_xml_parser(entry).ok()
    }

    pub fn process<W: Write>(
        &self,
        writer: &mut W,
        processing_summary: &mut ProcessingSummary,
    ) -> Result<(), OdfError> {
        if self.document_type == DocumentType::Spreadsheet {
            return Err("ODS format is not yet supported".into());
        }

        let entry = self.archive.find_entry("content.xml", true)?;

        let xml = Xml::new(entry);
        let mut iter = xml
            .into_iterator()
            .enter_xml_path(&["document-content", "body", "text"])?;
        process_text(&mut iter, writer, processing_summary)
    }

    /// Extracts embedded file to specified writer
    ///
    /// Returns false if archive does not contain entry
    pub fn extract_file_to_writer<W: Write>(
        &self,
        from: &str,
        to: &mut W,
    ) -> Result<bool, OdfError> {
        if !self.archive.contains(from) {
            return Ok(false);
        }
        let mut entry = self.archive.find_entry(from, false)?;
        io::copy(&mut entry, to)?;
        Ok(true)
    }
}

fn process_text<R: Read + BufRead, W: Write>(
    iter: &mut XmlEventIterator<R>,
    writer: &mut W,
    processing_summary: &mut ProcessingSummary,
) -> Result<(), OdfError> {
    let mut p_count = 0;
    let mut add_leading_space = false;
    while let Some(event) = iter.next()? {
        match event {
            Event::StartElement(start_element) => match start_element.name.as_str() {
                "p" | "h" => {
                    p_count += 1;
                }
                "image" => {
                    let image = start_element.attributes.iter().find_map(|attr| {
                        if attr.key == "href" {
                            Some(attr.value.to_string())
                        } else {
                            None
                        }
                    });
                    if let Some(image) = image {
                        processing_summary.images.push(image);
                    }
                }
                "a" => {
                    let hyperlink = start_element.attributes.iter().find_map(|attr| {
                        if attr.key == "href" {
                            Some(attr.value.to_string())
                        } else {
                            None
                        }
                    });
                    if let Some(image) = hyperlink {
                        processing_summary.hyperlinks.push(image);
                    }
                }
                "line-break" => {
                    add_leading_space = false;
                    writer.write_all(b"\n")?;
                }
                _ => debug!("Ignoring {start_element:?}"),
            },
            Event::EndElement(name) => match name.as_str() {
                "p" | "h" => {
                    writer.write_all(b"\n")?;
                    p_count -= 1;
                    add_leading_space = false;
                }
                "span" => {
                    add_leading_space = true;
                }
                _ => {}
            },
            Event::Text(text) => {
                if p_count > 0 {
                    if add_leading_space {
                        writer.write_all(b" ")?;
                        add_leading_space = false;
                    }
                    writer.write_all(text.as_bytes())?;
                } else {
                    debug!("Ignoring {text}")
                }
            }
        }
    }
    Ok(())
}
