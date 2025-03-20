use bitflags::bitflags;
use byteorder::{LE, ReadBytesExt};
use serde::{Serialize, Serializer};
use std::{
    fmt::Display,
    io::{self, Cursor, Seek},
};

use super::{CodePage, CodepageString, ReadVec, UnicodeString};

pub trait ReadLinkInfo {
    fn read_lnk_link_info(&mut self, codepage: CodePage) -> Result<LinkInfo, io::Error>;
    fn read_lnk_volume_id(&mut self, codepage: CodePage) -> Result<VolumeID, io::Error>;
    fn read_lnk_common_network_relative_link(
        &mut self,
        codepage: CodePage,
    ) -> Result<CommonNetworkRelativeLink, io::Error>;
}

impl<Reader: std::io::Read + Seek> ReadLinkInfo for Reader {
    fn read_lnk_link_info(&mut self, codepage: CodePage) -> Result<LinkInfo, io::Error> {
        let pos = self.stream_position()?;
        let size = self.read_u32::<LE>()? as usize;
        self.seek(io::SeekFrom::Start(pos))?;
        let data = self.read_bytes(size)?;
        return LinkInfo::from_slice(codepage, data.as_slice());
    }
    fn read_lnk_volume_id(&mut self, codepage: CodePage) -> Result<VolumeID, io::Error> {
        let pos = self.stream_position()?;
        let size = self.read_u32::<LE>()? as usize;
        self.seek(io::SeekFrom::Start(pos))?;
        let data = self.read_bytes(size)?;
        return VolumeID::from_slice(codepage, data.as_slice());
    }
    fn read_lnk_common_network_relative_link(
        &mut self,
        codepage: CodePage,
    ) -> Result<CommonNetworkRelativeLink, io::Error> {
        let pos = self.stream_position()?;
        let size = self.read_u32::<LE>()? as usize;
        self.seek(io::SeekFrom::Start(pos))?;
        let data = self.read_bytes(size)?;
        return CommonNetworkRelativeLink::from_slice(codepage, data.as_slice());
    }
}

#[derive(Serialize)]
pub struct LinkInfo {
    pub link_info_size: u32,
    pub link_info_header_size: u32,
    pub link_info_flags: LinkInfoFlags,
    pub volume_id_offset: u32,
    pub local_base_path_offset: u32,
    pub common_network_relative_link_offset: u32,
    pub common_path_suffix_offset: u32,
    pub local_base_path_offset_unicode: Option<u32>,
    pub common_path_suffix_offset_unicode: Option<u32>,
    pub volume_id: Option<VolumeID>,
    pub local_base_path: Option<CodepageString>,
    pub common_network_relative_link: Option<CommonNetworkRelativeLink>,
    pub common_path_suffix: CodepageString,
    pub local_base_path_unicode: Option<UnicodeString>,
    pub common_path_suffix_unicode: Option<UnicodeString>,
}

