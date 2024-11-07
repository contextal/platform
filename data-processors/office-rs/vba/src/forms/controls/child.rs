//! # Office forms embeddable child controls
//! The Office forms controls which can be embedded in parent controls but cannot
//! act as parent controls in turn

mod buttons;
mod image;
mod label;
mod morph;
mod scrollbar;
mod tabstrip;

pub use buttons::{CommandButton, SpinButton};
pub use image::Image;
pub use label::Label;
pub use morph::{CheckBox, ComboBox, ListBox, OptionButton, TextBox, ToggleButton};
pub use scrollbar::ScrollBar;
pub use tabstrip::{Tab, TabStrip};
