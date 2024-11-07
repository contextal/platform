//! Excel structures
use super::{
    records::from_record::{Anomalies, FromRecordStream},
    ExcelError, RecordStream,
};
use ctxutils::cmp::Unsigned as _;
use ctxutils::win32::GUID;
use from_record_derive::FromRecordStream;
use std::{
    fmt,
    io::{Read, Seek},
};
use tracing::debug;

macro_rules! verify_field {
    ($field:ident, $expected_value:expr) => {
        if $field != $expected_value {
            return Err(format!(
                "Found unexpected {} value 0x{:0width$x}, should be 0x{:0width$x}",
                stringify!($field),
                $field,
                $expected_value,
                width = 2 * std::mem::size_of_val(&$field),
            )
            .into());
        }
    };
}

/// Boolean or error
#[derive(Debug)]
pub enum Bes {
    /// Boolean value
    Boolean(bool),
    /// Error value
    Error(String),
}

impl fmt::Display for Bes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bes::Boolean(bool) => write!(f, "{}", bool),
            Bes::Error(error) => write!(f, "{}", error),
        }
    }
}

impl FromRecordStream for Bes {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("Bes::from_record({stream:?})");
        let bool_err = stream.rdu8()?;
        let f_error = stream.rdu8()?;

        if f_error == 0 {
            if bool_err > 1 {
                anomalies.push(format!("Bes: Invalid boolean value 0x{:x}", bool_err));
            }
            Ok(Bes::Boolean(bool_err != 0))
        } else {
            if f_error > 1 {
                anomalies.push(format!("Bes: Invalid f_error value 0x{:x}", f_error));
            }
            let str = error_to_string(bool_err);
            if !str.starts_with('#') {
                anomalies.push(format!("Bes: Invalid error value 0x{:x}", bool_err));
            }
            Ok(Bes::Error(str))
        }
    }
}

fn error_to_string(err: u8) -> String {
    match err {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        0x2A => "#N/A",
        0x2B => "#GETTING_DATA",
        x => return format!("UNSUPPORTED_ERROR(0x{x:02x})"),
    }
    .to_string()
}

/// Type of BOF
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BofType {
    /// Workbook
    Workbook,
    /// Dialog or Worksheet. It depends on WsBool record in substream.
    DialogOrWorksheet,
    /// Chartsheet
    ChartSheet,
    /// Macrosheet
    MacroSheet,
    /// Fallback for invalid values
    Invalid(u16),
}

/// A cell in a sheet
#[derive(Debug, FromRecordStream)]
#[from_record(Struct)]
pub struct Cell {
    /// Zero-based row index
    pub rw: u16,
    /// Zero-based column index
    pub col: u16,
    /// Zero-based index of a XF record
    pub ixfe: u16,
}

/// MS Excel version
#[derive(Debug, PartialEq)]
pub enum ExcelVersion {
    /// Excel 97
    Excel97,
    /// Excel 2000
    Excel2000,
    /// Excel 2002
    Excel2002,
    /// Excel 2003
    OfficeExcel2003,
    /// Excel 2007
    OfficeExcel2007,
    /// Excel 2010
    Excel2010,
    /// Excel 2013
    Excel2013,
    /// Future version of Excel not covered by this enum
    Future(u8),
}

impl ExcelVersion {
    /// Converts u8 to enum ExcelVersion
    pub fn new(version: u8) -> Self {
        match version {
            0x0 => ExcelVersion::Excel97,
            0x1 => ExcelVersion::Excel2000,
            0x2 => ExcelVersion::Excel2002,
            0x3 => ExcelVersion::OfficeExcel2003,
            0x4 => ExcelVersion::OfficeExcel2007,
            0x6 => ExcelVersion::Excel2010,
            0x7 => ExcelVersion::Excel2013,
            x => ExcelVersion::Future(x),
        }
    }
}

/// Current value of a formula
#[derive(Debug)]
pub enum FormulaValue {
    /// Number
    Xnum(f64),
    /// String
    String(String),
    /// Boolean
    Boolean(bool),
    /// Error
    Error(String),
    /// Empty
    Blank,
}

impl FromRecordStream for FormulaValue {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FormulaValue::from_record({stream:?})");
        let byte1 = stream.rdu8()?;
        let byte2 = stream.rdu8()?;
        let byte3 = stream.rdu8()?;
        let byte4 = stream.rdu8()?;
        let byte5 = stream.rdu8()?;
        let byte6 = stream.rdu8()?;
        let f_expr_o = stream.rdu16()?;
        if f_expr_o == 0xFFFF {
            match byte1 {
                0x00 => Ok(FormulaValue::String(String::new())),
                0x01 => Ok(FormulaValue::Boolean(byte3 != 0)),
                0x02 => {
                    let str = error_to_string(byte3);
                    if !str.starts_with('#') {
                        anomalies.push(format!("FormulaValue: Invalid error value 0x{:x}", byte3));
                    }
                    Ok(FormulaValue::Error(str))
                }
                0x03 => Ok(FormulaValue::Blank),
                other => Err(format!("FormulaValue: Invalid byte1 value 0x{other:02x}").into()),
            }
        } else {
            let le_bytes = f_expr_o.to_le_bytes();
            let bytes = [
                byte1,
                byte2,
                byte3,
                byte4,
                byte5,
                byte6,
                le_bytes[0],
                le_bytes[1],
            ];
            let float = f64::from_le_bytes(bytes);
            Ok(FormulaValue::Xnum(float))
        }
    }
}

