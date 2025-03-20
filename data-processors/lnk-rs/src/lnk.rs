pub mod extra_data;
pub mod link_info;
pub mod link_target_id_list;
pub mod shell_link_header;

use self::{
    extra_data::{ExtraData, ReadExtraData},
    link_info::{LinkInfo, ReadLinkInfo},
    link_target_id_list::{LinkTargetIDList, ReadLinkTargetIDList},
    shell_link_header::{LinkFlags, ReadShellLinkHeader, ShellLinkHeader},
};
use byteorder::{LE, ReadBytesExt};
use ctxole::oleps::{CodepageString, UnicodeString};
use serde::{Serialize, Serializer};
use std::{
    fs::File,
    io::{self, Seek},
    path::PathBuf,
};

type CodePage = u16;
const DEFAULT_CODEPAGE: CodePage = 1252;

#[derive(Serialize)]
pub struct LnkFile {
    pub shell_link_header: ShellLinkHeader,
    pub link_target_id_list: Option<LinkTargetIDList>,
    pub link_info: Option<LinkInfo>,
    pub string_data: StringData,
    pub extra_data: Vec<ExtraData>,
}

pub trait ReadVec {
    fn read_bytes(&mut self, size: usize) -> Result<Vec<u8>, io::Error>;
    fn read_windows_string(&mut self, codepage: CodePage) -> Result<CodepageString, io::Error>;
    fn read_windows_string_with_size(
        &mut self,
        codepage: CodePage,
        size: usize,
    ) -> Result<CodepageString, io::Error>;
    fn read_windows_unicode_string(&mut self) -> Result<UnicodeString, io::Error>;
    fn read_windows_unicode_string_with_size(
        &mut self,
        size: usize,
    ) -> Result<UnicodeString, io::Error>;
    fn read_lnk_string_data_entry(
        &mut self,
        codepage: CodePage,
        unicode: bool,
    ) -> Result<StringDataEntry, io::Error>;
    fn read_lnk_string_data(
        &mut self,
        codepage: CodePage,
        flags: &LinkFlags,
    ) -> Result<StringData, io::Error>;
}

impl<Reader: std::io::Read> ReadVec for Reader {
    fn read_bytes(&mut self, bytes: usize) -> Result<Vec<u8>, io::Error> {
        let mut result = vec![0; bytes];
        self.read_exact(&mut result)?;
        return Ok(result);
    }
    fn read_windows_string(&mut self, codepage: CodePage) -> Result<CodepageString, io::Error> {
        let mut vec = Vec::<u8>::new();
        loop {
            let c = self.read_u8()?;
            if c == 0 {
                break;
            }
            vec.push(c);
        }
        let data = vec.to_vec();
        return Ok(CodepageString { data, codepage });
    }
    fn read_windows_string_with_size(
        &mut self,
        codepage: CodePage,
        size: usize,
    ) -> Result<CodepageString, io::Error> {
        let vec = self.read_bytes(size)?;
        let mut slice = &vec[..];
        for i in 0..vec.len() {
            if vec[i] == 0 {
                slice = &vec[0..i];
                break;
            }
        }
        let data = slice.to_vec();
        return Ok(CodepageString { data, codepage });
    }
    fn read_windows_unicode_string(&mut self) -> Result<UnicodeString, io::Error> {
        let mut data = Vec::<u8>::new();
        loop {
            let c1 = self.read_u8()?;
            let c2 = self.read_u8()?;
            if c1 == 0 && c2 == 0 {
                break;
            }
            data.push(c1);
            data.push(c2);
        }
        return Ok(UnicodeString {
            data: data.to_vec(),
        });
    }
    fn read_windows_unicode_string_with_size(
        &mut self,
        size: usize,
    ) -> Result<UnicodeString, io::Error> {
        let data = self.read_bytes(size)?;
        for i in 0..data.len() / 2 {
            if data[2 * i] == 0 && data[2 * i + 1] == 0 {
                return Ok(UnicodeString {
                    data: data[0..2 * i].to_vec(),
                });
            }
        }
        return Ok(UnicodeString {
            data: data.to_vec(),
        });
    }
    fn read_lnk_string_data_entry(
        &mut self,
        codepage: CodePage,
        unicode: bool,
    ) -> Result<StringDataEntry, io::Error> {
        let count_characters = self.read_u16::<LE>()?;

        let string = match unicode {
            true => StringDataEnum::WindowsUnicodeString(
                self.read_windows_unicode_string_with_size(2 * count_characters as usize)?,
            ),
            false => StringDataEnum::WindowsString(
                self.read_windows_string_with_size(codepage, count_characters as usize)?,
            ),
        };
        return Ok(StringDataEntry::new(string));
    }
    fn read_lnk_string_data(
        &mut self,
        codepage: CodePage,
        flags: &LinkFlags,
    ) -> Result<StringData, io::Error> {
        let unicode = flags.contains(LinkFlags::IsUnicode);
        let name_string = match flags.contains(LinkFlags::HasName) {
            true => Some(self.read_lnk_string_data_entry(codepage, unicode)?),
            false => None,
        };
        let relative_path = match flags.contains(LinkFlags::HasRelativePath) {
            true => Some(self.read_lnk_string_data_entry(codepage, unicode)?),
            false => None,
        };
        let working_dir = match flags.contains(LinkFlags::HasWorkingDir) {
            true => Some(self.read_lnk_string_data_entry(codepage, unicode)?),
            false => None,
        };
        let command_line_arguments = match flags.contains(LinkFlags::HasArguments) {
            true => Some(self.read_lnk_string_data_entry(codepage, unicode)?),
            false => None,
        };
        let icon_location = match flags.contains(LinkFlags::HasIconLocation) {
            true => Some(self.read_lnk_string_data_entry(codepage, unicode)?),
            false => None,
        };
        return Ok(StringData::new(
            name_string,
            relative_path,
            working_dir,
            command_line_arguments,
            icon_location,
        ));
    }
}

