use crate::config::Config;
use openssl::{pkcs7::Pkcs7Flags, x509::X509NameRef};
use pdfium_render::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};
use tracing::warn;

#[derive(Debug, Serialize)]
pub(crate) struct Certificate {
    version: i32,
    serial: Option<String>,
    issuer: Option<String>,
    subject: Option<String>,
    not_before: Option<PrimitiveDateTime>,
    not_after: Option<PrimitiveDateTime>,
    algo: String,
    // is_valid: bool,
    // is_trusted: bool,
}

#[derive(Debug, Serialize)]
pub struct Signature {
    certificates: Vec<Certificate>,
    serial: Option<String>,
    issuer: Option<String>,
    algo: Option<String>,
    time: Option<time::PrimitiveDateTime>,
    //is_valid: bool,
    reason: Option<String>,
}

pub struct Signatures {
    pub signatures: Vec<Signature>,
    pub number_of_unreadable_signatures: usize,
    pub symbols: HashSet<&'static str>,
}

fn format_x509_name(name: &X509NameRef) -> String {
    let mut result = String::new();
    for entry in name.entries() {
        let Ok(data) = entry.data().as_utf8() else {
            continue;
        };
        let Ok(nid) = entry.object().nid().short_name() else {
            continue;
        };
        if !result.is_empty() {
            result.push_str(", ");
        }
        result.push_str(&format!("{nid}: {data}"));
    }
    result
}

impl Signature {
    pub fn from_pdf_signature(pdf_signature: &PdfSignature, config: &Config) -> Option<Self> {
        let contents = pdf_signature.bytes();
        if contents.len() > config.max_signature_size {
            warn!(
                "signature size ({}) is larger than the limit ({})",
                contents.len(),
                config.max_signature_size
            );
            return None;
        };
        let pkcs7 = openssl::pkcs7::Pkcs7::from_der(&contents).ok()?;
        let signed_data = pkcs7.signed()?;
        let mut certificates = Vec::new();
        let stack = signed_data.certificates()?;
        let mut time = None;
        let mut issuer = None;
        let mut algo = None;
        let mut serial = None;
        let reason = pdf_signature.reason();

        if let Some(signing_date) = pdf_signature.signing_date() {
            let date_format = format_description!(
                "D:[year][month][day][hour][minute][second][offset_hour sign:mandatory]'[offset_minute]'"
            );
            time = OffsetDateTime::parse(&signing_date, &date_format)
                .ok()
                .map(|t| {
                    let t = t.to_offset(UtcOffset::UTC);
                    PrimitiveDateTime::new(t.date(), t.time())
                });
        }

        if let Ok(stack) = pkcs7.signers(stack, Pkcs7Flags::all()) {
            if let Some(signer) = stack.get(0) {
                issuer = Some(format_x509_name(signer.issuer_name()));
                algo = Some(signer.signature_algorithm().object().to_string());
                serial = signer
                    .serial_number()
                    .to_bn()
                    .ok()
                    .and_then(|bn| bn.to_hex_str().ok())
                    .map(|s| format!("0x{}", s.to_ascii_uppercase()));
            }
        }

        for cert in stack.into_iter() {
            let version = cert.version();
            let serial = cert
                .serial_number()
                .to_bn()
                .ok()
                .and_then(|bn| bn.to_hex_str().ok())
                .map(|s| format!("0x{}", s.to_ascii_uppercase()));
            let issuer = Some(format_x509_name(cert.issuer_name()));
            let subject = Some(format_x509_name(cert.subject_name()));
            let date_format = format_description!(
                "[month repr:short] [day padding:space] [hour]:[minute]:[second] [year] GMT"
            );
            let not_before =
                PrimitiveDateTime::parse(&cert.not_before().to_string(), &date_format).ok();
            let not_after =
                PrimitiveDateTime::parse(&cert.not_after().to_string(), &date_format).ok();
            let algo = cert.signature_algorithm().object().to_string();

            certificates.push(Certificate {
                version,
                serial,
                issuer,
                subject,
                not_before,
                not_after,
                algo,
            });
        }
        //pkcs7.signers(certs, flags)

        Some(Self {
            certificates,
            serial,
            issuer,
            algo,
            reason,
            time,
        })
    }
}

impl Signatures {
    pub fn from_pdf_document(document: &PdfDocument, config: &Config) -> Self {
        let mut symbols = HashSet::new();
        let mut signatures = Vec::new();
        let mut number_of_unreadable_signatures = 0;
        for signature in document.signatures().iter() {
            if let Some(signature) = Signature::from_pdf_signature(&signature, config) {
                signatures.push(signature);
                if signatures.len() >= config.max_signatures as usize {
                    warn!(
                        "maximum number of signatures ({}) has been reached",
                        config.max_signatures
                    );
                    symbols.insert("MAX_SIGNATURES_REACHED");
                    break;
                }
            } else {
                number_of_unreadable_signatures += 1;
            }
        }
        Signatures {
            number_of_unreadable_signatures,
            signatures,
            symbols,
        }
    }
}
