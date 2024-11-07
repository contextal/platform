//! # Parser for Microsoft Word and Excel files in Office Open XML format

#![warn(missing_docs)]
mod archive;
mod content_types;
mod docx;
mod drawing;
mod error;
/// Describes relationships between document parts
pub mod relationship;
mod xlsb;
mod xlsx;
mod xml;

use crate::{archive::Entry, content_types::ContentType};
use archive::Archive;
use bitflags::bitflags;
use ctxole::Ole;
use regex::Regex;
use relationship::{FileToProcess, TargetMode};
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Seek, Write},
    path::Path,
    rc::Rc,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::{debug, warn};
use vba::{Vba, VbaDocument};
pub use xlsb::{BinarySheet, BinaryWorkbook};
use xml::reader::{EventReader, XmlEvent};

pub use docx::Wordprocessing;
pub use error::OoxmlError;
pub use relationship::{Relationship, RelationshipType};
pub use xlsx::{Sheet, SheetType, Workbook};

/// This enum contains specific document type (Word/Excel)
pub enum Document<R: Read + Seek> {
    /// Word document in WordprocessingML format
    Docx(Box<Wordprocessing<R>>),
    /// Excel document in ShpreadsheetML format
    Xlsx(Workbook<R>),
    /// Excel document in binary format
    Xlsb(BinaryWorkbook<R>),
}

/// Structure containing main document and its properties
pub struct Ooxml<R: Read + Seek> {
    archive: Rc<Archive<R>>,
    /// Main document
    pub document: Document<R>,
    /// Document properties
    pub properties: DocumentProperties,
}

bitflags! {
    /// Specifies the security level of a document
    #[derive(Debug, Clone)]
    pub struct DocumentSecurity: u8 {
        /// Document is password protected.
        const PASSWORD_PROTECTED = 1;
        /// Document is recommended to be opened as read-only.
        const READ_ONLY_RECOMMENDED = 2;
        /// Document is enforced to be opened as read-only.
        const READ_ONLY_ENFORCED = 4;
        /// Document is locked for annotation.
        const ANNOTATION_LOCKED = 8;
    }
}

impl<R: Read + Seek> Ooxml<R> {
    const CONTENT_TYPES_PATH: &'static str = "[Content_Types].xml";

