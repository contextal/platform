use crate::OdfError;
use quick_xml::Reader;
use std::io::{BufRead, Read};
use tracing::debug;

#[derive(Debug)]
pub(crate) enum Event {
    StartElement(StartElement),
    EndElement(String),
    Text(String),
}

#[derive(Debug)]
pub(crate) struct Attribute {
    pub(crate) key: String,
    pub(crate) value: String,
}

#[derive(Debug)]
pub(crate) struct StartElement {
    pub(crate) name: String,
    pub(crate) attributes: Vec<Attribute>,
}

pub(crate) struct Xml<R: Read + BufRead> {
    reader: Reader<R>,
    bufer: Vec<u8>,
    stack: Vec<String>,
    queued_end_event: bool,
    finished: bool,
}

impl<R: Read + BufRead> Xml<R> {
    pub(crate) fn new(reader: R) -> Self {
        let mut reader = Reader::from_reader(reader);
        reader.trim_text(true);
        Self {
            reader,
            bufer: Vec::new(),
            stack: Vec::new(),
            queued_end_event: false,
            finished: false,
        }
    }
    pub(crate) fn into_iterator(self) -> XmlEventIterator<R> {
        let depth = self.stack.len();
        XmlEventIterator {
            xml: self,
            depth,
            outer_depth: Vec::new(),
        }
    }
}

pub(crate) struct XmlEventIterator<R: Read + BufRead> {
    xml: Xml<R>,
    depth: usize,
    outer_depth: Vec<usize>,
}

impl<R: Read + BufRead> XmlEventIterator<R> {
    pub(crate) fn enter_inner_element(self) -> Result<XmlEventIterator<R>, OdfError> {
        if self.depth >= self.xml.stack.len() {
            return Err("XmlEventIterator::enter_inner_element() failed".into());
        }
        let xml = self.xml;
        let depth: usize = xml.stack.len();
        let mut outer_depth = self.outer_depth;
        outer_depth.push(self.depth);
        Ok(XmlEventIterator {
            xml,
            depth,
            outer_depth,
        })
    }

    pub(crate) fn enter_xml_path(self, xml_path: &[&str]) -> Result<XmlEventIterator<R>, OdfError> {
        let mut slice = xml_path;

        let mut iter = self;

        while let Some(&head) = slice.first() {
            let event = match iter.next()? {
                Some(event) => event,
                None => return Err(format!("Unable to find <{head}> element").into()),
            };
            match event {
                Event::StartElement(start_element) => {
                    if start_element.name == head {
                        debug!("Found <{}>", start_element.name);
                        iter = iter.enter_inner_element()?;
                        slice = &slice[1..];
                    } else {
                        debug!("Skipping <{}>", start_element.name);
                        iter.skip()?;
                    }
                }
                _ => continue,
            }
        }

        Ok(iter)
    }

    #[allow(dead_code)]
    pub(crate) fn return_outer_element(mut self) -> Result<XmlEventIterator<R>, OdfError> {
        if self.xml.finished {
            return Err("Xml file processing already finished".into());
        }
        let outer_element_depth = self
            .outer_depth
            .pop()
            .ok_or(OdfError::from("No outer element found"))?;
        while self.xml.stack.len() >= self.depth {
            if self.xml.queued_end_event {
                self.xml.queued_end_event = false;
                if self.xml.stack.pop().is_none() {
                    return Err("Logic error".into());
                }
                continue;
            }
            self.xml.bufer.clear();
            let event = match self.xml.reader.read_event_into(&mut self.xml.bufer) {
                Ok(event) => event,
                Err(error) => {
                    self.xml.finished = true;
                    return Err(error.into());
                }
            };
            match event {
                quick_xml::events::Event::Eof => return Err("Unexpected end of xml file".into()),

                quick_xml::events::Event::Start(bytes_start) => {
                    let name_bytes = bytes_start.local_name().into_inner();
                    let name = String::from_utf8_lossy(name_bytes).to_string();
                    self.xml.stack.push(name);
                }
                quick_xml::events::Event::End(bytes_end) => {
                    let name_bytes = bytes_end.local_name().into_inner();
                    let name = String::from_utf8_lossy(name_bytes).to_string();
                    let last = self.xml.stack.pop();
                    if Some(&name) != last.as_ref() {
                        self.xml.finished = true;
                        return Err(format!("Unexpected </{name}> element").into());
                    }
                }

                quick_xml::events::Event::Empty(_)
                | quick_xml::events::Event::Text(_)
                | quick_xml::events::Event::CData(_)
                | quick_xml::events::Event::Comment(_)
                | quick_xml::events::Event::Decl(_)
                | quick_xml::events::Event::PI(_)
                | quick_xml::events::Event::DocType(_) => {}
            }
        }
        let xml = self.xml;
        let depth = outer_element_depth;
        let outer_depth = self.outer_depth;
        Ok(XmlEventIterator {
            xml,
            depth,
            outer_depth,
        })
    }

