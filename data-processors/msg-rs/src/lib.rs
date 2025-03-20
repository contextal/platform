#![warn(missing_docs)]
//! Outlook MSG parser
//!
//! A parser for Outlook MSG data
//!
//! The main interface is [`Msg`]
pub mod crtf;
mod props;

use ctxole::Ole;
use ctxutils::io::*;
pub use props::*;
use std::borrow::Cow;
use std::io::{self, Read, Seek};

#[derive(Debug)]
/// A message recipient
pub struct Recipient<'o, O: Read + Seek> {
    /// Recipient id
    pub id: u32,
    /// Recipient properties
    pub properties: Properties<'o, O>,
}

impl<'o, O: Read + Seek> Recipient<'o, O> {
    fn new(
        id: u32,
        ole: &'o Ole<O>,
        base: &str,
        prop_map: &PropertyMap,
    ) -> Result<Self, io::Error> {
        let base = format!("{base}__recip_version1.0_#{id:08X}/");
        let name = format!("{base}__properties_version1.0");
        let mut r = ole.get_stream_reader(&ole.get_entry_by_name(&name)?);
        r.seek(io::SeekFrom::Current(8))?; // reserved
        Ok(Self {
            id,
            properties: Properties::new(&mut r, &base, prop_map, ole)?,
        })
    }

    /// Return the best guess for the recipient name if available
    pub fn name(&self) -> Option<String> {
        for k in [
            "RecipientDisplayName",
            "DisplayName",
            "TransmittableDisplayName",
        ] {
            if let Some(Ok(s)) = self.properties.read_string(k) {
                return Some(s);
            }
        }
        None
    }

    /// Return the best guess for the recipient e-mail address if available
    pub fn email(&self) -> Option<String> {
        for k in ["SmtpAddress", "EmailAddress"] {
            if let Some(Ok(s)) = self.properties.read_string(k) {
                return Some(s);
            }
        }
        None
    }

    /// Return the type of recipient if available
    pub fn kind(&self) -> Option<RecipientType> {
        Some(match self.properties.as_int("RecipientType")? {
            1 => RecipientType::To,
            2 => RecipientType::Cc,
            3 => RecipientType::Bcc,
            _ => RecipientType::Unknown,
        })
    }
}

#[derive(Debug)]
/// A type of recipient
pub enum RecipientType {
    /// To
    To,
    /// CC
    Cc,
    /// BCC
    Bcc,
    /// Invalid or unknown type
    Unknown,
}

impl RecipientType {
    /// Return the type as a `str`
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::To => "To",
            Self::Cc => "Cc",
            Self::Bcc => "Bcc",
            Self::Unknown => "",
        }
    }
}

#[derive(Debug)]
/// Message *attachment*
///
/// Note: these are not necessarily MIME parts or actual attachments
pub struct Attachment<'o, O: Read + Seek> {
    /// Attachment id
    pub id: u32,
    /// Attachment properties
    pub properties: Properties<'o, O>,
}

impl<'o, O: Read + Seek> Attachment<'o, O> {
    fn new(
        id: u32,
        ole: &'o Ole<O>,
        base: &str,
        prop_map: &PropertyMap,
    ) -> Result<Self, io::Error> {
        let base = format!("{base}__attach_version1.0_#{id:08X}/");
        let name = format!("{base}__properties_version1.0");
        let mut r = ole.get_stream_reader(&ole.get_entry_by_name(&name)?);
        r.seek(io::SeekFrom::Current(8))?; // reserved
        let properties = Properties::new(&mut r, &base, prop_map, ole)?;
        Ok(Self { id, properties })
    }

    /// Return the best guess for the attachment name if available
    pub fn name(&self) -> Option<String> {
        for k in ["AttachLongFilename", "DisplayName", "AttachFilename"] {
            if let Some(Ok(s)) = self.properties.read_string(k) {
                return Some(s);
            }
        }
        None
    }

    /// Return the MIME type of the attachment if available
    pub fn mime_type(&self) -> Option<String> {
        self.properties.read_string("AttachMimeTag")?.ok()
    }

    /// Return whether the attachment is hidden
    pub fn hidden(&self) -> bool {
        self.properties.as_bool("AttachmentHidden").unwrap_or(false)
    }