/// Related to checkboxes or radio buttons (unused)
#[derive(Debug)]
pub struct FtCbls {}

impl FtCbls {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtCbls::from_record({stream:?})");
        verify_field!(ft, 0x0A);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x0C);
        stream.sink(12)?;
        Ok(Self {})
    }
}

/// Properties of checkbox or radio button
#[derive(Debug)]
pub struct FtCblsData {
    /// State of checkbox or radio button
    pub f_checked: FtCblsDataCheckState,
    /// Unicode character of the control's accelerator key
    pub accel: u16,
    /// Specifies whether the control is expected to be displayed without 3d effects
    pub f_no_3d: bool,
}

impl FtCblsData {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtCblsData::from_record({stream:?})");
        verify_field!(ft, 0x12);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x08);
        let f_checked = match stream.rdu16()? {
            0 => FtCblsDataCheckState::Unchecked,
            1 => FtCblsDataCheckState::Checked,
            2 => FtCblsDataCheckState::Mixed,
            v => FtCblsDataCheckState::Invalid(v),
        };
        let accel = stream.rdu16()?;
        let reserved = stream.rdu16()?;
        if reserved != 0 {
            anomalies.push(format!(
                "Invalid FtCblsData.reserved value 0x{reserved:02x}"
            ));
        }
        let misc = stream.rdu16()?;
        let f_no_3d = misc & 1 != 0;
        Ok(Self {
            f_checked,
            accel,
            f_no_3d,
        })
    }
}

/// Checkbox or radio button state
#[derive(Debug)]
pub enum FtCblsDataCheckState {
    /// Control is in unchected state.
    Unchecked,
    /// Control is in checked state.
    Checked,
    /// Control is in mixed state.
    Mixed,
    /// Fallback for invalid value
    Invalid(u16),
}

/// Clipboard format
#[derive(Debug)]
pub struct FtCf {
    /// Specifies the Windows clipboard format of the data associated with the picture.
    pub cf: FtCfType,
}

impl FtCf {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtCf::from_record({stream:?})");
        verify_field!(ft, 0x07);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x02);
        let cf = FtCfType::from_record(stream, anomalies)?;
        Ok(Self { cf })
    }
}

/// Type of the clipboard format
#[derive(Debug)]
pub enum FtCfType {
    /// Specifies the format of the picture is an enhanced metafile.
    EnchancedMetafile,
    /// Specifies the format of the picture is a bitmap.
    Bitmap,
    /// Specifies the picture is in an unspecified format that is neither and enhanced metafile nor a bitmap.
    Unspecified,
    /// Fallback for invalid value
    Invalid(u16),
}

impl FromRecordStream for FtCfType {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtCfType::from_record({stream:?})");
        let value = stream.rdu16()?;
        Ok(match value {
            0x0002 => FtCfType::EnchancedMetafile,
            0x0009 => FtCfType::Bitmap,
            0xFFFF => FtCfType::Unspecified,
            other => FtCfType::Invalid(other),
        })
    }
}

/// Common object properties
#[derive(Debug)]
pub struct FtCmo {
    /// Specifies the type of object represented by the Obj record that contains this FtCmo.
    pub ot: FtCmoType,
    /// Specifies the identifier of this object.
    pub id: u16,
    /// Specifies whether this object is locked.
    pub f_locked: bool,
    /// Specifies whether the application is expected to choose the object’s size.
    pub f_default_size: bool,
    /// Specifies whether this is a chart object that is expected to be published the next time the sheet containing it is published.
    pub f_published: bool,
    /// Specifies whether the image of this object is intended to be included when printed.
    pub f_print: bool,
    /// Specifies whether this object has been disabled.
    pub f_disabled: bool,
    /// Specifies whether this is an auxiliary object that can only be automatically inserted by the application (as opposed to an object that can be inserted by a user).
    pub f_ui_obj: bool,
    /// Specifies whether this object is expected to be updated on load to reflect the values in the range associated with the object.
    pub f_recalc_obj: bool,
    /// Specifies whether this object is expected to be updated whenever the value of a cell in the range associated with the object changes.
    pub f_recalc_obj_always: bool,
}

impl FromRecordStream for FtCmo {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtCmo::from_record({stream:?})");
        let ft = stream.rdu16()?;
        verify_field!(ft, 0x15);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x12);
        let ot = FtCmoType::from_primitive(stream.rdu16()?);
        let id = stream.rdu16()?;

        let misc = stream.rdu16()?;
        let f_locked = misc & 1 != 0;
        if misc & (1 << 1) != 0 {
            anomalies.push("FtCmo: reserved flag is not zero".into());
        }
        let f_default_size = misc & (1 << 2) != 0;
        let f_published = misc & (1 << 3) != 0;
        let f_print = misc & (1 << 4) != 0;
        let f_disabled = misc & (1 << 7) != 0;
        let f_ui_obj = misc & (1 << 8) != 0;
        let f_recalc_obj = misc & (1 << 9) != 0;
        let f_recalc_obj_always = misc & (1 << 12) != 0;

        stream.sink(12)?;

        Ok(Self {
            ot,
            id,
            f_locked,
            f_default_size,
            f_published,
            f_print,
            f_disabled,
            f_ui_obj,
            f_recalc_obj,
            f_recalc_obj_always,
        })
    }
}

