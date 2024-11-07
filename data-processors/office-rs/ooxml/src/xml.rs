pub(crate) mod reader {
    use quick_xml::{name::QName, NsReader};
    use std::{
        borrow::Cow,
        collections::VecDeque,
        io::{BufRead, Read},
    };
    use tracing::debug;

    use crate::OoxmlError;

    use super::mkstr;

    pub(crate) struct EventReader<R: Read + BufRead> {
        reader: NsReader<R>,
        buf: Vec<u8>,
        deque: VecDeque<XmlEvent>,
        event_stack: Vec<Vec<u8>>,
    }

    impl<R: Read + BufRead> EventReader<R> {
        pub fn next(&mut self) -> Result<XmlEvent, quick_xml::Error> {
            self.buf.clear();
            if let Some(event) = self.deque.pop_front() {
                debug!("{event:?}");
                return Ok(event);
            }
            loop {
                let event = match self.reader.read_event_into(&mut self.buf)? {
                    quick_xml::events::Event::Decl(e) => XmlEvent::StartDocument {
                        encoding: match e.encoding() {
                            Some(Ok(Cow::Borrowed(encoding))) => mkstr(encoding),
                            _ => String::new(),
                        },
                    },
                    quick_xml::events::Event::Eof => XmlEvent::EndDocument,
                    quick_xml::events::Event::Start(e) => {
                        self.event_stack.push(e.name().0.to_vec());
                        let name = OwnedName {
                            local_name: mkstr(e.name().local_name().as_ref()),
                        };
                        let mut attributes = Vec::<OwnedAttribute>::new();
                        for a in e.attributes() {
                            let a = match a {
                                Ok(a) => a,
                                Err(err) => return Err(quick_xml::Error::InvalidAttr(err)),
                            };
                            let name = OwnedName {
                                local_name: mkstr(a.key.local_name().as_ref()),
                            };
                            let value = a.unescape_value().unwrap_or_default().to_string();
                            attributes.push(OwnedAttribute { name, value });
                        }
                        XmlEvent::StartElement { name, attributes }
                    }
                    quick_xml::events::Event::End(e) => XmlEvent::EndElement {
                        name: OwnedName {
                            local_name: mkstr(e.local_name().as_ref()),
                        },
                    },
                    quick_xml::events::Event::Empty(e) => {
                        self.deque.push_back(XmlEvent::EndElement {
                            name: OwnedName {
                                local_name: mkstr(e.local_name().as_ref()),
                            },
                        });
                        let name = OwnedName {
                            local_name: mkstr(e.local_name().as_ref()),
                        };
                        let mut attributes = Vec::<OwnedAttribute>::new();
                        for a in e.attributes() {
                            let a = match a {
                                Ok(a) => a,
                                Err(err) => return Err(quick_xml::Error::InvalidAttr(err)),
                            };
                            let name = OwnedName {
                                local_name: mkstr(a.key.local_name().as_ref()),
                            };
                            let value = a.unescape_value().unwrap_or_default().to_string();
                            attributes.push(OwnedAttribute { name, value });
                        }
                        XmlEvent::StartElement { name, attributes }
                    }
                    quick_xml::events::Event::Text(e) => {
                        let text = match e.unescape() {
                            Ok(text) => text.to_string(),
                            Err(_) => mkstr(e.into_inner().as_ref()),
                        };
                        XmlEvent::Characters(text)
                    }
                    _ => continue,
                };
                debug!("{event:?}");
                return Ok(event);
            }
        }
        pub fn skip(&mut self) -> Result<(), quick_xml::Error> {
            if let Some(XmlEvent::EndElement { .. }) = self.deque.front() {
                self.deque.pop_front();
                return Ok(());
            }
            if let Some(end) = self.event_stack.pop() {
                self.buf.clear();
                let end = QName(&end);
                self.reader.read_to_end_into(end, &mut self.buf)?;
                Ok(())
            } else {
                Err(quick_xml::Error::UnexpectedEof(
                    "Invalid skip() usage".to_string(),
                ))
            }
        }
        pub fn new(reader: R) -> Self {
            let mut reader = NsReader::from_reader(reader);
            reader.trim_text(true);
            Self {
                reader,
                buf: Vec::new(),
                deque: VecDeque::new(),
                event_stack: Vec::new(),
            }
        }
        pub fn position(&self) -> Result<u64, OoxmlError> {
            Ok(u64::try_from(self.reader.buffer_position())?)
        }
        // pub fn trim_text(&mut self, val: bool) {
        //     self.reader.trim_text(val);
        // }
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    pub(crate) enum XmlEvent {
        StartDocument {
            encoding: String,
        },
        EndDocument,
        StartElement {
            name: OwnedName,
            attributes: Vec<OwnedAttribute>,
        },
        EndElement {
            name: OwnedName,
        },
        Characters(String),
        Whitespace(String),
    }

    #[derive(Debug)]
    pub(crate) struct OwnedAttribute {
        pub(crate) name: OwnedName,
        pub(crate) value: String,
    }

    #[derive(Debug)]
    pub(crate) struct OwnedName {
        pub(crate) local_name: String,
    }

    // #[derive(Debug)]
    // pub(crate) struct Namespace {}
}

fn mkstr(input: &[u8]) -> String {
    String::from_utf8_lossy(input).to_string()
}