impl LinkInfo {
    pub fn from_slice(codepage: CodePage, slice: &[u8]) -> Result<LinkInfo, io::Error> {
        let mut reader = Cursor::new(slice);
        let link_info_size = reader.read_u32::<LE>()?;
        let link_info_header_size = reader.read_u32::<LE>()?;
        let link_info_flags = LinkInfoFlags::from_bits_retain(reader.read_u32::<LE>()?);
        let volume_id_offset = reader.read_u32::<LE>()?;
        let local_base_path_offset = reader.read_u32::<LE>()?;
        let common_network_relative_link_offset = reader.read_u32::<LE>()?;
        let common_path_suffix_offset = reader.read_u32::<LE>()?;
        let (local_base_path_offset_unicode, common_path_suffix_offset_unicode) =
            match link_info_flags.contains(LinkInfoFlags::VolumeIDAndLocalBasePath)
                && link_info_header_size >= 0x24
            {
                true => (
                    Some(reader.read_u32::<LE>()?),
                    Some(reader.read_u32::<LE>()?),
                ),
                false => (None, None),
            };
        let volume_id: Option<VolumeID> =
            match link_info_flags.contains(LinkInfoFlags::VolumeIDAndLocalBasePath) {
                true => {
                    reader.seek(io::SeekFrom::Start(volume_id_offset as u64))?;
                    Some(reader.read_lnk_volume_id(codepage)?)
                }
                false => None,
            };

        let local_base_path =
            match link_info_flags.contains(LinkInfoFlags::VolumeIDAndLocalBasePath) {
                true => {
                    reader.seek(io::SeekFrom::Start(local_base_path_offset as u64))?;
                    Some(reader.read_windows_string(codepage)?)
                }
                false => None,
            };

        let common_network_relative_link: Option<CommonNetworkRelativeLink> =
            match link_info_flags.contains(LinkInfoFlags::CommonNetworkRelativeLinkAndPathSuffix) {
                true => {
                    reader.seek(io::SeekFrom::Start(
                        common_network_relative_link_offset as u64,
                    ))?;
                    Some(reader.read_lnk_common_network_relative_link(codepage)?)
                }
                false => None,
            };

        reader.seek(io::SeekFrom::Start(common_path_suffix_offset as u64))?;
        let common_path_suffix = reader.read_windows_string(codepage)?;
        let local_base_path_unicode: Option<UnicodeString>;
        let common_path_suffix_unicode: Option<UnicodeString>;
        if link_info_header_size >= 0x24 {
            if link_info_flags.contains(LinkInfoFlags::VolumeIDAndLocalBasePath) {
                reader.seek(io::SeekFrom::Start(
                    local_base_path_offset_unicode.unwrap() as u64
                ))?;
                local_base_path_unicode = Some(reader.read_windows_unicode_string()?);
            } else {
                local_base_path_unicode = None;
            }
            reader.seek(io::SeekFrom::Start(
                common_path_suffix_offset_unicode.unwrap() as u64,
            ))?;
            common_path_suffix_unicode = Some(reader.read_windows_unicode_string()?);
        } else {
            local_base_path_unicode = None;
            common_path_suffix_unicode = None;
        }

        return Ok(LinkInfo {
            link_info_size,
            link_info_header_size,
            link_info_flags,
            volume_id_offset,
            local_base_path_offset,
            common_network_relative_link_offset,
            common_path_suffix_offset,
            local_base_path_offset_unicode,
            common_path_suffix_offset_unicode,
            volume_id,
            local_base_path,
            common_network_relative_link,
            common_path_suffix,
            local_base_path_unicode,
            common_path_suffix_unicode,
        });
    }
}

bitflags! {
    #[repr(C)]
    pub struct LinkInfoFlags : u32 {
        const VolumeIDAndLocalBasePath = 1;
        const CommonNetworkRelativeLinkAndPathSuffix = 1 << 1;
    }
}

impl Serialize for LinkInfoFlags {
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

#[derive(Serialize)]
pub struct VolumeID {
    pub volume_id_size: u32,
    pub drive_type: DriveType,
    pub drive_serial_number: u32,
    pub volume_label_offset: u32,
    pub volume_label_offset_unicode: Option<u32>,
    pub volume_label: CodepageString,
    pub volume_label_unicode: Option<UnicodeString>,
}

impl VolumeID {
    pub fn new(
        volume_id_size: u32,
        drive_type: DriveType,
        drive_serial_number: u32,
        volume_label_offset: u32,
        volume_label_offset_unicode: Option<u32>,
        volume_label: CodepageString,
        volume_label_unicode: Option<UnicodeString>,
    ) -> Self {
        Self {
            volume_id_size,
            drive_type,
            drive_serial_number,
            volume_label_offset,
            volume_label_offset_unicode,
            volume_label,
            volume_label_unicode,
        }
    }
    pub fn from_slice(codepage: CodePage, slice: &[u8]) -> Result<VolumeID, io::Error> {
        let mut reader = Cursor::new(slice);
        let volume_id_size = reader.read_u32::<LE>()?;
        let drive_type: DriveType = reader.read_u32::<LE>()?.into();
        let drive_serial_number = reader.read_u32::<LE>()?;
        let volume_label_offset = reader.read_u32::<LE>()?;
        let volume_label_offset_unicode = match volume_label_offset {
            0x14 => Some(reader.read_u32::<LE>()?),
            _ => None,
        };
        let volume_label_unicode: Option<UnicodeString>;
        if let Some(offset) = volume_label_offset_unicode {
            reader.seek(io::SeekFrom::Start(offset as u64))?;
            volume_label_unicode = Some(reader.read_windows_unicode_string()?);
        } else {
            volume_label_unicode = None;
        }
        reader.seek(io::SeekFrom::Start(volume_label_offset as u64))?;
        let volume_label = reader.read_windows_string(codepage)?;

        return Ok(VolumeID::new(
            volume_id_size,
            drive_type,
            drive_serial_number,
            volume_label_offset,
            volume_label_offset_unicode,
            volume_label,
            volume_label_unicode,
        ));
    }
}

#[derive(Serialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DriveType {
    DriveUnknown,
    DriveNoRootDir,
    DriveRemovable,
    DriveFixed,
    DriveRemote,
    DriveCdrom,
    DriveRamdisk,
    #[serde(serialize_with = "serialize_drive_type_invalid")]
    Invalid(u32),
}

