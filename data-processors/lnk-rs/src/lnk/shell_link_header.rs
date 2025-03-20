use bitflags::bitflags;
use byteorder::{LE, ReadBytesExt};
use ctxole::oleps::{Filetime, FromOlepsReader};
use ctxutils::win32::GUID;
use serde::{Serialize, Serializer};
use std::fmt::Display;
use std::io;
use std::io::Cursor;

use super::ReadVec;

pub trait ReadShellLinkHeader {
    fn read_lnk_shell_link_header(&mut self) -> Result<ShellLinkHeader, io::Error>;
    fn read_lnk_guid(&mut self) -> Result<GUID, io::Error>;
}

impl<Reader: std::io::Read> ReadShellLinkHeader for Reader {
    fn read_lnk_shell_link_header(&mut self) -> Result<ShellLinkHeader, io::Error> {
        let data = self.read_bytes(ShellLinkHeader::HEADER_SIZE)?;
        let shell_link_header = ShellLinkHeader::from_slice(data.as_slice())?;
        return Ok(shell_link_header);
    }
    fn read_lnk_guid(&mut self) -> Result<GUID, io::Error> {
        GUID::from_le_stream(self)
    }
}

/// The ShellLinkHeader structure contains identification information, timestamps, and flags that specify the presence of optional structures
#[repr(C)]
#[derive(Serialize)]
pub struct ShellLinkHeader {
    /// The size, in bytes, of this structure. This value MUST be 0x0000004C
    pub header_size: u32,
    /// A class identifier (CLSID). This value MUST be 00021401-0000-0000-C000-000000000046
    pub link_clsid: GUID,
    /// A LinkFlags structure that specifies information about the shell link and the presence of optional portions of the structure
    pub link_flags: LinkFlags,
    /// A FileAttributesFlags structure that specifies information about the link target
    pub file_attributes_flag: FileAttributesFlags,
    /// A FILETIME structure that specifies the creation time of the link target in UTC (Coordinated Universal Time). If the value is zero, there is no creation time set on the link target.
    pub creation_time: Filetime,
    /// A FILETIME structure that specifies the access time of the link target in UTC (Coordinated Universal Time). If the value is zero, there is no access time set on the link target.
    pub access_time: Filetime,
    /// A FILETIME structure that specifies the write time of the link target in UTC (Coordinated Universal Time). If the value is zero, there is no access time set on the link target.
    pub write_time: Filetime,
    /// A 32-bit unsigned integer that specifies the size, in bytes, of the link target. If the link target file is larger than 0xFFFFFFFF, this value specifies the least significant 32 bits of the link target file size.
    pub file_size: u32,
    /// A 32-bit signed integer that specifies the index of an icon within a given icon location
    pub icon_index: u32,
    /// A 32-bit unsigned integer that specifies the expected window state of an application launched by the link. This value SHOULD be one of the following:
    ///     SW_SHOWNORMAL       0x00000001  The application is open and its window is open in a normal fashion.
    ///     SW_SHOWMAXIMIZED    0x00000003  The application is open, and keyboard focus is given to the application, but its window is not shown
    ///     SW_SHOWMINNOACTIVE  0x00000007  The application is open, but its window is not shown. It is not given the keyboard focus
    /// All other values MUST be treated as SW_SHOWNORMAL
    pub show_command: ShowCommand,
    /// A HotKeyFlags structure that specifies the keystrokes used to launch the application referenced by the shortcut key. This value is assigned to the application after it is launched, so that pressing the key activates that application
    pub hot_key: HotKeyFlags,
    /// A value that MUST be zero
    pub reserved1: u16,
    /// A value that MUST be zero
    pub reserved2: u32,
    /// A value that MUST be zero
    pub reserved3: u32,
}

impl ShellLinkHeader {
    pub const HEADER_SIZE: usize = 0x4c;
    pub fn from_slice(slice: &[u8]) -> Result<ShellLinkHeader, io::Error> {
        let mut reader = Cursor::new(slice);

        fn read_header_size(reader: &mut Cursor<&[u8]>) -> Result<u32, io::Error> {
            let header_size = reader.read_u32::<LE>()?;
            if header_size != 0x4c {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid HeaderSize",
                ));
            }
            return Ok(header_size);
        }
        fn read_clsid(reader: &mut Cursor<&[u8]>) -> Result<GUID, io::Error> {
            let clsid = reader.read_lnk_guid()?;
            if clsid.to_string().to_uppercase() != "00021401-0000-0000-C000-000000000046" {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid LinkCLSID",
                ));
            }
            return Ok(clsid);
        }

        return Ok(ShellLinkHeader {
            header_size: read_header_size(&mut reader)?,
            link_clsid: read_clsid(&mut reader)?,
            link_flags: LinkFlags::from_bits_retain(reader.read_u32::<LE>()?),
            file_attributes_flag: FileAttributesFlags::from_bits_retain(reader.read_u32::<LE>()?),
            creation_time: Filetime::from_oleps_reader(&mut reader, 0, true)?,
            access_time: Filetime::from_oleps_reader(&mut reader, 0, true)?,
            write_time: Filetime::from_oleps_reader(&mut reader, 0, true)?,
            file_size: reader.read_u32::<LE>()?,
            icon_index: reader.read_u32::<LE>()?,
            show_command: ShowCommand::new(reader.read_u32::<LE>()?),
            hot_key: HotKeyFlags::new(reader.read_u16::<LE>()?),
            reserved1: reader.read_u16::<LE>()?,
            reserved2: reader.read_u32::<LE>()?,
            reserved3: reader.read_u32::<LE>()?,
        });
    }
}

