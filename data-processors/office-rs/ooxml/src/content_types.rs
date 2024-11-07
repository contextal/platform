use crate::OoxmlError;
use serde::Deserialize;
use std::io::BufRead;

#[derive(Deserialize, Debug)]
pub(crate) enum ContentType {
    #[serde(rename = "Default")]
    #[allow(dead_code)]
    Extension {
        #[serde(rename = "@Extension")]
        extension: String,
        #[serde(rename = "@ContentType")]
        content_type: String,
    },
    #[serde(rename = "Override")]
    PartName {
        #[serde(rename = "@PartName")]
        part_name: String,
        #[serde(rename = "@ContentType")]
        content_type: String,
    },
}

#[derive(Deserialize, Debug)]
struct Types {
    #[serde(rename = "$value")]
    types: Vec<ContentType>,
}

impl ContentType {
    pub(crate) fn load_from_xml_parser<R: BufRead>(r: R) -> Result<Vec<ContentType>, OoxmlError> {
        let types: Types = quick_xml::de::from_reader(r)?;
        Ok(types.types)
    }
}
