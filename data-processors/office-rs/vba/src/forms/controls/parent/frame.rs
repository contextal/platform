pub mod multipage;
use crate::Ole;
use crate::forms::controls::*;
use std::fmt;
use std::io::{self, Read, Seek};

/// A Frame control
///
/// A `Frame` is both a *child controls* and a *parent controls*: it is embedded
/// inside some control and can in turn have children
pub struct Frame<'a, R: Read + Seek> {
    /// Info common to all child controls (e.g. name, ID, placement, etc.)
    pub ci: OleSiteConcreteControl,
    /// Info common to all parent controls (e.g. visual aspects, child controls, etc.)
    pub pi: ParentControlInfo<'a, R>,
}

impl<'a, R: Read + Seek> fmt::Debug for Frame<'a, R> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Frame")
            .field("ci", &self.ci)
            .field("pi", &self.pi)
            .finish()
    }
}

impl<'a, R: 'a + Read + Seek> Frame<'a, R> {
    pub(crate) fn new(
        ole: &'a Ole<R>,
        control: &OleSiteConcreteControl,
        storage_name: &str,
    ) -> Result<Self, io::Error> {
        Ok(Self {
            ci: control.clone(),
            pi: ParentControlInfo::new(ole, &control.name.clone(), storage_name)?,
        })
    }
}

impl<'a, R: Read + Seek> ParentControl<'a, R> for Frame<'a, R> {
    fn pctrl_info(&'a self) -> &'a ParentControlInfo<'a, R> {
        &self.pi
    }
}

impl<'a, R: Read + Seek> ChildControl for Frame<'a, R> {
    fn cctrl_info(&self) -> &OleSiteConcreteControl {
        &self.ci
    }
}