/// Type of object
#[derive(Debug, PartialEq)]
pub enum FtCmoType {
    /// Group
    Group,
    /// Line
    Line,
    /// Rectangle
    Rectangle,
    /// Oval
    Oval,
    /// Arc
    Arc,
    /// Chart
    Chart,
    /// Text
    Text,
    /// Button
    Button,
    /// Picture
    Picture,
    /// Polygon
    Polygon,
    /// Checkbox
    Checkbox,
    /// RadioButton
    RadioButton,
    /// EditBox
    EditBox,
    /// Label
    Label,
    /// DialogBox
    DialogBox,
    /// SpinControl
    SpinControl,
    /// Scrollbar
    Scrollbar,
    /// List
    List,
    /// GroupBox
    GroupBox,
    /// DropdownList
    DropdownList,
    /// Note
    Note,
    /// OfficeArtObject
    OfficeArtObject,
    /// Fallback for invalid value
    Invalid(u16),
}

impl FtCmoType {
    pub(crate) fn from_primitive(value: u16) -> Self {
        match value {
            0x0000 => FtCmoType::Group,
            0x0001 => FtCmoType::Line,
            0x0002 => FtCmoType::Rectangle,
            0x0003 => FtCmoType::Oval,
            0x0004 => FtCmoType::Arc,
            0x0005 => FtCmoType::Chart,
            0x0006 => FtCmoType::Text,
            0x0007 => FtCmoType::Button,
            0x0008 => FtCmoType::Picture,
            0x0009 => FtCmoType::Polygon,
            0x000B => FtCmoType::Checkbox,
            0x000C => FtCmoType::RadioButton,
            0x000D => FtCmoType::EditBox,
            0x000E => FtCmoType::Label,
            0x000F => FtCmoType::DialogBox,
            0x0010 => FtCmoType::SpinControl,
            0x0011 => FtCmoType::Scrollbar,
            0x0012 => FtCmoType::List,
            0x0013 => FtCmoType::GroupBox,
            0x0014 => FtCmoType::DropdownList,
            0x0019 => FtCmoType::Note,
            0x001E => FtCmoType::OfficeArtObject,
            other => FtCmoType::Invalid(other),
        }
    }
}

/// Edit box properties
#[derive(Debug)]
pub struct FtEdoData {
    /// Specifies what input data validation is expected to be performed by this edit box.
    pub ivt_edit: FtEdoDataValidation,
    /// Specifies whether this edit box supports multiple lines of text.
    pub f_multi_line: bool,
    /// Specifies whether this edit box contains a vertical scrollbar.
    pub f_vscroll: bool,
    /// Specifies the associated list control.
    pub id: u16,
}

impl FtEdoData {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtEdoData::from_record({stream:?})");
        verify_field!(ft, 0x10);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x08);
        let ivt_edit = match stream.rdu16()? {
            0 => FtEdoDataValidation::AnyString,
            1 => FtEdoDataValidation::Integer,
            2 => FtEdoDataValidation::Number,
            3 => FtEdoDataValidation::RangeReference,
            4 => FtEdoDataValidation::Formula,
            v => FtEdoDataValidation::Invalid(v),
        };
        let f_multi_line = stream.rdu16()? != 0;
        let f_vscroll = stream.rdu16()? != 0;
        let id = stream.rdu16()?;
        Ok(Self {
            ivt_edit,
            f_multi_line,
            f_vscroll,
            id,
        })
    }
}

/// Strings accepted by validation
#[derive(Debug)]
pub enum FtEdoDataValidation {
    /// Any string. No validation is expected.
    AnyString,
    /// An interger.
    Integer,
    /// An number.
    Number,
    /// A range reference
    RangeReference,
    /// A formula
    Formula,
    /// Falback for invalid value.
    Invalid(u16),
}

/// Object group properties
#[derive(Debug)]
pub struct FtGboData {
    /// Unicode character of the control's accelerator key
    pub accel: u16,
    /// Specifies whether the control is expected to be displayed without 3d effects
    pub f_no_3d: bool,
}

impl FtGboData {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtGboData::from_record({stream:?})");
        verify_field!(ft, 0x0F);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x06);
        let accel = stream.rdu16()?;
        let reserved = stream.rdu16()?;
        if reserved != 0 {
            anomalies.push(format!("Invalid FtGboData.reserved value 0x{reserved:02x}"));
        }
        let misc = stream.rdu16()?;
        let f_no_3d = misc & 1 != 0;
        Ok(Self { accel, f_no_3d })
    }
}

/// Group properties related structure (unused)
#[derive(Debug)]
pub struct FtGmo {}

impl FtGmo {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtGmo::from_record({stream:?})");
        verify_field!(ft, 0x06);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x02);
        stream.sink(2)?;
        Ok(Self {})
    }
}

/// Properties of a list or drop-down list
#[derive(Debug)]
pub struct FtLbsData {
    /// An unsigned integer that indirectly specifies whether some of the data in this structure
    /// appear in a subsequent Continue record. If cbFContinued is 0x0000, all of the fields
    /// in this structure except ft and cbFContinued MUST NOT exist.
    pub cb_f_continued: u16,
    /// FtLbsData optional fields wrapped in FtLbsDataInternal for convinience.
    pub internal: Option<FtLbsDataInternal>,
}

impl FtLbsData {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        cmo: &FtCmo,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtLbsData::from_record({stream:?})");
        verify_field!(ft, 0x13);
        let cb_f_continued = stream.rdu16()?;
        if cb_f_continued == 0 {
            return Ok(Self {
                cb_f_continued,
                internal: None,
            });
        }