    /// Return the attachment creation time if available
    pub fn ctime(&self) -> Option<&time::OffsetDateTime> {
        self.properties.as_time("CreationTime")?
    }

    /// Return the attachment modification time if available
    pub fn mtime(&self) -> Option<&time::OffsetDateTime> {
        self.properties.as_time("LastModificationTime")?
    }

    /// Return a reader for the attachment content if available
    pub fn get_binary_stream(&self) -> Option<Result<impl Read + Seek + 'o, std::io::Error>> {
        self.properties.get_stream("AttachDataBinary")
    }

    /// Return the attachment as an [`EmbMsg`]
    pub fn get_embedded_message(
        &self,
        ole: &'o Ole<O>,
        prop_map: &'o PropertyMap,
    ) -> Result<EmbMsg<'o, O>, io::Error> {
        EmbMsg::new(ole, &self.properties.base, prop_map)
    }
}

#[derive(Debug)]
/// Embedded message
pub struct EmbMsg<'o, O: Read + Seek> {
    /// Global property map
    pub property_map: &'o PropertyMap,
    /// Next recipient id
    pub next_recipient_id: u32,
    /// Next attachment id
    pub next_attachment_id: u32,
    /// Number of recipients (as parsed)
    pub recipient_count: u32,
    /// Number of *attachments* (as parsed)
    pub attachment_count: u32,
    /// Message properties
    pub properties: Properties<'o, O>,
    /// Message recipients
    pub recipients: Vec<Recipient<'o, O>>,
    /// Message *attachments*
    pub attachments: Vec<Attachment<'o, O>>,
}

impl<'o, O: Read + Seek> EmbMsg<'o, O> {
    /// Create a new [`EmbMsg`]
    pub fn new(
        ole: &'o Ole<O>,
        prop_base: &str,
        property_map: &'o PropertyMap,
    ) -> Result<Self, io::Error> {
        let base = format!("{}__substg1.0_3701000D/", prop_base);
        let name = format!("{base}__properties_version1.0");
        let mut r = ole.get_stream_reader(&ole.get_entry_by_name(&name)?);
        r.seek(io::SeekFrom::Current(8))?; // reserved
        let next_recipient_id = rdu32le(&mut r)?;
        let next_attachment_id = rdu32le(&mut r)?;
        let recipient_count = rdu32le(&mut r)?;
        let attachment_count = rdu32le(&mut r)?;
        let properties = Properties::new(&mut r, &base, property_map, ole)?;
        let mut recipients: Vec<Recipient<'o, O>> =
            Vec::with_capacity(recipient_count.try_into().unwrap_or(0));
        for rcpt_id in 0..recipient_count {
            recipients.push(Recipient::new(rcpt_id, ole, &base, property_map)?);
        }
        let mut attachments: Vec<Attachment<'o, O>> =
            Vec::with_capacity(attachment_count.try_into().unwrap_or(0));
        for attm_id in 0..attachment_count {
            attachments.push(Attachment::new(attm_id, ole, &base, property_map)?);
        }
        Ok(Self {
            property_map,
            next_recipient_id,
            next_attachment_id,
            recipient_count,
            attachment_count,
            properties,
            recipients,
            attachments,
        })
    }
}

#[derive(Debug)]
/// Outlook message
pub struct Msg<'o, O: Read + Seek> {
    /// Global property map
    pub property_map: PropertyMap,
    /// Next recipient id
    pub next_recipient_id: u32,
    /// Next attachment id
    pub next_attachment_id: u32,
    /// Number of recipients (as parsed)
    pub recipient_count: u32,
    /// Number of *attachments* (as parsed)
    pub attachment_count: u32,
    /// Message properties
    pub properties: Properties<'o, O>,
    /// Message recipients
    pub recipients: Vec<Recipient<'o, O>>,
    /// Message *attachments*
    pub attachments: Vec<Attachment<'o, O>>,
}