bitflags! {
    pub struct LinkFlags : u32 {
        const HasLinkTargetIDList = 1;
        const HasLinkInfo = 1 << 1;
        const HasName = 1 << 2;
        const HasRelativePath = 1 << 3;
        const HasWorkingDir = 1 << 4;
        const HasArguments = 1 << 5;
        const HasIconLocation = 1 << 6;
        const IsUnicode = 1 << 7;
        const ForceNoLinkInfo = 1 << 8;
        const HasExpString = 1 << 9;
        const RunInSeparateProcess = 1 << 10;
        //const Unused1 = 1 << 11;
        const HasDarwinID = 1 << 12;
        const RunAsUser = 1 << 13;
        const HasExpIcon = 1 << 14;
        const NoPidlAlias = 1 << 15;
        //const Unused2 = 1 << 16;
        const RunWithShimLayer = 1 << 17;
        const ForceNoLinkTrack = 1 << 18;
        const EnableTargetMetadata = 1 << 19;
        const DisableLinkPathTracking = 1 << 20;
        const DisableKnownFolderTracking = 1 << 21;
        const DisableKnownFolderAlias = 1 << 22;
        const AllowLinkToLink = 1 << 23;
        const UnaliasOnSave = 1 << 24;
        const PreferEnvironmentPath = 1 << 25;
        const KeepLocalIDListForUNCTarget = 1 << 26;
    }
}

impl Serialize for LinkFlags {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut v = Vec::<String>::new();
        for iter in self.iter_names() {
            v.push(iter.0.to_string());
        }
        return v.serialize(serializer);
    }
}

bitflags! {
    pub struct FileAttributesFlags : u32 {
        const FILE_ATTRIBUTE_READONLY = 1;
        const FILE_ATTRIBUTE_HIDDEN = 1 << 1;
        const FILE_ATTRIBUTE_SYSTEM = 1 << 2;
        const Reserved1 = 1 << 3;
        const FILE_ATTRIBUTE_DIRECTORY = 1 << 4;
        const FILE_ATTRIBUTE_ARCHIVE = 1 << 5;
        const Reserved2 = 1 << 6;
        const FILE_ATTRIBUTE_NORMAL = 1 << 7;
        const FILE_ATTRIBUTE_TEMPORARY = 1 << 8;
        const FILE_ATTRIBUTE_SPARSE_FILE = 1 << 9;
        const FILE_ATTRIBUTE_REPARSE_POINT = 1 << 10;
        const FILE_ATTRIBUTE_COMPRESSED = 1 << 11;
        const FILE_ATTRIBUTE_OFFLINE= 1 << 12;
        const FILE_ATTRIBUTE_NOT_CONTENT_INDEXED = 1 << 13;
        const FILE_ATTRIBUTE_ENCRYPTED = 1 << 14;
    }
}

impl Serialize for FileAttributesFlags {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut v = Vec::<String>::new();
        for iter in self.iter_names() {
            v.push(iter.0.to_string());
        }
        return v.serialize(serializer);
    }
}

#[repr(C)]
pub struct ShowCommand {
    value: u32,
}

impl Serialize for ShowCommand {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        return self.to_string().serialize(serializer);
    }
}

impl ShowCommand {
    pub fn new(value: u32) -> Self {
        Self { value }
    }
}

impl Display for ShowCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self.value {
            0x01 => "SW_SHOWNORMAL",
            0x03 => "SW_SHOWMAXIMIZED",
            0x07 => "SW_SHOWMINNOACTIVE",
            _ => "SW_SHOWNORMAL (Invalid)",
        };
        return write!(f, "{result}");
    }
}

#[repr(C)]
pub struct HotKeyFlags {
    low_byte: u8,
    high_byte: u8,
}

impl HotKeyFlags {
    pub fn new(data: u16) -> Self {
        let low_byte = data as u8;
        let high_byte = (data >> 8) as u8;
        return Self {
            low_byte,
            high_byte,
        };
    }

    fn low_byte_to_string(&self) -> String {
        return match self.low_byte {
            0 => "None".to_string(),
            0x30..=0x39 | 0x41..=0x5A => char::from_u32(self.low_byte as u32).unwrap().to_string(),
            0x70..=0x87 => format!("F{}", self.low_byte - 0x69),
            0x90 => "NUM LOCK".to_string(),
            0x91 => "SCROLL LOCK".to_string(),
            _ => "Invalid".to_string(),
        };
    }
    fn high_byte_to_string(&self) -> String {
        let mut v = Vec::<String>::new();
        if self.high_byte & 0x01 != 0 {
            v.push("Shift + ".to_string());
        }
        if self.high_byte & 0x02 != 0 {
            v.push("Ctrl + ".to_string());
        }
        if self.high_byte & 0x04 != 0 {
            v.push("Alt + ".to_string());
        }
        let result = v.concat();
        return result;
    }
}

impl Display for HotKeyFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = self.low_byte_to_string();
        let modifiers = self.high_byte_to_string();
        return write!(f, "{modifiers}{key}");
    }
}

impl Serialize for HotKeyFlags {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        return self.to_string().serialize(serializer);
    }
}

#[test]
fn test_guid() {
    let buff2: [u8; 16] = [
        0xC9, 0x8B, 0x91, 0x35, 0x6D, 0x19, 0xEA, 0x40, 0x97, 0x79, 0x88, 0x9D, 0x79, 0xB7, 0x53,
        0xF0,
    ];
    let mut reader = Cursor::new(&buff2[..]);
    let resut = reader.read_lnk_guid();
    assert!(resut.is_ok());
    let guid = resut.unwrap();
    assert_eq!(guid.to_string(), "35918bc9-196d-40ea-9779-889d79b753f0");
}
