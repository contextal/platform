use crate::{xml::Xml, OdfError};
use serde::Serialize;
use std::{collections::HashMap, io::BufRead};

#[derive(Debug)]
pub(crate) struct Meta {
    pub(crate) properties: HashMap<String, String>,
    pub(crate) user_properties: Vec<UserProperty>,
}

#[derive(Debug, Serialize)]
pub struct UserProperty {
    pub key: String,
    pub value: String,
}

impl Meta {
    pub fn load_from_xml_parser<R: BufRead>(r: R) -> Result<Meta, OdfError> {
        let xml = Xml::new(r);
        let mut iter = xml
            .into_iterator()
            .enter_xml_path(&["document-meta", "meta"])?;

        let mut properties = HashMap::new();
        let mut user_properties = Vec::new();

        let mut key = None;
        let mut value = None;

        while let Some(event) = iter.next()? {
            match event {
                crate::xml::Event::StartElement(start_element) => {
                    if start_element.name == "user-defined" {
                        let name = start_element
                            .attributes
                            .iter()
                            .find_map(|attr| {
                                if attr.key == "name" {
                                    Some(attr.value.to_string())
                                } else {
                                    None
                                }
                            })
                            .ok_or("Unable to find attribute 'name'")?;
                        key = Some((name, true))
                    } else {
                        key = Some((start_element.name.to_string(), false))
                    }
                }
                crate::xml::Event::EndElement(_) => {
                    if key.is_none() || value.is_none() {
                        continue;
                    }
                    let (key, is_user_property) = key.take().unwrap();
                    let value = value.take().unwrap();
                    if is_user_property {
                        user_properties.push(UserProperty { key, value });
                    } else {
                        properties.insert(key, value);
                    }
                }
                crate::xml::Event::Text(text) => value = Some(text),
            }
        }

        Ok(Meta {
            properties,
            user_properties,
        })
    }
}
