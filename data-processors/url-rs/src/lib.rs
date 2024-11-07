use file_download::FileDownload;
use http_response::{DataUrl, HttpResponse};
use serde::Serialize;
use std::{
    collections::HashSet,
    ops::{Deref, DerefMut},
};

pub mod backend_state;
pub mod config;
pub mod error;
pub mod file_download;
pub mod http_response;
pub mod page;

/// A wrapper to hold a set of backend result symbols, which are intended to be applied-to/used-in
/// the backend response.
#[derive(Debug, Default)]
pub struct BackendResultSymbols(pub HashSet<&'static str>);

impl BackendResultSymbols {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Deref for BackendResultSymbols {
    type Target = HashSet<&'static str>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BackendResultSymbols {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Kinds of objects which could be produced by the backend.
#[derive(Debug, Serialize)]
pub enum ChildType {
    /// Page screenshot produced by the browser.
    Screenshot {
        /// Document's URL.
        url: Option<Url>,
    },

    /// Output produced by Crome's print-to-pdf API call.
    PrintToPdf {
        /// Document's URL.
        url: Option<Url>,
    },

    /// Contents of the URL request document. Usually this is an HTML page.
    ///
    /// The contents might be different from the original HTML, as JavaScript can modify it.
    PageHtmlContent {
        /// Document's URL.
        url: Option<Url>,

        /// Page title.
        title: Option<String>,
    },

    /// Intercepted HTTP response.
    HttpResponse(HttpResponse),

    /// Data URL
    DataUrl(DataUrl),

    /// Downloaded file (what in usual browser experience causes a download dialog or immediately
    /// starts a background download visible in "Downloads" menu).
    FileDownload(FileDownload),
}

/// Wrapper type for String-represented Request ID.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct RequestId(pub String);

impl<T> From<T> for RequestId
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        Self(value.as_ref().into())
    }
}

/// Wraper type for String-represented URL.
#[derive(Debug, Eq, Hash, PartialEq, Serialize)]
pub struct Url(pub String);

impl<T> From<T> for Url
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        Self(value.as_ref().into())
    }
}

impl Deref for Url {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Wraper type for String-represented GUID.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Guid(pub String);

impl<T> From<T> for Guid
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        Self(value.as_ref().into())
    }
}

impl Deref for Guid {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