    pub(crate) fn next(&mut self) -> Result<Option<Event>, OdfError> {
        if self.xml.finished {
            return Ok(None);
        }
        if self.xml.stack.len() < self.depth {
            return Ok(None);
        }
        loop {
            if self.xml.queued_end_event {
                self.xml.queued_end_event = false;
                let name = match self.xml.stack.pop() {
                    Some(name) => name,
                    None => return Err("Logic error".into()),
                };
                return Ok(Some(Event::EndElement(name)));
            }
            self.xml.bufer.clear();
            let event = match self.xml.reader.read_event_into(&mut self.xml.bufer) {
                Ok(event) => event,
                Err(error) => {
                    self.xml.finished = true;
                    return Err(error.into());
                }
            };
            match event {
                quick_xml::events::Event::Start(bytes_start) => {
                    let name_bytes = bytes_start.local_name().into_inner();
                    let name = String::from_utf8_lossy(name_bytes).to_string();
                    let mut attributes = Vec::new();
                    for attribute in bytes_start.attributes() {
                        let a = match attribute {
                            Ok(a) => a,
                            Err(error) => {
                                self.xml.finished = true;
                                return Err(error.into());
                            }
                        };
                        let key = String::from_utf8_lossy(a.key.local_name().as_ref()).to_string();
                        let value = a.unescape_value().unwrap_or_default().to_string();
                        attributes.push(Attribute { key, value });
                    }
                    self.xml.stack.push(name.to_string());
                    let start_element = StartElement { name, attributes };
                    return Ok(Some(Event::StartElement(start_element)));
                }
                quick_xml::events::Event::End(bytes_end) => {
                    let name_bytes = bytes_end.local_name().into_inner();
                    let name = String::from_utf8_lossy(name_bytes).to_string();

                    let last = self.xml.stack.pop();
                    if Some(&name) != last.as_ref() {
                        self.xml.finished = true;
                        return Err(format!("Unexpected </{name}> element").into());
                    }
                    return Ok(Some(Event::EndElement(name)));
                }
                quick_xml::events::Event::Empty(bytes_start) => {
                    let name_bytes = bytes_start.local_name().into_inner();
                    let name = String::from_utf8_lossy(name_bytes).to_string();
                    let mut attributes = Vec::new();
                    for attribute in bytes_start.attributes() {
                        let a = match attribute {
                            Ok(a) => a,
                            Err(err) => {
                                self.xml.finished = true;
                                return Err(err.into());
                            }
                        };
                        let key = String::from_utf8_lossy(a.key.local_name().as_ref()).to_string();
                        let value = a.unescape_value().unwrap_or_default().to_string();
                        attributes.push(Attribute { key, value });
                    }
                    self.xml.stack.push(name.to_string());
                    let start_element = StartElement { name, attributes };
                    self.xml.queued_end_event = true;
                    return Ok(Some(Event::StartElement(start_element)));
                }
                quick_xml::events::Event::Text(e) => {
                    let text = match e.unescape() {
                        Ok(text) => text.to_string(),
                        Err(_) => String::from_utf8_lossy(e.into_inner().as_ref()).to_string(),
                    };
                    return Ok(Some(Event::Text(text)));
                }

                quick_xml::events::Event::Eof => {
                    self.xml.finished = true;
                    if self.xml.stack.is_empty() {
                        return Ok(None);
                    } else {
                        return Err("Unexpected end of xml file".into());
                    }
                }

                quick_xml::events::Event::Decl(_)
                | quick_xml::events::Event::CData(_)
                | quick_xml::events::Event::Comment(_)
                | quick_xml::events::Event::PI(_)
                | quick_xml::events::Event::DocType(_) => continue,
            };
        }
    }
    pub(crate) fn skip(&mut self) -> Result<(), OdfError> {
        if let Some(name) = self.xml.stack.pop() {
            if self.xml.queued_end_event {
                self.xml.queued_end_event = false;
                return Ok(());
            }
            let mut stack = Vec::new();
            loop {
                self.xml.bufer.clear();
                let event = match self.xml.reader.read_event_into(&mut self.xml.bufer) {
                    Ok(event) => event,
                    Err(error) => {
                        self.xml.finished = true;
                        return Err(error.into());
                    }
                };
                match event {
                    quick_xml::events::Event::Start(bytes_start) => {
                        let name_bytes = bytes_start.local_name().into_inner();
                        let name = String::from_utf8_lossy(name_bytes).to_string();
                        stack.push(name);
                    }

                    quick_xml::events::Event::End(bytes_end) => {
                        let name_bytes = bytes_end.local_name().into_inner();
                        let current = String::from_utf8_lossy(name_bytes).to_string();
                        if let Some(last) = stack.pop() {
                            if current != last {
                                return Err(format!(
                                    "Invalid element </{current}>. Expecting </{last}>"
                                )
                                .into());
                            }
                        } else if current != name {
                            return Err(format!(
                                "Invalid element <{current}>. Expecting </{name}>"
                            )
                            .into());
                        } else {
                            return Ok(());
                        }
                    }

                    quick_xml::events::Event::Eof => {
                        self.xml.finished = true;
                        return Err("Unexpected end of xml file".into());
                    }
                    _ => continue,
                };
            }
        } else {
            Err("Skip failed: Not in element".into())
        }
    }
}