impl<'o, O: Read + Seek> Msg<'o, O> {
    /// Parse an ole object as an Outlook MSG
    pub fn new(ole: &'o Ole<O>) -> Result<Self, io::Error> {
        let property_map = PropertyMap::new(ole)?;
        let name = "/__properties_version1.0";
        let mut r = ole.get_stream_reader(&ole.get_entry_by_name(name)?);
        r.seek(io::SeekFrom::Current(8))?; // reserved
        let next_recipient_id = rdu32le(&mut r)?;
        let next_attachment_id = rdu32le(&mut r)?;
        let recipient_count = rdu32le(&mut r)?;
        let attachment_count = rdu32le(&mut r)?;
        r.seek(io::SeekFrom::Current(8))?; // reserved
        let properties = Properties::new(&mut r, "/", &property_map, ole)?;
        let mut recipients: Vec<Recipient<'o, O>> =
            Vec::with_capacity(recipient_count.try_into().unwrap_or(0));
        for rcpt_id in 0..recipient_count {
            recipients.push(Recipient::new(rcpt_id, ole, "/", &property_map)?);
        }
        let mut attachments: Vec<Attachment<'o, O>> =
            Vec::with_capacity(attachment_count.try_into().unwrap_or(0));
        for attm_id in 0..attachment_count {
            attachments.push(Attachment::new(attm_id, ole, "/", &property_map)?);
        }
        Ok(Self {
            property_map,
            next_recipient_id,
            next_attachment_id,
            recipient_count,
            attachment_count,
            properties,
            recipients,
            attachments,
        })
    }
}