        let fmla = ObjFmla::from_record(stream, false, anomalies)?;
        let c_lines = stream.rdu16()?;
        let i_sel = stream.rdu16()?;
        let misc = stream.rdu8()?;
        let f_use_cb = misc & 1 != 0;
        let f_valid_plex = misc & 2 != 0;
        let f_valid_ids = misc & 4 != 0;
        let f_no_3d = misc & 8 != 0;
        let w_list_sel_type = (misc >> 4) & 3;
        let lct = stream.rdu8()?;
        let id_edit = stream.rdu16()?;
        let drop_data = if cmo.ot == FtCmoType::DropdownList {
            Some(LbsDropData::from_record(stream, anomalies)?)
        } else {
            None
        };
        let mut rg_lines = Vec::<XLUnicodeString>::new();
        if f_valid_plex {
            while rg_lines.len() < usize::from(c_lines) {
                rg_lines.push(XLUnicodeString::from_record(stream, anomalies)?);
            }
        }
        let mut bsels = Vec::<bool>::new();
        if w_list_sel_type != 0 {
            while rg_lines.len() < usize::from(c_lines) {
                bsels.push(stream.rdu8()? != 0);
            }
        }
        let internal = Some(FtLbsDataInternal {
            bsels,
            c_lines,
            i_sel,
            id_edit,
            drop_data,
            f_no_3d,
            f_use_cb,
            f_valid_ids,
            f_valid_plex,
            fmla,
            lct,
            w_list_sel_type,
            rg_lines,
        });
        Ok(Self {
            cb_f_continued,
            internal,
        })
    }
}

/// Properties of a list or drop-down list (actual content)
#[derive(Debug)]
pub struct FtLbsDataInternal {
    /// Specifies the range of cell values that are the items in this list.
    pub fmla: ObjFmla,
    /// pecifies the number of items in the list.
    pub c_lines: u16,
    /// pecifies the one-based index of the first selected item in this list.
    pub i_sel: u16,
    /// Specifies whether the lct field must be used or ignored.
    pub f_use_cb: bool,
    /// Specifies whether the rgLines field exists.
    pub f_valid_plex: bool,
    /// Specifies whether the idEdit field must be use or ignored.
    pub f_valid_ids: bool,
    /// Specifies whether this control is displayed without 3d effects.
    pub f_no_3d: bool,
    /// Specifies the type of selection behavior this list control is expected to support.
    pub w_list_sel_type: u8,
    /// Specifies the behavior class of this list.
    pub lct: u8,
    /// Specifies the edit box associated with this list.
    pub id_edit: u16,
    /// Specifies properties for this dropdown control.
    pub drop_data: Option<LbsDropData>,
    /// Specifies items in the list.
    pub rg_lines: Vec<XLUnicodeString>,
    /// Specifies which items in the list are part of a multiple selection.
    pub bsels: Vec<bool>,
}

/// Action associated to a control
#[derive(Debug)]
pub struct FtMacro {
    /// Specifies the name of a macro.
    pub fmla: ObjFmla,
}

impl FtMacro {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtMacro::from_record({stream:?})");
        verify_field!(ft, 0x04);
        let fmla = ObjFmla::from_record(stream, false, anomalies)?;
        Ok(Self { fmla })
    }
}

/// Note-type properties
#[derive(Debug)]
pub struct FtNts {
    /// Specifies which items in the list are part of a multiple selection.
    pub guid: GUID,
    /// pecifies whether the comment is shared.
    pub f_shared_note: bool,
}

impl FtNts {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtNts::from_record({stream:?})");
        verify_field!(ft, 0x0D);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x16);
        let guid = GUID::from_le_stream(stream)?;
        let misc = stream.rdi16()?;
        let f_shared_note = misc != 0;
        stream.sink(4)?;
        Ok(Self {
            f_shared_note,
            guid,
        })
    }
}

/// Picture associated data location
#[derive(Debug)]
pub struct FtPictFmla {
    /// Specifies the location of the data for the object associated with the Obj record that contains this FtPictFmla.
    pub fmla: ObjFmla,
    /// Specifies object's data location. Its meaning depends on the value of the cmo.fPrstm field of the Obj record that contains this FtPictFmla
    pub l_pos_in_ctl_stm: Option<u32>,
    /// pecifies the size of this object’s data within the control stream.
    pub cb_buf_in_ctl_stm: Option<u32>,
    /// An optional PictFmlaKey.
    pub key: Option<PictFmlaKey>,
}

impl FtPictFmla {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        pict_flags: &FtPioGrbit,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtPictFmla::from_record({stream:?}, {pict_flags:?})");
        verify_field!(ft, 0x09);
        let _cb = stream.rdu16()?;
        let fmla = ObjFmla::from_record(stream, true, anomalies)?;
        let l_pos_in_ctl_stm = if fmla.fmla.is_some()
            && fmla.fmla.as_ref().unwrap().rgce.first() == Some(&0x02 /* PgTbl */)
        {
            Some(stream.rdu32()?)
        } else {
            None
        };
        let cb_buf_in_ctl_stm = if pict_flags.f_prstm {
            Some(stream.rdu32()?)
        } else {
            None
        };
        let key = if pict_flags.f_ctl {
            Some(PictFmlaKey::from_record(stream, anomalies)?)
        } else {
            None
        };
        Ok(Self {
            fmla,
            l_pos_in_ctl_stm,
            cb_buf_in_ctl_stm,
            key,
        })
    }
}