impl LnkFile {
    pub fn load(path: PathBuf) -> Result<LnkFile, io::Error> {
        let codepage = DEFAULT_CODEPAGE;
        let mut input_file = File::open(path)?;
        input_file.seek(io::SeekFrom::End(0))?;
        let file_size = input_file.stream_position()?;
        input_file.seek(io::SeekFrom::Start(0))?;

        let shell_link_header = input_file.read_lnk_shell_link_header()?;
        let link_target_id_list = match shell_link_header
            .link_flags
            .contains(LinkFlags::HasLinkTargetIDList)
        {
            true => Some(input_file.read_lnk_link_target_id_list(codepage)?),
            false => None,
        };
        let link_info = match shell_link_header
            .link_flags
            .contains(LinkFlags::HasLinkInfo)
        {
            true => Some(input_file.read_lnk_link_info(codepage)?),
            false => None,
        };
        let string_data =
            input_file.read_lnk_string_data(codepage, &shell_link_header.link_flags)?;
        let extra_data = match input_file.stream_position()? == file_size {
            true => Vec::<ExtraData>::new(),
            false => input_file.read_lnk_extra_data_vec(codepage)?,
        };

        let result = Self {
            shell_link_header,
            link_target_id_list,
            link_info,
            string_data,
            extra_data,
        };
        Ok(result)
    }
}

#[derive(Serialize)]
pub struct StringData {
    pub name_string: Option<StringDataEntry>,
    pub relative_path: Option<StringDataEntry>,
    pub working_dir: Option<StringDataEntry>,
    pub command_line_arguments: Option<StringDataEntry>,
    pub icon_location: Option<StringDataEntry>,
}

impl StringData {
    pub fn new(
        name_string: Option<StringDataEntry>,
        relative_path: Option<StringDataEntry>,
        working_dir: Option<StringDataEntry>,
        command_line_arguments: Option<StringDataEntry>,
        icon_location: Option<StringDataEntry>,
    ) -> Self {
        Self {
            name_string,
            relative_path,
            working_dir,
            command_line_arguments,
            icon_location,
        }
    }
}

pub struct StringDataEntry {
    pub string: StringDataEnum,
}

impl Serialize for StringDataEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        return self.string.serialize(serializer);
    }
}

pub enum StringDataEnum {
    WindowsString(CodepageString),
    WindowsUnicodeString(UnicodeString),
}

impl Serialize for StringDataEnum {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        return match self {
            StringDataEnum::WindowsString(string) => string.serialize(serializer),
            StringDataEnum::WindowsUnicodeString(string) => string.serialize(serializer),
        };
    }
}

impl StringDataEntry {
    pub fn new(string: StringDataEnum) -> Self {
        Self { string }
    }
}