/// The Message trait provides a common interface for [`Msg`], [`EmbMsg`]
/// and their commong wrapper [`GenericMessage`]
pub trait Message<'o, O: Read + Seek + 'o> {
    /// Get the next recipient id
    fn get_next_recipient_id(&self) -> u32;
    /// Get the next attachment id
    fn get_next_attachment_id(&self) -> u32;
    /// Get the number of recipients (as parsed)
    fn get_recipient_count(&self) -> u32;
    /// Get the number of *attachments* (as parsed)
    fn get_attachment_count(&self) -> u32;
    /// Get the message properties
    fn get_properties(&self) -> &Properties<'o, O>;
    /// Get the message recipients
    fn get_recipients(&self) -> &[Recipient<'o, O>];
    /// Get the message *attachments*
    fn get_attachments(&self) -> &[Attachment<'o, O>];
    /// Get the property map
    fn get_property_map(&self) -> &PropertyMap;

    /// Return the best guess for the sender name if available
    fn sender_name(&self) -> Option<String> {
        for k in ["SentRepresentingName", "SenderName"] {
            if let Some(Ok(s)) = self.get_properties().read_string(k) {
                return Some(s);
            }
        }
        None
    }

    /// Return the best guess for the sender e-mail address if available
    fn sender_email(&self) -> Option<String> {
        for k in [
            "SentRepresentingSmtpAddress",
            "SenderSmtpAddress",
            "SentRepresentingEmailAddress",
            "SenderEmailAddress",
        ] {
            if let Some(Ok(s)) = self.get_properties().read_string(k) {
                return Some(s);
            }
        }
        None
    }

    /// Return the SMTP e-mail headers if available
    fn headers(&self) -> Option<Headers> {
        if let Some(Ok(headers)) = self.get_properties().read_string("TransportMessageHeaders") {
            Some(Headers(headers))
        } else {
            None
        }
    }

    /// Return a reader for the plain-text message body if available
    fn plain_body(&'o self) -> Option<Result<impl Read + 'o, std::io::Error>> {
        let cp = self
            .get_properties()
            .as_int("InternetCodepage")
            .unwrap_or(1252) as u16;
        let stream = self
            .get_properties()
            .get_stream("Body")?
            .map(|stream| utf8dec_rs::UTF8DecReader::for_windows_cp(cp, stream));
        Some(stream)
    }

    /// Return a reader for the RTF message body if available
    fn rtf_body(
        &'o self,
    ) -> Option<Result<crtf::CompressedRtf<impl Read + Seek + 'o>, std::io::Error>> {
        let compressed_stream = self.get_properties().get_stream("RtfCompressed")?;
        Some(match compressed_stream {
            Ok(stream) => crtf::CompressedRtf::new(stream),
            Err(e) => Err(e),
        })
    }

    /// Return a reader for the HTML message body if available
    fn html_body(&'o self) -> Option<Result<impl Read + 'o, std::io::Error>> {
        let cp = self
            .get_properties()
            .as_int("InternetCodepage")
            .unwrap_or(1252) as u16;
        for k in ["BodyHtml", "Html"] {
            if let Some(res) = self.get_properties().get_stream(k) {
                return Some(
                    res.map(|stream| utf8dec_rs::UTF8DecReader::for_windows_cp(cp, stream)),
                );
            }
        }
        None
    }
}

/// A convenience generic wrapper for either a [`Msg`] or [`EmbMsg`]
pub enum GenericMessage<'o, O: Read + Seek> {
    /// A [`Msg`]
    Main(Msg<'o, O>),
    /// An [`EmbMsg`]
    Embedded(EmbMsg<'o, O>),
}

impl<'o, O: Read + Seek + 'o> Message<'o, O> for GenericMessage<'o, O> {
    fn get_next_recipient_id(&self) -> u32 {
        match self {
            Self::Main(msg) => msg.next_recipient_id,
            Self::Embedded(msg) => msg.next_recipient_id,
        }
    }
    fn get_next_attachment_id(&self) -> u32 {
        match self {
            Self::Main(msg) => msg.next_attachment_id,
            Self::Embedded(msg) => msg.next_attachment_id,
        }
    }
    fn get_recipient_count(&self) -> u32 {
        match self {
            Self::Main(msg) => msg.recipient_count,
            Self::Embedded(msg) => msg.recipient_count,
        }
    }
    fn get_attachment_count(&self) -> u32 {
        match self {
            Self::Main(msg) => msg.attachment_count,
            Self::Embedded(msg) => msg.attachment_count,
        }
    }
    fn get_properties(&self) -> &Properties<'o, O> {
        match self {
            Self::Main(msg) => &msg.properties,
            Self::Embedded(msg) => &msg.properties,
        }
    }
    fn get_recipients(&self) -> &[Recipient<'o, O>] {
        match self {
            Self::Main(msg) => &msg.recipients,
            Self::Embedded(msg) => &msg.recipients,
        }
    }
    fn get_attachments(&self) -> &[Attachment<'o, O>] {
        match self {
            Self::Main(msg) => &msg.attachments,
            Self::Embedded(msg) => &msg.attachments,
        }
    }
    fn get_property_map(&self) -> &PropertyMap {
        match self {
            Self::Main(msg) => &msg.property_map,
            Self::Embedded(msg) => msg.property_map,
        }
    }
}

/// SMTP message headers
pub struct Headers(String);
impl<'a> IntoIterator for &'a Headers {
    type Item = (&'a str, Cow<'a, str>);
    type IntoIter = HeaderIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        HeaderIter {
            headers: self,
            at: 0,
        }
    }
}

/// SMTP message headers iterator
pub struct HeaderIter<'a> {
    headers: &'a Headers,
    at: usize,
}

impl<'a> Iterator for HeaderIter<'a> {
    type Item = (&'a str, Cow<'a, str>);

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        let buf = &self.headers.0[self.at..];
        let mut lines = buf.split_inclusive('\n');
        let (k, v) = loop {
            let line = lines.next()?;
            self.at += line.len();
            if let Some(eol) = line.find(':') {
                break (&line[0..eol], &line[(eol + 1)..]);
            } else {
                // Bogus header
                continue;
            }
        };
        let k = k.trim();
        let mut v = Cow::from(v.trim());
        loop {
            let line = lines.next();
            if line.is_none() {
                break;
            }
            let line = line.unwrap();
            if !line.starts_with(['\t', ' ']) {
                break;
            }
            self.at += line.len();
            let line = line.trim();
            let v_val = v.to_mut();
            v_val.push(' ');
            v_val.push_str(line);
        }
        Some((k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_iter() {
        let hdrs = Headers(String::from(
            "one:1\n\
             two:    2:two\n\
             bogus line\n\
             novalue  :\n\
             folded:     one     \r\n\
             \t \t     two  \n\
             last: final",
        ));
        let mut it: HeaderIter = hdrs.into_iter();
        assert_eq!(it.next().unwrap(), ("one", "1".into()));
        assert_eq!(it.next().unwrap(), ("two", "2:two".into()));
        assert_eq!(it.next().unwrap(), ("novalue", "".into()));
        assert_eq!(it.next().unwrap(), ("folded", "one two".into()));
        assert_eq!(it.next().unwrap(), ("last", "final".into()));
    }
}