/// Picture boolean properties
#[derive(Debug)]
pub struct FtPioGrbit {
    /// Specifies whether the picture’s aspect ratio is preserved when rendered in different views.
    pub f_auto_pict: bool,
    /// Specifies whether the pictFmla field of the Obj record that contains this FtPioGrbit specifies a DDE reference.
    pub f_dde: bool,
    /// Specifies whether this object is expected to be updated on print to reflect the values in the cell associated with the object.
    pub f_print_calc: bool,
    ///  specifies whether the picture is displayed as an icon.
    pub f_icon: bool,
    /// Specifies whether this object is an ActiveX control.
    pub f_ctl: bool,
    /// Specifies whether the object data are stored in an embedding storage or in the controls stream (ctls).
    pub f_prstm: bool,
    /// Specifies whether this is a camera picture.
    pub f_camera: bool,
    /// Specifies whether this picture’s size has been explicitly set.
    pub f_default_size: bool,
    /// Specifies whether the OLE server for the object is called to load the object’s data automatically when the parent workbook is opened.
    pub f_auto_load: bool,
}

impl FtPioGrbit {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtPioGrbit::from_record({stream:?})");
        verify_field!(ft, 0x08);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x02);

        let misc = stream.rdu16()?;
        let f_auto_pict = misc & 1 != 0;
        let f_dde = misc & 1 << 1 != 0;
        let f_print_calc = misc & 1 << 2 != 0;
        let f_icon = misc & 1 << 3 != 0;
        let f_ctl = misc & 1 << 4 != 0;
        let f_prstm = misc & 1 << 5 != 0;
        let f_camera = misc & 1 << 7 != 0;
        let f_default_size = misc & 1 << 8 != 0;
        let f_auto_load = misc & 1 << 9 != 0;

        Ok(Self {
            f_auto_load,
            f_auto_pict,
            f_camera,
            f_ctl,
            f_dde,
            f_default_size,
            f_icon,
            f_print_calc,
            f_prstm,
        })
    }
}

/// Radio button properties (unused)
#[derive(Debug)]
pub struct FtRbo {}

impl FtRbo {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtRbo::from_record({stream:?})");
        verify_field!(ft, 0x0B);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x06);
        stream.sink(6)?;
        Ok(Self {})
    }
}

/// Radio button properties
#[derive(Debug)]
pub struct FtRboData {
    /// Specifies the next radio button in a group of radio buttons.
    pub id_rad_next: u16,
    /// Specifies whether this is the first radio button in its group.
    pub f_first_btn: bool,
}

impl FtRboData {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtRboData::from_record({stream:?})");
        verify_field!(ft, 0x11);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x04);
        let id_rad_next = stream.rdu16()?;
        let misc = stream.rdu16()?;
        let f_first_btn = misc != 0;
        Ok(Self {
            id_rad_next,
            f_first_btn,
        })
    }
}

/// Scrollable control properties
#[derive(Debug)]
pub struct FtSbs {
    /// Current value of the control.
    pub i_val: i16,
    /// Minimum allowable value of the control.
    pub i_min: i16,
    /// Maximum allowable value of the control.
    pub i_max: i16,
    /// Specifies the amount by which the control’s value is changed when the user clicks on one of the control’s minor increment regions.
    pub d_inc: i16,
    /// Specifies the amount by which the control’s value is changed when the user clicks on the scrollbar’s page up or page down region.
    pub d_page: i16,
    /// Specifies whether this control scrolls horizontally (true) or vertically (false).
    pub f_horiz: bool,
    /// Scrollbar pixel width
    pub dx_scroll: i16,
    /// Specifies whether this control is expected to be displayed.
    pub f_draw: bool,
    /// Specifies whether only the slider portion of this control is expected to be displayed.
    pub f_draw_slider_only: bool,
    /// Specifies whether the control is expected to interactively track a mouse drag of the control’s scroll thumb (aka elevator).
    pub f_track_elevator: bool,
    /// Specifies whether the control is expected to be displayed without 3d effects
    pub f_no3d: bool,
}

impl FtSbs {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("FtSbs::from_record({stream:?})");
        verify_field!(ft, 0x0C);
        let cb = stream.rdu16()?;
        verify_field!(cb, 0x14);
        stream.sink(4)?;
        let i_val: i16 = stream.rdi16()?;
        let i_min = stream.rdi16()?;
        let i_max = stream.rdi16()?;
        let d_inc = stream.rdi16()?;
        let d_page = stream.rdi16()?;
        let misc = stream.rdu16()?;
        let f_horiz = misc != 0;
        let dx_scroll = stream.rdi16()?;
        let misc = stream.rdu16()?;
        let f_draw = misc & 1 != 0;
        let f_draw_slider_only = misc & 1 << 1 != 0;
        let f_track_elevator = misc & 1 << 2 != 0;
        let f_no3d = misc & 1 << 3 != 0;

        Ok(Self {
            d_inc,
            d_page,
            dx_scroll,
            i_max,
            i_min,
            i_val,
            f_draw,
            f_draw_slider_only,
            f_horiz,
            f_no3d,
            f_track_elevator,
        })
    }
}

/// Hidden state of a sheet
#[derive(Debug)]
pub enum HSState {
    /// Visible
    Visible,
    /// Hidden
    Hidden,
    /// Very Hidden: The sheet is hidden and cannot be displayed using the user interface.
    VeryHidden,
    /// Falback for invalid value
    Invalid(u8),
}

