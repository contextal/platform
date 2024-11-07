use crate::{archive::Archive, Ooxml, OoxmlError};
use serde::Deserialize;
use std::io::{Read, Seek};

/// Enum describing Relationship type
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(from = "String")]
pub enum RelationshipType {
    /// Main document
    OfficeDocument,
    /// Header file
    Header,
    /// Footer file
    Footer,
    /// Embeded image
    Image,
    /// Embeded chart
    Chart,
    /// Semantic data for diagram
    DiagramData,
    /// Embedded MS Office Document
    Package,
    /// Embedded OLE Object
    OleObject,
    /// Embedded VBA Poject
    VbaProject,
    /// Hyperlink
    Hyperlink,
    /// Shared Strings used by Workbook
    SharedStrings,
    /// Document Core Properties
    CoreProperties,
    /// Document Extended Properties
    ExtendedProperties,
    /// User defined Custom Properties
    CustomProperties,
    /// Document settings
    Settings,
    /// Worksheet
    Worksheet,
    ///  macrosheet
    Macrosheet,
    /// Dialogsheet
    Dialogsheet,
    /// Chartsheet
    Chartsheet,
    /// Drawing
    Drawing,
    /// Failover for Relationship Types not covered by this enum
    Other(String),
}

impl From<String> for RelationshipType {
    fn from(s: String) -> RelationshipType {
        match s.as_str() {
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" => RelationshipType::OfficeDocument,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" => RelationshipType::Header,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" => RelationshipType::Footer,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" => RelationshipType::Image,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" => RelationshipType::Chart,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/diagramData" => RelationshipType::DiagramData,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package" => RelationshipType::Package,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/oleObject" => RelationshipType::OleObject,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" => RelationshipType::Hyperlink,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" => RelationshipType::SharedStrings,
            "http://schemas.microsoft.com/office/2006/relationships/vbaProject" => RelationshipType::VbaProject,
            "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" => RelationshipType::CoreProperties,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" => RelationshipType::ExtendedProperties,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" => RelationshipType::CustomProperties,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" => RelationshipType::Settings,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" => RelationshipType::Worksheet,
            "http://schemas.microsoft.com/office/2006/relationships/xlMacrosheet" => RelationshipType::Macrosheet,
            "http://schemas.microsoft.com/office/2006/relationships/xlIntlMacrosheet" => RelationshipType::Macrosheet,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/dialogsheet" => RelationshipType::Dialogsheet,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet" => RelationshipType::Chartsheet,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" => RelationshipType::Drawing,
            _ => RelationshipType::Other(s),
        }
    }
}

impl RelationshipType {
    /// Return relationship type string representation
    pub fn name(&self) -> &str {
        match self {
            RelationshipType::OfficeDocument => "OfficeDocument",
            RelationshipType::Header => "Header",
            RelationshipType::Footer => "Footer",
            RelationshipType::Image => "Image",
            RelationshipType::Chart => "Chart",
            RelationshipType::DiagramData => "DiagramData",
            RelationshipType::Package => "Package",
            RelationshipType::OleObject => "OleObject",
            RelationshipType::VbaProject => "VbaProject",
            RelationshipType::Hyperlink => "Hyperlink",
            RelationshipType::SharedStrings => "SharedStrings",
            RelationshipType::CoreProperties => "CoreProperties",
            RelationshipType::ExtendedProperties => "ExtendedProperties",
            RelationshipType::CustomProperties => "CustomProperties",
            RelationshipType::Settings => "Settings",
            RelationshipType::Worksheet => "Worksheet",
            RelationshipType::Macrosheet => "Macrosheet",
            RelationshipType::Dialogsheet => "Dialogsheet",
            RelationshipType::Chartsheet => "Chartsheet",
            RelationshipType::Drawing => "Drawing",
            RelationshipType::Other(other) => other,
        }
    }
}

/// Relationship describe references from parts to other internal resources in the package or to external
/// resources. They represent the type of connection between a source part and a target resource, and make the
/// connection directly discoverable without looking at the part contents, so they are quick to resolve.
#[derive(Debug)]
pub struct Relationship {
    /// Relationship identifier
    pub id: String,
    /// Relationship target path
    pub target: TargetMode,
    /// Relationship Type
    pub rel_type: RelationshipType,
}

fn map_relationship<R: Read + Seek>(rel: &RelationshipPriv, file: &str) -> Relationship {
    let id = rel.id.clone();
    let target = rel.target.clone();
    let rel_type = rel.rel_type.clone();
    if rel.target_mode != Some("External".to_string()) {
        if let Ok(target) = Ooxml::<R>::normalize_path(file, &rel.target) {
            let target = TargetMode::Internal(target);
            return Relationship {
                id,
                target,
                rel_type,
            };
        }
        // else fallback to TargetMode::External
    }

    let target = TargetMode::External(target);
    Relationship {
        id,
        target,
        rel_type,
    }
}

//  if let Some(ref mut rels) = relationships.rels {
//         for rel in rels.iter_mut() {
//             rel.target = Ooxml::<R>::normalize_path(file, &rel.target)?;
//         }
//     }

/// Relationship target path
#[derive(Debug, Clone)]
pub enum TargetMode {
    /// Absolute path inside ZIP archive
    Internal(String),
    /// Path to external resurce
    External(String),
}

#[derive(Deserialize, Debug)]
struct RelationshipPriv {
    #[serde(rename = "@Id")]
    id: String,
    #[serde(rename = "@Target")]
    target: String,
    #[serde(rename = "@Type")]
    rel_type: RelationshipType,
    #[serde(rename = "@TargetMode")]
    target_mode: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Relationships {
    #[serde(rename = "Relationship")]
    rels: Option<Vec<RelationshipPriv>>,
}

/// Describes reference to internal resource
#[derive(Debug, Clone)]
pub struct FileToProcess {
    /// Absolute path inside ZIP
    pub path: String,
    /// Relationship Type
    pub rel_type: RelationshipType,
}

impl Relationship {
    pub(crate) fn load_relationships_for<R: Read + Seek>(
        archive: &Archive<R>,
        file: &str,
    ) -> Result<Option<Vec<Relationship>>, OoxmlError> {
        let rel_path = Self::get_relation_file_path::<R>(file);

        if !archive.contains(&rel_path) {
            return Ok(None);
        }
        let entry = archive.find_entry(&rel_path, true)?;
        let relationships: Relationships = quick_xml::de::from_reader(entry)?;

        if let Some(relationships) = relationships.rels {
            let result: Vec<Relationship> = relationships
                .iter()
                .map(|r| map_relationship::<R>(r, file))
                .collect();
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    fn get_relation_file_path<R: Read + Seek>(file: &str) -> String {
        if file == Ooxml::<R>::CONTENT_TYPES_PATH {
            return "_rels/.rels".to_string();
        }
        let (rel_dir, filename) = match file.rfind('/') {
            Some(index) => {
                let parent = [&file[0..index], "_rels"].join("/");
                let filename = &file[index + 1..];
                (parent, filename)
            }
            None => ("_rels".to_string(), file),
        };
        let filename = [filename, "rels"].join(".");
        [rel_dir, filename].join("/")
    }

    pub(crate) fn list_vba(relationships: &[Relationship]) -> Vec<&Relationship> {
        relationships
            .iter()
            .filter(|x| x.rel_type == RelationshipType::VbaProject)
            .collect()
    }

    pub(crate) fn find_relationship(
        relationships: &[Relationship],
        rel_type: RelationshipType,
    ) -> Option<&Relationship> {
        relationships.iter().find(|x| x.rel_type == rel_type)
    }
}
