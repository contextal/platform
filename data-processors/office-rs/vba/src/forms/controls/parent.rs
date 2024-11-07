//! # Office forms parent controls
//! The Office forms controls which are suitable to contain embedded
//! child controls

mod frame;
mod userform;

pub use frame::multipage::{MultiPage, Page};
pub use frame::Frame;
pub use userform::{DesignerInfo, UserForm};