impl FromRecordStream for HSState {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("HSState::from_record({stream:?})");
        let misc = stream.rdu8()?;
        Ok(match misc {
            0x00 => HSState::Visible,
            0x01 => HSState::Hidden,
            0x02 => HSState::VeryHidden,
            other => HSState::Invalid(other),
        })
    }
}

/// Drop-down control properties
#[derive(Debug)]
pub struct LbsDropData {
    /// Specifies the style of this dropdown.
    pub w_style: LbsDropDataStyle,
    /// Specifies whether the data displayed by the dropdown has been filtered in some way.
    pub c_filtered: bool,
    /// Specifies the number of lines to be displayed in the dropdown.
    pub c_line: u16,
    /// Specifies the smallest width in pixels allowed for the dropdown window.
    pub dx_min: u16,
    /// Specifies the current string value in the dropdown.
    pub str: XLUnicodeString,
}

impl FromRecordStream for LbsDropData {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("LbsDropData::from_record({stream:?})");
        let misc = stream.rdu16()?;
        let w_style = match misc & 3 {
            0 => LbsDropDataStyle::Combo,
            1 => LbsDropDataStyle::ComboEdit,
            2 => LbsDropDataStyle::Simple,
            _ => LbsDropDataStyle::Invalid,
        };
        let c_filtered = misc & 8 != 0;
        let c_line = stream.rdu16()?;
        let dx_min = stream.rdu16()?;
        let str = XLUnicodeString::from_record(stream, anomalies)?;
        if stream.pos % 2 == 1 {
            stream.sink(1)?;
        }
        Ok(Self {
            w_style,
            c_filtered,
            c_line,
            dx_min,
            str,
        })
    }
}

/// Style of drop-down control
#[derive(Debug)]
pub enum LbsDropDataStyle {
    /// Combo dropdown control
    Combo,
    /// Combo Edit dropdown control
    ComboEdit,
    /// Simple dropdown control (just the dropdown button)
    Simple,
    /// Fallback for invalid value
    Invalid,
}

/// *Formula*
#[derive(Debug)]
pub struct ObjFmla {
    /// Optional ObjectParsedFormula that specifies the formula
    pub fmla: Option<ObjectParsedFormula>,
    /// An optional PictFmlaEmbedInfo.
    pub embed_info: Option<PictFmlaEmbedInfo>,
}

impl ObjFmla {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft_pict_fmla: bool,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("ObjFmla::from_record({stream:?})");
        let cb_fmla = stream.rdu16()?;
        let end_offset = stream.pos + usize::from(cb_fmla);
        let fmla = if cb_fmla > 0 {
            Some(ObjectParsedFormula::from_record(stream, anomalies)?)
        } else {
            None
        };
        let embed_info =
            if ft_pict_fmla && fmla.is_some() && fmla.as_ref().unwrap().rgce.first() == Some(&0x02)
            {
                Some(PictFmlaEmbedInfo::from_record(stream, anomalies)?)
            } else {
                None
            };
        let padding = end_offset.saturating_sub(stream.pos);
        let padding = u16::try_from(padding)?;
        stream.sink(padding)?;
        Ok(Self { fmla, embed_info })
    }
}

/// PAWEL: ?!?!?!
#[derive(Debug)]
pub struct ObjLinkFmla {
    /// An ObjFmla that specifies the formula which specifies a range which contains a value that is linked to the state of the control.
    pub fmla: ObjFmla,
}

impl ObjLinkFmla {
    pub(crate) fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        ft: u16,
        cmo: &FtCmo,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("ObjLinkFmla::from_record({stream:?})");
        match &cmo.ot {
            FtCmoType::Checkbox | FtCmoType::RadioButton => verify_field!(ft, 0x0014),
            FtCmoType::SpinControl
            | FtCmoType::Scrollbar
            | FtCmoType::List
            | FtCmoType::DropdownList => verify_field!(ft, 0x000E),
            other => return Err(format!("ObjLinkFmla cannot exist for cmo.ot={:?}", other).into()),
        }
        let fmla = ObjFmla::from_record(stream, false, anomalies)?;
        Ok(Self { fmla })
    }
}

/// A *Formula* used in embedded objects
#[derive(Debug)]
pub struct ObjectParsedFormula {
    rgce: Vec<u8>,
}

impl FromRecordStream for ObjectParsedFormula {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("ObjectParsedFormula::from_record({stream:?})");
        let misc = stream.rdu16()?;
        let cce = misc & 0x7FFF;
        // Unused, optional data. Documentation does not specify when this field exists.
        stream.sink(4)?;
        let mut rgce = vec![0u8; usize::from(cce)];
        stream.read_exact(&mut rgce)?;
        Ok(Self { rgce })
    }
}

/// Information about an embedded control associated to a *Formula*
#[derive(Debug)]
pub struct PictFmlaEmbedInfo {
    //ttb: u8,
    //cb_class: u8,
    /// Specifies the class name of the embedded control associated with this Obj.
    pub str_class: XLUnicodeStringNoCch,
}

/// Runtime license key
#[derive(Debug)]
pub struct PictFmlaKey {
    /// Specifies the license key for the ActiveX control. This field is passed to a license-aware object creation method.
    pub key_buf: Vec<u8>,
    /// Specifies a reference to the range where the value of the first cell is linked to the current selection in this picture control.
    pub fmla_linked_cell: ObjFmla,
    /// Specifies the range used to populate the content of this picture control.
    pub fmla_list_fill_range: ObjFmla,
}