fn serialize_drive_type_invalid<S>(value: &u32, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    return format!("0x{:08X}", value).serialize(s);
}

impl From<u32> for DriveType {
    fn from(value: u32) -> Self {
        match value {
            0 => DriveType::DriveUnknown,
            1 => DriveType::DriveNoRootDir,
            2 => DriveType::DriveRemovable,
            3 => DriveType::DriveFixed,
            4 => DriveType::DriveRemote,
            5 => DriveType::DriveCdrom,
            6 => DriveType::DriveRamdisk,
            other => DriveType::Invalid(other),
        }
    }
}

#[derive(Serialize)]
pub struct CommonNetworkRelativeLink {
    pub common_network_relative_link_size: u32,
    pub common_network_relative_link_flags: CommonNetworkRelativeLinkFlags,
    pub net_name_offset: u32,
    pub device_name_offset: u32,
    pub network_provider_type: NetworkProviderType,
    pub net_name_offset_unicode: Option<u32>,
    pub device_name_offset_unicode: Option<u32>,
    pub net_name: CodepageString,
    pub device_name: Option<CodepageString>,
    pub net_name_unicode: Option<UnicodeString>,
    pub device_name_unicode: Option<UnicodeString>,
}

impl CommonNetworkRelativeLink {
    pub fn from_slice(
        codepage: CodePage,
        slice: &[u8],
    ) -> Result<CommonNetworkRelativeLink, io::Error> {
        let mut reader = Cursor::new(slice);
        let common_network_relative_link_size = reader.read_u32::<LE>()?;
        let common_network_relative_link_flags =
            CommonNetworkRelativeLinkFlags::from_bits_retain(reader.read_u32::<LE>()?);
        let net_name_offset = reader.read_u32::<LE>()?;
        let device_name_offset = reader.read_u32::<LE>()?;
        let network_provider_type = NetworkProviderType::new(reader.read_u32::<LE>()?);
        let net_name_offset_unicode = match net_name_offset > 0x14 {
            true => Some(reader.read_u32::<LE>()?),
            false => None,
        };
        let device_name_offset_unicode = match net_name_offset > 0x14 {
            true => Some(reader.read_u32::<LE>()?),
            false => None,
        };
        reader.seek(io::SeekFrom::Start(net_name_offset as u64))?;
        let net_name = reader.read_windows_string(codepage)?;
        let device_name = match common_network_relative_link_flags
            .contains(CommonNetworkRelativeLinkFlags::ValidDevice)
        {
            true => {
                reader.seek(io::SeekFrom::Start(device_name_offset as u64))?;
                Some(reader.read_windows_string(codepage)?)
            }
            false => None,
        };
        let net_name_unicode: Option<UnicodeString> = match net_name_offset > 0x14 {
            true => {
                reader.seek(io::SeekFrom::Start(device_name_offset as u64))?;
                Some(reader.read_windows_unicode_string()?)
            }
            false => None,
        };
        let device_name_unicode: Option<UnicodeString> = match net_name_offset > 0x14 {
            true => {
                reader.seek(io::SeekFrom::Start(device_name_offset as u64))?;
                Some(reader.read_windows_unicode_string()?)
            }
            false => None,
        };

        return Ok(CommonNetworkRelativeLink {
            common_network_relative_link_size,
            common_network_relative_link_flags,
            net_name_offset,
            device_name_offset,
            network_provider_type,
            net_name_offset_unicode,
            device_name_offset_unicode,
            net_name,
            device_name,
            net_name_unicode,
            device_name_unicode,
        });
    }
}