    /// Opens ooxml file, recognizes its type and loads document properties.
    pub fn new(r: R, shared_strings_cache_limit: u64) -> Result<Ooxml<R>, OoxmlError> {
        let archive = Rc::new(Archive::new(r)?);
        let entry = archive.find_entry(Self::CONTENT_TYPES_PATH, true)?;
        let content_types = ContentType::load_from_xml_parser(entry)?;

        let mut document: Option<Document<R>> = None;
        for content_type in content_types {
            if let ContentType::PartName {
                part_name,
                content_type,
            } = content_type
            {
                debug!("content_type={content_type}");
                match content_type.as_str() {
                    "application/vnd.ms-word.document.macroEnabled.main+xml" |
                    "application/vnd.ms-word.template.macroEnabledTemplate.main+xml" |
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml" => {
                        let path = Self::normalize_path(Self::CONTENT_TYPES_PATH, &part_name)?;
                        let wordprocessing = Wordprocessing::open(&archive, &path)?;
                        document = Some(Document::Docx(Box::new(wordprocessing)));
                        break;
                    }
                    "application/vnd.ms-excel.sheet.macroEnabled.main+xml" |
                    "application/vnd.ms-excel.template.macroEnabled.main+xml" |
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml" => {
                        let path = Self::normalize_path(Self::CONTENT_TYPES_PATH, &part_name)?;
                        let spreadsheet = Workbook::open(&archive, &path, shared_strings_cache_limit)?;
                        document = Some(Document::Xlsx(spreadsheet));
                        break;
                    }
                    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml" => {
                        // todo!("Presentation support is not implemented yet")
                    }
                    _ => {}
                }
            }
        }

        let relationships =
            Relationship::load_relationships_for(&archive, Self::CONTENT_TYPES_PATH)?
                .ok_or("Failed to load relationships for '[Content_Types].xml'")?;

        if document.is_none() {
            let target = match Relationship::find_relationship(
                &relationships,
                relationship::RelationshipType::OfficeDocument,
            )
            .map(|r| {
                if let TargetMode::Internal(target) = &r.target {
                    Some(target.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default()
            {
                Some(r) => r,
                None => return Err("Unrecognized document type".into()),
            };

            if let Ok(node) = load_first_start_element(&archive, &target) {
                match node.as_str() {
                    "workbook" => {
                        let spreadsheet =
                            Workbook::open(&archive, &target, shared_strings_cache_limit)?;
                        document = Some(Document::Xlsx(spreadsheet));
                    }
                    "document" => {
                        let wordprocessing = Wordprocessing::open(&archive, &target)?;
                        document = Some(Document::Docx(Box::new(wordprocessing)));
                    }
                    _ => {}
                }
            } else {
                const BRT_BEGIN_BOOK: &[u8] = &[0x83, 0x01, 0x00];
                let mut file = archive.find_entry(&target, false)?;
                let mut buf = [0u8; 3];
                if file.read_exact(&mut buf).is_ok() && buf == BRT_BEGIN_BOOK {
                    let workbook =
                        BinaryWorkbook::open(&archive, &target, shared_strings_cache_limit)?;
                    document = Some(Document::Xlsb(workbook));
                }
            }
        }

        let properties = Self::read_metadata(&archive, &relationships)?;
        if let Some(document) = document {
            Ok(Self {
                archive,
                document,
                properties,
            })
        } else {
            Err("Unrecognized document type".into())
        }
    }

    /// Extracts embedded file to specified location
    pub fn extract_file<P: AsRef<Path>>(&self, from: &str, to: P) -> Result<(), OoxmlError> {
        let mut entry = self.archive.find_entry(from, false)?;
        let mut output = File::create(to)?;
        io::copy(&mut entry, &mut output)?;
        Ok(())
    }

    /// Extracts embedded file to specified writer
    ///
    /// Returns false if archive does not contain entry
    pub fn extract_file_to_writer<W: Write>(
        &self,
        from: &str,
        to: &mut W,
    ) -> Result<bool, OoxmlError> {
        if !self.archive.contains(from) {
            return Ok(false);
        }
        let mut entry = self.archive.find_entry(from, false)?;
        io::copy(&mut entry, to)?;
        Ok(true)
    }

    /// Opens embedded file
    pub fn open_file(&self, path: &str) -> Result<Entry, std::io::Error> {
        self.archive.find_entry(path, false)
    }

    fn normalize_path(current_file: &str, new_file: &str) -> Result<String, OoxmlError> {
        if current_file.starts_with('/') || current_file.ends_with('/') {
            unreachable!("current_file cannot start or end with slash");
        }
        if let Some(path) = new_file.strip_prefix('/') {
            return Ok(path.to_string());
        }
        static URI_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let uri_re = URI_RE.get_or_init(|| Regex::new("^[a-zA-Z][a-zA-Z0-9+-.]*:.*").unwrap());
        if uri_re.is_match(current_file) {
            return Err(format!("'{current_file}' is not valid path").into());
        }
        if uri_re.is_match(new_file) {
            return Ok(new_file.to_string());
        }
        let parent = if let Some(index) = current_file.rfind('/') {
            &current_file[0..index]
        } else {
            ""
        };
        let path = if parent.is_empty() {
            new_file.to_string()
        } else {
            [parent, new_file].join("/")
        };
        let parts: Vec<&str> = path.split('/').collect();
        let mut result_parts = Vec::<&str>::new();
        for part in parts {
            match part {
                "." => {}
                ".." => {
                    if result_parts.pop().is_none() {
                        return Err("Path traversal detected".into());
                    }
                }
                part => result_parts.push(part),
            }
        }
        Ok(result_parts.join("/"))
    }

    fn read_metadata(
        archive: &Archive<R>,
        relationships: &[Relationship],
    ) -> Result<DocumentProperties, OoxmlError> {
        fn extract_value(parser: &mut EventReader<Entry>) -> Result<String, OoxmlError> {
            let mut result = String::new();
            loop {
                match parser.next()? {
                    XmlEvent::EndDocument => return Err("Unexpected end of document".into()),
                    XmlEvent::StartElement { .. } => parser.skip()?,
                    XmlEvent::EndElement { .. } => break,
                    XmlEvent::Characters(str) => result.push_str(&str),
                    XmlEvent::Whitespace(str) => result.push_str(&str),
                    _ => {}
                }
            }
            Ok(result)
        }
        let mut result = DocumentProperties::default();

        let target = Relationship::find_relationship(
            relationships,
            relationship::RelationshipType::CoreProperties,
        )
        .map(|r| {
            if let TargetMode::Internal(target) = &r.target {
                Some(target.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
        if let Some(target) = target {
            let entry = archive.find_entry(&target, true)?;
            let mut parser = EventReader::new(entry);
            let event = match parser.next()? {
                XmlEvent::StartDocument { .. } => parser.next()?,
                event => event,
            };
            match event {
                XmlEvent::StartElement { name, .. } if name.local_name == "coreProperties" => {}
                _ => return Err("Expected <coreProperties>".into()),
            }
            loop {
                match parser.next()? {
                    XmlEvent::EndDocument => return Err("Unexpected end of document".into()),
                    XmlEvent::StartElement { name, .. } => match name.local_name.as_str() {
                        "title" => result.core_properties.title = Some(extract_value(&mut parser)?),
                        "subject" => {
                            result.core_properties.subject = Some(extract_value(&mut parser)?)
                        }

                        "creator" => {
                            result.core_properties.creator = Some(extract_value(&mut parser)?)
                        }
                        "keywords" => {
                            result.core_properties.keywords = Some(extract_value(&mut parser)?)
                        }
                        "description" => {
                            result.core_properties.description = Some(extract_value(&mut parser)?)
                        }
                        "lastModifiedBy" => {
                            result.core_properties.last_modified_by =
                                Some(extract_value(&mut parser)?)
                        }
                        "revision" => {
                            result.core_properties.revision = Some(extract_value(&mut parser)?)
                        }
                        "created" => {
                            result.core_properties.created =
                                OffsetDateTime::parse(&extract_value(&mut parser)?, &Rfc3339).ok()
                        }
                        "modified" => {
                            result.core_properties.modified =
                                OffsetDateTime::parse(&extract_value(&mut parser)?, &Rfc3339).ok()
                        }
                        "category" => {
                            result.core_properties.category = Some(extract_value(&mut parser)?)
                        }
                        "contentStatus" => {
                            result.core_properties.content_status =
                                Some(extract_value(&mut parser)?)
                        }
                        "language" => {
                            result.core_properties.language = Some(extract_value(&mut parser)?)
                        }
                        "last_printed" => {
                            result.core_properties.last_printed =
                                OffsetDateTime::parse(&extract_value(&mut parser)?, &Rfc3339).ok()
                        }
                        "version" => {
                            result.core_properties.version =
                                extract_value(&mut parser)?.parse().ok()
                        }
                        _ => {
                            parser.skip()?;
                        }
                    },
                    XmlEvent::EndElement { .. } => break,
                    _ => {}
                }
            }
        }
        let target = Relationship::find_relationship(
            relationships,
            relationship::RelationshipType::ExtendedProperties,
        )
        .map(|r| {
            if let TargetMode::Internal(target) = &r.target {
                Some(target.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
        if let Some(target) = target {
            let entry = archive.find_entry(&target, true)?;
            let mut parser = EventReader::new(entry);
            let event = match parser.next()? {
                XmlEvent::StartDocument { .. } => parser.next()?,
                event => event,
            };
            match event {
                XmlEvent::StartElement { name, .. } if name.local_name == "Properties" => {}
                _ => return Err("Expected <Properties>".into()),
            }
            loop {
                match parser.next()? {
                    XmlEvent::EndDocument => return Err("Unexpected end of document".into()),
                    XmlEvent::StartElement { name, .. } => match name.local_name.as_str() {
                        "Manager" => {
                            result.extended_properties.manager = Some(extract_value(&mut parser)?)
                        }

                        "Company" => {
                            result.extended_properties.company = Some(extract_value(&mut parser)?)
                        }
                        "HyperlinkBase" => {
                            result.extended_properties.hyperlink_base =
                                Some(extract_value(&mut parser)?)
                        }
                        "Template" => {
                            result.extended_properties.template = Some(extract_value(&mut parser)?)
                        }
                        "Pages" => {
                            result.extended_properties.pages =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "Words" => {
                            result.extended_properties.words =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "Characters" => {
                            result.extended_properties.characters =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "Lines" => {
                            result.extended_properties.lines =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "Paragraphs" => {
                            result.extended_properties.paragraphs =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "Slides" => {
                            result.extended_properties.slides =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "Notes" => {
                            result.extended_properties.notes =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "TotalTime" => {
                            result.extended_properties.total_time =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "HiddenSlides" => {
                            result.extended_properties.hidden_slides =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "MMClips" => {
                            result.extended_properties.mm_clips =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "CharactersWithSpaces" => {
                            result.extended_properties.characters_with_spaces =
                                extract_value(&mut parser)?.parse().ok();
                        }
                        "ScaleCrop" => {
                            result.extended_properties.scale_crop =
                                try_parse_bool(extract_value(&mut parser)?);
                        }
                        "LinksUpToDate" => {
                            result.extended_properties.links_up_to_date =
                                try_parse_bool(extract_value(&mut parser)?);
                        }
                        "SharedDoc" => {
                            result.extended_properties.shared_doc =
                                try_parse_bool(extract_value(&mut parser)?);
                        }
                        "HyperlinksChanged" => {
                            result.extended_properties.hyperlinks_changed =
                                try_parse_bool(extract_value(&mut parser)?);
                        }
                        "PresentationFormat" => {
                            result.extended_properties.presentation_format =
                                Some(extract_value(&mut parser)?);
                        }
                        "Application" => {
                            result.extended_properties.application =
                                Some(extract_value(&mut parser)?);
                        }
                        "AppVersion" => {
                            result.extended_properties.app_version =
                                Some(extract_value(&mut parser)?);
                        }
                        "DocSecurity" => {
                            result.extended_properties.doc_security =
                                match extract_value(&mut parser)?.parse()? {
                                    0 => None,
                                    val => Some(DocumentSecurity::from_bits_truncate(val)),
                                }
                        }
                        _ => {
                            parser.skip()?;
                        }
                    },
                    XmlEvent::EndElement { .. } => break,
                    _ => {}
                }
            }
        }
        let target = Relationship::find_relationship(
            relationships,
            relationship::RelationshipType::CustomProperties,
        )
        .map(|r| {
            if let TargetMode::Internal(target) = &r.target {
                Some(target.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();
        if let Some(target) = target {
            let entry = archive.find_entry(&target, true)?;
            let mut parser = EventReader::new(entry);
            let event = match parser.next()? {
                XmlEvent::StartDocument { .. } => parser.next()?,
                event => event,
            };
            match event {
                XmlEvent::StartElement { name, .. } if name.local_name == "Properties" => {}
                _ => return Err("Expected <Properties>".into()),
            }
            loop {
                match parser.next()? {
                    XmlEvent::EndDocument => return Err("Unexpected end of document".into()),
                    XmlEvent::EndElement { .. } => {
                        break;
                    }
                    XmlEvent::StartElement {
                        name, attributes, ..
                    } if name.local_name == "property" => {
                        let name = attributes
                            .into_iter()
                            .find_map(|a| {
                                if a.name.local_name == "name" {
                                    Some(a.value)
                                } else {
                                    None
                                }
                            })
                            .ok_or("Custom property does not have a name")?;
                        fn extract_value(
                            parser: &mut EventReader<Entry>,
                        ) -> Result<String, OoxmlError> {
                            let mut result = String::new();
                            loop {
                                match parser.next()? {
                                    XmlEvent::EndDocument => {
                                        return Err("Unexpected end of document".into());
                                    }
                                    XmlEvent::EndElement { .. } => {
                                        break;
                                    }
                                    XmlEvent::Characters(str) => result.push_str(&str),
                                    XmlEvent::Whitespace(str) => result.push_str(&str),
                                    _ => {
                                        return Err(
                                            "Unexpected XML Event inside custom property value"
                                                .into(),
                                        );
                                    }
                                };
                            }

                            Ok(result)
                        }

                        let mut property_value = UserDefinedProperty::Undecoded;

                        match parser.next()? {
                            XmlEvent::StartElement { name, .. } => match name.local_name.as_str() {
                                "i4" => {
                                    property_value = UserDefinedProperty::Int(
                                        extract_value(&mut parser)?.parse()?,
                                    )
                                }
                                "r8" => {
                                    property_value = UserDefinedProperty::Real(
                                        extract_value(&mut parser)?.parse()?,
                                    )
                                }
                                "lpwstr" => {
                                    property_value =
                                        UserDefinedProperty::String(extract_value(&mut parser)?)
                                }
                                "filetime" => {
                                    property_value =
                                        UserDefinedProperty::DateTime(OffsetDateTime::parse(
                                            &extract_value(&mut parser)?,
                                            &Rfc3339,
                                        )?)
                                }
                                "bool" => {
                                    let v = extract_value(&mut parser)?;
                                    property_value = if let Ok(v) = v.parse::<bool>() {
                                        UserDefinedProperty::Bool(v)
                                    } else {
                                        UserDefinedProperty::Bool(v.parse::<i32>()? != 0)
                                    }
                                }
                                _ => {
                                    parser.skip()?;
                                }
                            },
                            _ => {
                                return Err("Expecting property value".into());
                            }
                        }

                        match parser.next()? {
                            XmlEvent::EndElement { name } if name.local_name == "property" => {}
                            _ => return Err("Unexpected XmlEvent in <property>".into()),
                        }

                        result.custom_properties.push((name, property_value));
                    }
                    XmlEvent::StartElement { .. } => parser.skip()?,
                    _ => {}
                }
            }
        }

        Ok(result)
    }

    /// Return the entry in the document archive that contains VBA data
    pub fn get_vba_entry(&self) -> Option<Result<OoxmlVba<archive::Entry>, io::Error>> {
        match &self.document {
            Document::Docx(doc) => doc.get_vba_path(),
            Document::Xlsx(doc) => doc.get_vba_path(),
            Document::Xlsb(xlsb) => xlsb.get_vba_path(),
        }
        .map(|p| self.open_file(&p).and_then(Ole::new).map(OoxmlVba))
    }
}

/// Embedded Ole object containing VBA data
pub struct OoxmlVba<R: Read + Seek>(pub Ole<R>);

impl<'a, R: Read + Seek> VbaDocument<'a, R> for &'a OoxmlVba<R> {
    fn vba(self) -> Option<Result<Vba<'a, R>, io::Error>> {
        Some(Vba::new(&self.0, ""))
    }
}

enum ParserState {
    Begin,
    Middle(usize),
    End,
    Error(String),
    XmlError(quick_xml::Error),
}

/// User Defined Property values
#[derive(Debug)]
pub enum UserDefinedProperty {
    /// A string
    String(String),
    /// An integer
    Int(i32),
    /// A floating point number
    Real(f64),
    /// A boolean
    Bool(bool),
    /// A date
    DateTime(OffsetDateTime),
    /// An unsupported type
    Undecoded,
}

/// Users can associate core properties with packages. Such core properties enable users
/// to get and set well-known and common sets of property metadata to packages
#[derive(Debug, Default, Clone)]
pub struct CoreProperties {
    /// A categorization of the content of this package.
    pub category: Option<String>,
    /// The status of the content.
    pub content_status: Option<String>,
    /// Date of creation of the resource.
    pub created: Option<OffsetDateTime>,
    ///An entity primarily responsible for making the content of the resource.
    pub creator: Option<String>,
    /// An explanation of the content of the resource.
    pub description: Option<String>,
    /// An unambiguous reference to the resource within a given context.
    pub identifier: Option<String>,
    /// A delimited set of keywords to support searching and indexing.
    /// This is typically a list of terms that are not available elsewhere in the properties.
    pub keywords: Option<String>,
    /// The language of the intellectual content of the resource.
    /// Note that IETF RFC 3066 provides guidance on encoding to represent languages.
    pub language: Option<String>,
    /// The user who performed the last modification. The identification is environment-specific.
    pub last_modified_by: Option<String>,
    /// The date and time of the last printing.
    pub last_printed: Option<OffsetDateTime>,
    /// Date on which the resource was changed.
    pub modified: Option<OffsetDateTime>,
    /// The revision number.
    pub revision: Option<String>,
    /// The topic of the content of the resource.
    pub subject: Option<String>,
    /// The name given to the resource.
    pub title: Option<String>,
    /// The version number.
    pub version: Option<String>,
}

/// Extended properties are a predefined set of metadata properties that are applicable to Office Open XML documents.
#[derive(Debug, Default, Clone)]
pub struct ExtendedProperties {
    /// The name of the application that created this document.
    pub application: Option<String>,
    /// The version of the application which produced this document.
    pub app_version: Option<String>,
    /// The total number of characters in a document.
    pub characters: Option<i32>,
    /// The last count of the number of characters (including spaces) in this document.
    pub characters_with_spaces: Option<i32>,
    /// The name of a company associated with the document.
    pub company: Option<String>,
    /// The security level of a document
    pub doc_security: Option<DocumentSecurity>,
    /// The number of hidden slides in a presentation document.
    pub hidden_slides: Option<i32>,
    /// The base string used for evaluating relative hyperlinks in this document.
    pub hyperlink_base: Option<String>,
    /// This element specifies that one or more hyperlinks in this part were updated exclusively
    /// in this part by a producer. The next producer to open this document shall update
    /// the hyperlink relationships with the new hyperlinks specified in this part.
    pub hyperlinks_changed: Option<bool>,
    /// the total number of lines in a document when last saved by a conforming producer if applicable.
    pub lines: Option<i32>,
    /// This element indicates whether hyperlinks in a document are up-to-date.
    /// TRUE indicate that hyperlinks are updated. FALSE indicate that hyperlinks are outdated.
    pub links_up_to_date: Option<bool>,
    /// The name of a supervisor associated with the document.
    pub manager: Option<String>,
    /// The total number of sound or video clips that are present in the document.
    pub mm_clips: Option<i32>,
    /// The number of slides in a presentation containing notes.
    pub notes: Option<i32>,
    /// The total number of pages of a document if applicable.
    pub pages: Option<i32>,
    /// The total number of paragraphs found in a document if applicable.
    pub paragraphs: Option<i32>,
    /// The intended format for a presentation document.
    pub presentation_format: Option<String>,
    /// The display mode of the document thumbnail.
    /// TRUE enable scaling of the document thumbnail to the display.
    /// FALSE enable cropping of the document thumbnail to show only sections that fits the display.
    pub scale_crop: Option<bool>,
    /// This element indicates if this document is currently shared between multiple producers.
    pub shared_doc: Option<bool>,
    /// The total number of slides in a presentation document.
    pub slides: Option<i32>,
    /// The name of an external document template containing format and style information used to create the current document.
    pub template: Option<String>,
    /// Total time that a document has been edited. The default time unit is minutes.
    pub total_time: Option<i32>,
    /// The total number of words contained in a document when last saved.
    pub words: Option<i32>,
    //
    // TODO: SHOULD WE ADD SUPPORT FOR COMPLEX PROPERTIES?
    // <xsd:element name="HeadingPairs" minOccurs="0" maxOccurs="1" type="CT_VectorVariant"/>
    // <xsd:element name="TitlesOfParts" minOccurs="0" maxOccurs="1" type="CT_VectorLpstr"/>
    // <xsd:element name="HLinks" minOccurs="0" maxOccurs="1" type="CT_VectorVariant"/>
    // <xsd:element name="DigSig" minOccurs="0" maxOccurs="1" type="CT_DigSigBlob"/>
}

/// Combines all document properties
#[derive(Debug, Default)]
pub struct DocumentProperties {
    /// Core Properties
    pub core_properties: CoreProperties,
    /// Extended Properties
    pub extended_properties: ExtendedProperties,
    /// Custom Properties defined by user
    pub custom_properties: Vec<(String, UserDefinedProperty)>,
}

fn try_parse_bool(input: String) -> Option<bool> {
    match input.to_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => input.parse::<i32>().ok().map(|v| v != 0),
    }
}

fn load_first_start_element<R: Read + Seek>(
    archive: &Archive<R>,
    file: &str,
) -> Result<String, OoxmlError> {
    let entry = archive.find_entry(file, true)?;
    let mut parser = EventReader::new(entry);
    loop {
        match parser.next()? {
            XmlEvent::StartDocument { .. } | XmlEvent::Characters(_) | XmlEvent::Whitespace(_) => {
                continue
            }
            XmlEvent::EndDocument => return Err("Unexpected end of document".into()),
            XmlEvent::EndElement { .. } => return Err("Format error".into()),
            XmlEvent::StartElement { name, .. } => return Ok(name.local_name),
        }
    }
}

/// Contains result of processing file
#[derive(Debug, Default)]
pub struct ProcessingSummary {
    /// List of files referenced in document
    pub files_to_process: Vec<FileToProcess>,
    /// List of hyperlinks detected in document
    pub hyperlinks: Vec<String>,
    /// List of detected external resources
    pub external_resources: Vec<String>,
    /// Non empty cells detected (Excel only)
    pub num_cells_detected: u64,
    /// Non empty cells processed (Excel only)
    pub num_cells_processed: u64,
    /// Number of sheets detected (Excel only)
    pub num_sheets_detected: u32,
    /// Number of sheets processed (Excel only)
    pub num_sheets_processed: u32,
    /// Sheet protection (Excel only)
    pub protection: HashMap<String, String>,
}

impl ProcessingSummary {
    pub(crate) fn contains(&self, path: &str) -> bool {
        self.files_to_process.iter().any(|x| x.path == path)
    }
}