impl FromRecordStream for PictFmlaKey {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("PictFmlaKey::from_record({stream:?})");
        let cb_key = stream.rdu32()?;
        let mut key_buf = vec![0u8; usize::try_from(cb_key)?];
        stream.read_exact(&mut key_buf)?;
        let fmla_linked_cell = ObjFmla::from_record(stream, false, anomalies)?;
        let fmla_list_fill_range = ObjFmla::from_record(stream, false, anomalies)?;
        Ok(Self {
            key_buf,
            fmla_linked_cell,
            fmla_list_fill_range,
        })
    }
}

impl FromRecordStream for PictFmlaEmbedInfo {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("PictFmlaEmbedInfo::from_record({stream:?})");
        let ttb = stream.rdu8()?;
        verify_field!(ttb, 0x03);
        let cb_class = stream.rdu8()?;
        let reserved = stream.rdu8()?;
        if reserved != 0 {
            anomalies.push(format!(
                "Invalid PictFmlaEmbedInfo.reserved value 0x{reserved:02x}"
            ));
        }
        let str_class =
            XLUnicodeStringNoCch::from_record(stream, usize::from(cb_class), anomalies)?;
        Ok(Self {
            //ttb,
            //cb_class,
            str_class,
        })
    }
}

/// Numeric value
#[derive(Debug)]
pub struct RkNumber {
    f_x100: bool,
    num: RkNumberValue,
}

impl fmt::Display for RkNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.num {
            RkNumberValue::Int(int) => {
                if self.f_x100 {
                    let float = f64::from(int) / 100.0;
                    write!(f, "{}", float)
                } else {
                    write!(f, "{}", int)
                }
            }
            RkNumberValue::Float(mut float) => {
                if self.f_x100 {
                    float /= 100.0;
                }
                write!(f, "{}", float)
            }
        }
    }
}

impl FromRecordStream for RkNumber {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("RkNumber::from_record({stream:?})");
        let misc = stream.rdu32()?;
        let f_x100 = misc & 1 != 0;
        let f_int = misc & (1 << 1) != 0;
        let num = RkNumberValue::from30bits(misc >> 2, f_int);
        Ok(RkNumber { f_x100, num })
    }
}

#[derive(Debug)]
enum RkNumberValue {
    Int(i32),
    Float(f64),
}

impl RkNumberValue {
    fn from30bits(num: u32, is_integer: bool) -> Self {
        if is_integer {
            let int = if num & (1 << 29) != 0 {
                num | 0xC0000000
            } else {
                num
            } as i32;
            RkNumberValue::Int(int)
        } else {
            let num = u64::from(num) << 34;
            let bytes = num.to_le_bytes();
            let float = f64::from_le_bytes(bytes);
            RkNumberValue::Float(float)
        }
    }
}

/// Numeric data in application-specific format
#[derive(Debug, FromRecordStream)]
#[from_record(Struct)]
pub struct RkRec {
    /// An IXFCell that specifies the format of the numeric value.
    pub ixfe: u16,
    /// Specifies the numeric value.
    pub rk: RkNumber,
}

impl fmt::Display for RkRec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        std::fmt::Display::fmt(&self.rk, f)
    }
}

/// Type of *Worksheet*
#[derive(Debug)]
pub enum SheetType {
    /// Dialog or Worksheet
    DialogOrWorksheet,
    /// Macrosheet
    MacroSheet,
    /// Chartsheet
    ChartSheet,
    /// VBA Module
    VbaModule,
    /// Fallback for invalid value
    Invalid(u8),
}

impl FromRecordStream for SheetType {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        _anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("SheetType::from_record({stream:?})");
        let misc = stream.rdu8()?;
        Ok(match misc {
            0x00 => SheetType::DialogOrWorksheet,
            0x01 => SheetType::MacroSheet,
            0x02 => SheetType::ChartSheet,
            0x06 => SheetType::VbaModule,
            other => SheetType::Invalid(other),
        })
    }
}

/// A Unicode string
pub struct ShortXLUnicodeString {
    cch: u8,
    f_hight_byte: bool,
    rgb: Vec<u8>,
}

fn xl_unicode_to_string(f_hight_byte: bool, rgb: &[u8]) -> String {
    if f_hight_byte {
        utf8dec_rs::decode_utf16le_str(rgb)
    } else {
        let buf = rgb.iter().flat_map(|x| [*x, 0]).collect::<Vec<u8>>();
        utf8dec_rs::decode_utf16le_str(&buf)
    }
}

impl std::fmt::Display for ShortXLUnicodeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", xl_unicode_to_string(self.f_hight_byte, &self.rgb))
    }
}

impl std::fmt::Debug for ShortXLUnicodeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str = xl_unicode_to_string(self.f_hight_byte, &self.rgb);
        f.debug_struct("ShortXLUnicodeString")
            .field("cch", &self.cch)
            .field("f_hight_byte", &self.f_hight_byte)
            .field("rgb", &str)
            .finish()
    }
}

impl FromRecordStream for ShortXLUnicodeString {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("ShortXLUnicodeString::from_record({stream:?})");
        let cch = stream.rdu8()?;
        let misc = stream.rdu8()?;
        let f_hight_byte = misc & 0x01 != 0;
        if misc & 0xFE != 0 {
            anomalies.push("ShortXLUnicodeString: Invalid reserved value".to_string());
        }
        let size = if f_hight_byte {
            2 * usize::from(cch)
        } else {
            usize::from(cch)
        };
        let mut rgb = vec![0u8; size];
        stream.read_exact(&mut rgb)?;
        Ok(ShortXLUnicodeString {
            cch,
            f_hight_byte,
            rgb,
        })
    }
}

