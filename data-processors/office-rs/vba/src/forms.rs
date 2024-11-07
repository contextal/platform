//! # An interface to Office Forms
//! Office Forms is a set of *ActiveX controls* that provides interactive surfaces to the user
//!
//! ## Control types and hierarchy
//! Form controls can be:
//! * *Parent controls* if they can contain other controls
//! * *Child controls* if they can be embedded (contained) inside parent controls
//!
//! Specifically:
//! * [`UserForm`]s are *parent-only* controls
//! * [`Frame`], [`MultiPage`] and [`Page`] are both *child* and *parent* controls
//! * Everything else is a [*child-only* control](controls::child)
//!
//! ## Navigation
//! Form controls are arranged in a tree based hierarchy:
//! * Root level: a *parent-only* control; this is always a [`UserForm`] typically obtained
//!   via [`Vba::forms()`](crate::Vba::forms)
//! * Branch level: a *child and parent* control which is embedded in the root or in another
//!   branch-level control and which in turn can contain more child controls.
//! * Leaf level: a *child-only* control.
//!
//! All parent controls - regardless if they are parent-only or "child and parent" - come
//! with a [`ParentControl`] trait. The trait allows child enumeration via the
//! [`children()`](ParentControl::children) method and provides quick access to all the
//! common properties of parent controls (see [`ParentControlInfo`]).
//!
//! All child controls - regardless of whether they are child-only or "child and parent" -
//! contain an [`OleSiteConcreteControl`] structure which stores properties that are common to
//! all child controls. The [`ChildControl`] trait provides quick access to them.
//! Additionally each concrete child control struct contains type specific fields.
//!
//! For research and IoCs nearly all fields are made `pub` and all structs are [`Debug`]
//!
//! # Notes
//! Office stores property definitions as differences from a default set, however in some cases
//! the exact default values are not specified nor are easily derivable empirically; these
//! properties are made of type `Option<T>`
//!
//! Similarly `None` is used for properties that are actually optional
//!
//! Despite this implementation has strictly followed [\[MS-OFORMS\]](https://docs.microsoft.com/en-us/openspecs/office_file_formats/ms-oforms/9c79701a-8c3e-4429-a139-b60ac3a1d50a)
//! and [\[MS-OVBA\]](https://docs.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/575462ba-bf67-4190-9fac-c275523c75fc),
//! insane effort was put into verifying vague, contradicting and missing information by crafting
//! the data and manually noting the effects inside Office
//!
//! The aim here is to confer an exact sense of what Office products do and, if feasible, to attempt to
//! extract info even when they give up and fail
//!
//! For that reason, structs in this module have (nearly) all of their fields `pub`; additionally most
//! structs contain an `anomalies` field to report about divergence from the specs which in most cases
//! would cause Office not to load them properly
//!
//! # Examples
//! ```no_run
//! use std::fs::File;
//! use std::io::BufReader;
//! use ctxole::Ole;
//! use vba::forms::*;
//! let ole = Ole::new(BufReader::new(File::open("MyDoc.doc").unwrap())).unwrap();
//! let form = UserForm::new(&ole, "UserForm1", "Macros/UserForm1").unwrap();
//! if let Some(label) = form.children().find_map(|c| {
//!     if let Ok(Control::Label(l)) = c { Some(l) } else { None }
//! }) {
//!     println!("Label {} has caption {}", label.control.name, label.caption);
//! }
//! ```

#[macro_use]
mod mask;
pub mod controls;
mod font;
mod picture;

pub use controls::{child::*, parent::*, *};
pub use font::Font;
pub use picture::Picture;