bitflags! {
    #[repr(C)]
    pub struct CommonNetworkRelativeLinkFlags : u32 {
        const ValidDevice = 1;
        const ValidNetType = 1 << 1;
    }
}

impl Serialize for CommonNetworkRelativeLinkFlags {
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

pub struct NetworkProviderType {
    value: u32,
}

impl NetworkProviderType {
    pub fn new(value: u32) -> Self {
        Self { value }
    }
}

impl Display for NetworkProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self.value {
            0x001A0000 => "WNNC_NET_AVID",
            0x001B0000 => "WNNC_NET_DOCUSPACE",
            0x001C0000 => "WNNC_NET_MANGOSOFT",
            0x001D0000 => "WNNC_NET_SERNET",
            0x001E0000 => "WNNC_NET_RIVERFRONT1",
            0x001F0000 => "WNNC_NET_RIVERFRONT2",
            0x00200000 => "WNNC_NET_DECORB",
            0x00210000 => "WNNC_NET_PROTSTOR",
            0x00220000 => "WNNC_NET_FJ_REDIR",
            0x00230000 => "WNNC_NET_DISTINCT",
            0x00240000 => "WNNC_NET_TWINS",
            0x00250000 => "WNNC_NET_RDR2SAMPLE",
            0x00260000 => "WNNC_NET_CSC",
            0x00270000 => "WNNC_NET_3IN1",
            0x00290000 => "WNNC_NET_EXTENDNET",
            0x002A0000 => "WNNC_NET_STAC",
            0x002B0000 => "WNNC_NET_FOXBAT",
            0x002C0000 => "WNNC_NET_YAHOO",
            0x002D0000 => "WNNC_NET_EXIFS",
            0x002E0000 => "WNNC_NET_DAV",
            0x002F0000 => "WNNC_NET_KNOWARE",
            0x00300000 => "WNNC_NET_OBJECT_DIRE",
            0x00310000 => "WNNC_NET_MASFAX",
            0x00320000 => "WNNC_NET_HOB_NFS",
            0x00330000 => "WNNC_NET_SHIVA",
            0x00340000 => "WNNC_NET_IBMAL",
            0x00350000 => "WNNC_NET_LOCK",
            0x00360000 => "WNNC_NET_TERMSRV",
            0x00370000 => "WNNC_NET_SRT",
            0x00380000 => "WNNC_NET_QUINCY",
            0x00390000 => "WNNC_NET_OPENAFS",
            0x003A0000 => "WNNC_NET_AVID1",
            0x003B0000 => "WNNC_NET_DFS",
            0x003C0000 => "WNNC_NET_KWNP",
            0x003D0000 => "WNNC_NET_ZENWORKS",
            0x003E0000 => "WNNC_NET_DRIVEONWEB",
            0x003F0000 => "WNNC_NET_VMWARE",
            0x00400000 => "WNNC_NET_RSFX",
            0x00410000 => "WNNC_NET_MFILES",
            0x00420000 => "WNNC_NET_MS_NFS",
            0x00430000 => "WNNC_NET_GOOGLE",
            _ => {
                return write!(f, "Invalid 0x{:0X}", self.value);
            }
        };
        return write!(f, "{result}");
    }
}

impl Serialize for NetworkProviderType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        return self.to_string().serialize(serializer);
    }
}