/// A Unicode string which can contain formatting information and phonetic string data
pub struct XLUnicodeRichExtendedString {
    cch: u16,
    rgb: Vec<u8>,
}

impl FromRecordStream for XLUnicodeRichExtendedString {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("XLUnicodeRichExtendedString::from_record({stream:?})");
        let cch = stream.rdu16()?;
        let misc = stream.rdu8()?;
        let mut f_hight_byte = misc & 0x01 != 0;
        if misc & 0x02 != 0 {
            anomalies.push("XLUnicodeRichExtendedString: Invalid reserved1 bit value".to_string());
        }
        let f_ext_st = misc & 0x04 != 0;
        let f_rich_st = misc & 0x08 != 0;
        if misc & 0xF0 != 0 {
            anomalies.push(format!(
                "XLUnicodeRichExtendedString: Invalid reserved2 value 0x{:x}",
                misc >> 4
            ));
        }
        let c_run = if f_rich_st {
            Some(stream.rdu16()?)
        } else {
            None
        };
        let cb_ext_rst = if f_ext_st {
            let cb_ext_rst = u32::try_from(stream.rdi32()?)?;
            Some(cb_ext_rst)
        } else {
            None
        };
        let size = 2 * usize::from(cch);
        //debug!("cch:{cch} f={f_hight_byte},{f_ext_st},{f_rich_st} c_run:{c_run:?} cb_ext_rst:{cb_ext_rst:?} size:{size}");
        let mut rgb = Vec::<u8>::new();
        rgb.reserve_exact(size);
        while rgb.len() < size {
            if stream.available() == 0 {
                let misc = stream.read_byte()?;
                f_hight_byte = misc & 0x01 != 0;
            }
            if f_hight_byte {
                rgb.push(stream.read_byte()?);
                rgb.push(stream.read_byte()?);
            } else {
                rgb.push(stream.read_byte()?);
                rgb.push(0);
            };
        }
        let size_of_format_run = 4;
        let mut bytes_to_skip = u32::from(c_run.unwrap_or_default()) * size_of_format_run
            + cb_ext_rst.unwrap_or_default();
        //debug!("rgb={}", xl_unicode_to_string(true, &rgb));
        while bytes_to_skip > 0 {
            let chunk = u16::MAX.umin(bytes_to_skip);
            stream.sink(chunk)?;
            bytes_to_skip -= u32::from(chunk);
        }

        Ok(Self { cch, rgb })
    }
}

impl std::fmt::Display for XLUnicodeRichExtendedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", xl_unicode_to_string(true, &self.rgb))
    }
}

impl std::fmt::Debug for XLUnicodeRichExtendedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str = xl_unicode_to_string(true, &self.rgb);
        f.debug_struct("XLUnicodeRichExtendedString")
            .field("cch", &self.cch)
            .field("rgb", &str)
            .finish()
    }
}

/// A Unicode string
pub struct XLUnicodeString {
    cch: u16,
    f_high_byte: bool,
    rgb: Vec<u8>,
}

impl FromRecordStream for XLUnicodeString {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("XLUnicodeString::from_record({stream:?})");
        let cch = stream.rdu16()?;
        let misc = stream.rdu8()?;
        let f_high_byte = misc & 0x01 != 0;
        if misc & 0xFE != 0 {
            anomalies.push("XLUnicodeString: Invalid reserved1 bit value".to_string());
        }
        let size = if f_high_byte {
            2 * usize::from(cch)
        } else {
            usize::from(cch)
        };
        let mut rgb = vec![0u8; size];
        stream.read_exact(&mut rgb)?;

        Ok(Self {
            cch,
            f_high_byte,
            rgb,
        })
    }
}

impl std::fmt::Display for XLUnicodeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", xl_unicode_to_string(self.f_high_byte, &self.rgb))
    }
}

impl std::fmt::Debug for XLUnicodeString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str = xl_unicode_to_string(self.f_high_byte, &self.rgb);
        f.debug_struct("XLUnicodeString")
            .field("cch", &self.cch)
            .field("f_hight_byte", &self.f_high_byte)
            .field("rgb", &str)
            .finish()
    }
}

/// A Unicode String
pub struct XLUnicodeStringNoCch {
    f_high_byte: bool,
    rgb: Vec<u8>,
}

impl XLUnicodeStringNoCch {
    fn from_record<R: Read + Seek>(
        stream: &mut RecordStream<R>,
        size: usize,
        anomalies: &mut Anomalies,
    ) -> Result<Self, ExcelError>
    where
        Self: Sized,
    {
        debug!("XLUnicodeStringNoCch::from_record({stream:?})");
        let misc = stream.rdu8()?;
        let f_high_byte = misc & 0x01 != 0;
        if misc & 0xFE != 0 {
            anomalies.push("XLUnicodeStringNoCch: Invalid reserved1 bit value".to_string());
        }
        let mut rgb = vec![0u8; size];
        stream.read_exact(&mut rgb)?;
        Ok(Self { f_high_byte, rgb })
    }
}

impl std::fmt::Display for XLUnicodeStringNoCch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", xl_unicode_to_string(self.f_high_byte, &self.rgb))
    }
}

impl std::fmt::Debug for XLUnicodeStringNoCch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str = xl_unicode_to_string(self.f_high_byte, &self.rgb);
        f.debug_struct("XLUnicodeStringNoCch")
            .field("f_hight_byte", &self.f_high_byte)
            .field("rgb", &str)
            .finish()
    }
}
