use super::{
    CodePage, CodepageString, ReadVec, UnicodeString, shell_link_header::ReadShellLinkHeader,
};
use byteorder::{LE, ReadBytesExt};
use ctxutils::win32::GUID;
use serde::{Serialize, Serializer};
use std::io::{self, Cursor, Read, Seek};
use time::{OffsetDateTime, UtcOffset};

pub trait ReadLinkTargetIDList {
    fn read_lnk_link_target_id_list(
        &mut self,
        codepage: CodePage,
    ) -> Result<LinkTargetIDList, io::Error>;
    fn read_lnk_fat_datetime(&mut self) -> Result<FatDateTime, io::Error>;
}

impl<Reader: std::io::Read + Seek> ReadLinkTargetIDList for Reader {
    fn read_lnk_link_target_id_list(
        &mut self,
        codepage: CodePage,
    ) -> Result<LinkTargetIDList, io::Error> {
        let pos = self.stream_position()?;
        let size = self.read_u16::<LE>()? as usize + 2;
        self.seek(io::SeekFrom::Start(pos))?;
        let data = self.read_bytes(size)?;
        return LinkTargetIDList::from_slice(codepage, &data);
    }
    fn read_lnk_fat_datetime(&mut self) -> Result<FatDateTime, io::Error> {
        let result = FatDateTime {
            date: self.read_u16::<LE>()?,
            time: self.read_u16::<LE>()?,
        };
        return Ok(result);
    }
}

#[derive(Serialize)]
pub struct LinkTargetIDList {
    pub id_list_size: u16,
    pub id_list: IDList,
}

impl LinkTargetIDList {
    pub fn new(id_list_size: u16, id_list: IDList) -> Self {
        Self {
            id_list_size,
            id_list,
        }
    }

    pub fn from_slice(codepage: CodePage, slice: &[u8]) -> Result<LinkTargetIDList, io::Error> {
        let mut reader = Cursor::new(slice);
        let id_list_size = reader.read_u16::<LE>()?;
        let id_list =
            IDList::from_slice(codepage, &slice[reader.position() as usize..], id_list_size)?;
        return Ok(LinkTargetIDList::new(id_list_size, id_list));
    }
}

#[derive(Serialize)]
pub struct IDList {
    pub item_id_list: Vec<ShellItemID>,
}

impl IDList {
    pub fn new(item_id_list: Vec<ShellItemID>) -> Self {
        Self { item_id_list }
    }

    pub fn from_slice(
        codepage: CodePage,
        slice: &[u8],
        mut id_list_size: u16,
    ) -> Result<IDList, io::Error> {
        let mut reader = Cursor::new(slice);
        let mut vec = Vec::<ShellItemID>::new();
        while id_list_size > 2 {
            let item_id_size = reader.read_u16::<LE>()?;
            let data = reader.read_bytes(item_id_size as usize - 2)?.to_vec();
            vec.push(ShellItemID::from_slice(codepage, &data)?);
            id_list_size -= item_id_size;
        }
        if id_list_size != 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "LinkTargetIDList size mismatch",
            ));
        }
        let terminal_id = reader.read_u16::<LE>()?;
        if terminal_id != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid LinkTargetIDList::IDList::TerminalID",
            ));
        }
        return Ok(IDList::new(vec));
    }
}

#[derive(Serialize)]
pub enum ShellItemID {
    RootFolderItem(RootFolderItem),
    VolumeShellItem(VolumeShellItem),
    FileEntryShellItem(FileEntryShellItem),
    // NetworkLocationShellItem(NetworkLocationShellItem),
    // CompressedFolderShellItem(CompressedFolderShellItem),
    UriShellItem(UriShellItem),
    // ControlPanelShellItem(ControlPanelShellItem),
    Unsupported(BinaryBlob),
}

impl ShellItemID {
    pub fn from_slice(codepage: CodePage, data: &[u8]) -> Result<Self, io::Error> {
        let class = &data[0];
        let result = match class {
            0x1e..=0x1f => ShellItemID::RootFolderItem(RootFolderItem::from_slice(data)?),
            0x20..=0x2f => {
                ShellItemID::VolumeShellItem(VolumeShellItem::from_slice(codepage, data)?)
            }
            0x30..=0x3f => {
                ShellItemID::FileEntryShellItem(FileEntryShellItem::from_slice(codepage, data)?)
            }
            // 0x40..=0x4f => {
            //     SHITEMID::NetworkLocationShellItem(NetworkLocationShellItem::from_slice(data)?)
            // }
            // 0x52 => {
            //     SHITEMID::CompressedFolderShellItem(CompressedFolderShellItem::from_slice(data)?)
            // }
            0x61 => ShellItemID::UriShellItem(UriShellItem::from_slice(codepage, data)?),
            // 0x70..=0x71 => {
            //     SHITEMID::ControlPanelShellItem(ControlPanelShellItem::from_slice(data)?)
            // }
            _ => ShellItemID::Unsupported(BinaryBlob::new(data.to_vec())),
        };
        return Ok(result);
    }
}

struct ClassType {
    pub value: u8,
}

impl Serialize for ClassType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        return format!("0x{:02X}", self.value).serialize(serializer);
    }
}

pub struct Signature {
    pub value: u32,
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        return format!("0x{:08X}", self.value).serialize(serializer);
    }
}

#[derive(Serialize)]
pub struct RootFolderItem {
    class_type: ClassType,
    sort_index: u8,
    shell_folder_id: GUID,
    description: Option<String>,
    extension_block: Option<BinaryBlob>,
}

impl RootFolderItem {
    pub fn from_slice(data: &[u8]) -> Result<Self, io::Error> {
        let mut reader = Cursor::new(&data);
        let class_type = ClassType {
            value: reader.read_u8()?,
        };
        let sort_index = reader.read_u8()?;
        let shell_folder_id = reader.read_lnk_guid()?;
        let description = guid_to_description(&shell_folder_id);
        let extension_block = match data.len() {
            19..=usize::MAX => Some(BinaryBlob::new(data[18..].to_vec())),
            _ => None,
        };
        return Ok(Self {
            class_type,
            sort_index,
            shell_folder_id,
            description,
            extension_block,
        });
    }
}

#[derive(Serialize)]
pub struct VolumeShellItem {
    class_type: ClassType,
    flags: ClassType,
    name: Option<CodepageString>,
    blob: BinaryBlob,
}

impl VolumeShellItem {
    pub fn from_slice(codepage: CodePage, data: &[u8]) -> Result<Self, io::Error> {
        let mut reader = Cursor::new(&data);
        let class_type = ClassType {
            value: reader.read_u8()?,
        };
        let flags = ClassType {
            value: class_type.value & (0xFF ^ 0x70),
        };
        let name = match flags.value & 0x01 {
            0 => None,
            _ => Some(reader.read_windows_string(codepage)?),
        };
        return Ok(Self {
            class_type,
            flags,
            name,
            blob: BinaryBlob::new(data.to_vec()),
        });
    }
}

#[derive(Serialize)]
pub enum FileEntryName {
    #[serde(rename = "ANSI")]
    Ansi(CodepageString),
    Unicode(UnicodeString),
}

#[derive(Serialize)]
pub struct FileEntryShellItem {
    class_type: ClassType,
    flags: ClassType,
    file_size: u32,
    modification_time: FatDateTime,
    file_attributes: u16,
    primary_name: FileEntryName,
    secondary_name: Option<FileEntryName>,
    shell_folder_identifier: Option<GUID>,
    extension: Vec<ExtensionBlock>,
}

pub struct FatDateTime {
    date: u16,
    time: u16,
}

impl FatDateTime {
    pub fn to_datetime(&self) -> Option<OffsetDateTime> {
        let year = ((self.date & 0xFE00) >> 9) as i32 + 1980;
        let month = ((self.date & 0x01E0) >> 5) as u8;
        let day = (self.date & 0x1F) as u8;
        let hour = ((self.time & 0xF800) >> 11) as u8;
        let min = ((self.time & 0x07E0) >> 5) as u8;
        let sec = 2 * (self.time & 0x1F) as u8;

        let month = time::Month::December.nth_next(month);
        let date = time::Date::from_calendar_date(year, month, day).ok()?;
        let time = time::Time::from_hms(hour, min, sec).ok()?;

        Some(OffsetDateTime::new_in_offset(date, time, UtcOffset::UTC))
    }
}

impl Serialize for FatDateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let datetime = self.to_datetime();
        match datetime {
            Some(datetime) => return datetime.to_string().serialize(serializer),
            None => return "(UNSET)".serialize(serializer),
        }
    }
}

fn is_pre_xp_shell_item(data: &[u8], pos: u64) -> Result<bool, io::Error> {
    let mut reader = Cursor::new(data);
    reader.seek(io::SeekFrom::End(-2))?;
    let offset = reader.read_u16::<LE>()?;
    if (offset as u64) < pos || (offset as usize) + 6 >= data.len() {
        return Ok(true);
    }
    if reader.seek(io::SeekFrom::Start(offset as u64)).is_err() {
        return Ok(true);
    }
    let _version = reader.read_u16::<LE>()?;
    let _signature_high = reader.read_u16::<LE>()?;
    let signature_low = reader.read_u16::<LE>()?;
    if signature_low != 0xbeef {
        return Ok(true);
    }
    return Ok(false);
}

#[derive(Serialize)]
pub enum ExtensionBlock {
    ExtensionBlock0004(ExtensionBlock0004),
    ExtensionBlockUnsupported(ExtensionBlockUnsupported),
}

impl ExtensionBlock {
    pub fn from_cursor(
        codepage: CodePage,
        reader: &mut Cursor<&[u8]>,
    ) -> Result<Option<ExtensionBlock>, io::Error> {
        let start_pos = reader.position();
        reader.seek(io::SeekFrom::End(0))?;
        if reader.position() == start_pos {
            return Ok(None);
        }
        reader.seek(io::SeekFrom::Start(start_pos))?;
        let size = reader.read_u16::<LE>()?;
        let version = reader.read_u16::<LE>()?;
        let signature = Signature {
            value: reader.read_u32::<LE>()?,
        };
        let data_size = size as usize - (reader.position() - start_pos) as usize - 2;
        let data = reader.read_bytes(data_size)?;
        let first_extension_version_offset = reader.read_u16::<LE>()?;

        let result = match signature.value {
            0xbeef0004 => ExtensionBlock0004::from_data(
                codepage,
                size,
                version,
                signature,
                data.as_slice(),
                first_extension_version_offset,
            )?,
            _ => ExtensionBlockUnsupported::from_data(
                size,
                version,
                signature,
                data.as_slice(),
                first_extension_version_offset,
            )?,
        };
        return Ok(Some(result));
    }
}

#[derive(Serialize)]
pub struct ExtensionBlock0004 {
    size: u16,
    version: u16,
    signature: Signature,
    creation_time: FatDateTime,
    access_time: FatDateTime,
    windows_version: u16,
    unknown0: Option<BinaryBlob>,
    file_reference: Option<u64>,
    unknown1: Option<BinaryBlob>,
    localized_name_size: Option<u16>,
    unknown2: Option<BinaryBlob>,
    unknown3: Option<BinaryBlob>,
    long_name_unicode: Option<UnicodeString>,
    localized_name: Option<CodepageString>,
    localized_name_unicode: Option<UnicodeString>,
    first_extension_version_offset: u16,
}

impl ExtensionBlock0004 {
    pub fn from_data(
        codepage: CodePage,
        size: u16,
        version: u16,
        signature: Signature,
        data: &[u8],
        first_extension_version_offset: u16,
    ) -> Result<ExtensionBlock, io::Error> {
        let mut reader = Cursor::new(data);
        let creation_time = reader.read_lnk_fat_datetime()?;
        let access_time = reader.read_lnk_fat_datetime()?;
        let windows_version = reader.read_u16::<LE>()?;

        let (unknown0, file_reference, unknown1) = match version >= 7 {
            true => (
                Some(BinaryBlob::from_reader(&mut reader, 2)?),
                Some(reader.read_u64::<LE>()?),
                Some(BinaryBlob::from_reader(&mut reader, 8)?),
            ),
            false => (None, None, None),
        };
        let localized_name_size = match version >= 3 {
            true => Some(reader.read_u16::<LE>()?),
            false => None,
        };
        let unknown2 = match version >= 9 {
            true => Some(BinaryBlob::from_reader(&mut reader, 4)?),
            false => None,
        };
        let unknown3 = match version >= 8 {
            true => Some(BinaryBlob::from_reader(&mut reader, 4)?),
            false => None,
        };
        let long_name_unicode = match version >= 3 {
            true => Some(reader.read_windows_unicode_string()?),
            false => None,
        };
        let localized_name = match version >= 3 && localized_name_size.unwrap() > 0 {
            true => Some(reader.read_windows_string(codepage)?),
            false => None,
        };
        let localized_name_unicode = match version >= 7 && localized_name_size.unwrap() > 0 {
            true => Some(reader.read_windows_unicode_string()?),
            false => None,
        };
        return Ok(ExtensionBlock::ExtensionBlock0004(ExtensionBlock0004 {
            size,
            version,
            signature,
            creation_time,
            access_time,
            windows_version,
            unknown0,
            file_reference,
            unknown1,
            localized_name_size,
            unknown2,
            unknown3,
            long_name_unicode,
            localized_name,
            localized_name_unicode,
            first_extension_version_offset,
        }));
    }
}

#[derive(Serialize)]
pub struct ExtensionBlockUnsupported {
    size: u16,
    version: u16,
    signature: Signature,
    data: BinaryBlob,
    first_extension_version_offset: u16,
}

impl ExtensionBlockUnsupported {
    pub fn from_data(
        size: u16,
        version: u16,
        signature: Signature,
        data: &[u8],
        first_extension_version_offset: u16,
    ) -> Result<ExtensionBlock, io::Error> {
        return Ok(ExtensionBlock::ExtensionBlockUnsupported(
            ExtensionBlockUnsupported {
                size,
                version,
                signature,
                data: BinaryBlob::new(data.to_vec()),
                first_extension_version_offset,
            },
        ));
    }
}

impl FileEntryShellItem {
    pub fn from_slice(codepage: CodePage, data: &[u8]) -> Result<Self, io::Error> {
        let mut reader = Cursor::new(data);
        let class_type = ClassType {
            value: reader.read_u8()?,
        };
        let flags = ClassType {
            value: class_type.value & (0xFF ^ 0x70),
        };
        // let is_directory = flags.value & 0x01 != 0;
        // let is_file = flags.value & 0x02 != 0;
        let has_unicode = flags.value & 0x04 != 0;
        let has_clsid = flags.value & 0x80 != 0;

        let _padding = reader.read_u8()?;
        let file_size = reader.read_u32::<LE>()?;
        let modification_time = reader.read_lnk_fat_datetime()?;
        let file_attributes = reader.read_u16::<LE>()?;
        let primary_name = match has_unicode {
            true => FileEntryName::Unicode(reader.read_windows_unicode_string()?),
            false => FileEntryName::Ansi(reader.read_windows_string(codepage)?),
        };
        if let FileEntryName::Ansi(ansi_name) = &primary_name {
            if ansi_name.data.len() % 2 == 0 {
                reader.read_u8()?;
            }
        }

        let pre_xp = is_pre_xp_shell_item(data, reader.position())?;
        let secondary_name: Option<FileEntryName>;
        let shell_folder_identifier: Option<GUID>;
        let mut extension = Vec::<ExtensionBlock>::new();
        if !pre_xp {
            let name = match has_unicode {
                true => FileEntryName::Unicode(reader.read_windows_unicode_string()?),
                false => FileEntryName::Ansi(reader.read_windows_string(codepage)?),
            };
            if let FileEntryName::Ansi(ansi_name) = &name {
                if ansi_name.data.len() % 2 == 0 {
                    reader.read_u8()?;
                }
            }
            secondary_name = Some(name);
            if has_clsid {
                shell_folder_identifier = Some(reader.read_lnk_guid()?);
            } else {
                shell_folder_identifier = None;
            }
        } else {
            secondary_name = None;
            shell_folder_identifier = None;
            loop {
                let extension_block = ExtensionBlock::from_cursor(codepage, &mut reader)?;
                if extension_block.is_none() {
                    break;
                }
                extension.push(extension_block.unwrap());
            }
        }

        return Ok(Self {
            class_type,
            flags,
            file_size,
            modification_time,
            file_attributes,
            primary_name,
            secondary_name,
            shell_folder_identifier,
            extension,
        });
    }
}

#[derive(Serialize)]
pub struct UriShellItem {
    class_type: ClassType,
    flags: ClassType,
    unknown1: u32,
    //data: Option<UriShellItemData>,
    uri_string: FileEntryName,
    unknown2: BinaryBlob,
}
impl UriShellItem {
    pub fn from_slice(codepage: CodePage, input: &[u8]) -> Result<Self, io::Error> {
        let mut reader = Cursor::new(input);
        let class_type = ClassType {
            value: reader.read_u8()?,
        };
        let flags = ClassType {
            value: reader.read_u8()?,
        };
        let has_unicode = flags.value & 0x80 != 0;
        let unknown1 = reader.read_u32::<LE>()?;
        if unknown1 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsupported UriShellItem structure",
            ));
        }
        // let data = match data_size > 0 {
        //     true => {
        //         let slice = reader.read_bytes(data_size as usize)?;
        //         Some(UriShellItemData::from_slice(&slice, has_unicode)?)
        //     }
        //     false => None,
        // };
        let uri_string = match has_unicode {
            true => FileEntryName::Unicode(reader.read_windows_unicode_string()?),
            false => FileEntryName::Ansi(reader.read_windows_string(codepage)?),
        };
        let blob_size = input.len() - reader.position() as usize;
        let unknown2 = match blob_size > 0 {
            true => BinaryBlob {
                data: reader.read_bytes(blob_size)?,
            },
            false => BinaryBlob {
                data: Vec::<u8>::new(),
            },
        };

        return Ok(Self {
            class_type,
            flags,
            unknown1,
            uri_string,
            unknown2,
        });
    }
}

pub struct BinaryBlob {
    pub data: Vec<u8>,
}

impl BinaryBlob {
    pub fn new(data: Vec<u8>) -> Self {
        return Self { data };
    }
    pub fn from_reader<R: Read>(reader: &mut R, size: usize) -> Result<Self, io::Error> {
        return Ok(Self {
            data: reader.read_bytes(size)?,
        });
    }
}

impl Serialize for BinaryBlob {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut result = "0x".to_string();
        for byte in &self.data {
            result.push_str(format!("{:02X}", byte).as_str());
        }
        return result.serialize(serializer);
    }
}

/// function returns description of known GUID
/// Based on https://github.com/libyal/libfwsi/wiki/Shell-Folder-identifiers
fn guid_to_description(guid: &GUID) -> Option<String> {
    let str = guid.to_string().to_lowercase();
    let result = match str.as_str() {
        "00020d75-0000-0000-c000-000000000046" => Some("Inbox"),
        "00020d76-0000-0000-c000-000000000046" => Some("Inbox"),
        "00c6d95f-329c-409a-81d7-c46c66ea7f33" => Some("Default Location"),
        "0142e4d0-fb7a-11dc-ba4a-000ffe7ab428" => Some("Biometric Devices (Biometrics)"),
        "025a5937-a6be-4686-a844-36fe4bec8b6d" => Some("Power Options"),
        "031e4825-7b94-4dc3-b131-e946b44c8dd5" => Some("Users Libraries"),
        "04731b67-d933-450a-90e6-4acd2e9408fe" => Some("Search Folder"),
        "05d7b0f4-2121-4eff-bf6b-ed3f69b894d9" => Some("Taskbar (Notification Area Icons)"),
        "0875dcb6-c686-4243-9432-adccf0b9f2d7" => {
            Some("Microsoft !OneNote Namespace Extension for Windows Desktop Search")
        }
        "0afaced1-e828-11d1-9187-b532f1e9575d" => Some("Folder Shortcut"),
        "0bd8e793-d371-11d1-b0b5-0060972919d7" => Some("!SolidWorks Enterprise PDM"),
        "0cd7a5c0-9f37-11ce-ae65-08002b2e1262" => Some("Cabinet File"),
        "0df44eaa-ff21-4412-828e-260a8728e7f1" => Some("Taskbar and Start Menu"),
        "11016101-e366-4d22-bc06-4ada335c892b" => {
            Some("Internet Explorer History and Feeds Shell Data Source for Windows Search")
        }
        "1206f5f1-0569-412c-8fec-3204630dfb70" => Some("Credential Manager"),
        "13e7f612-f261-4391-bea2-39df4f3fa311" => Some("Windows Desktop Search"),
        "15eae92e-f17a-4431-9f28-805e482dafd4" => Some("Install New Programs (Get Programs)"),
        "1723d66a-7a12-443e-88c7-05e1bfe79983" => Some("Previous Versions Delegate Folder"),
        "17cd9488-1228-4b2f-88ce-4298e93e0966" => Some("Default Programs (Set User Defaults)"),
        "1a9ba3a0-143a-11cf-8350-444553540000" => Some("Shell Favorite Folder"),
        "1d2680c9-0e2a-469d-b787-065558bc7d43" => Some("Fusion Cache"),
        "1f3427c8-5c10-4210-aa03-2ee45287d668" => Some("User Pinned"),
        "1f43a58c-ea28-43e6-9ec4-34574a16ebb7" => {
            Some("Windows Desktop Search MAPI Namespace Extension Class")
        }
        "1f4de370-d627-11d1-ba4f-00a0c91eedba" => {
            Some("Search Results - Computers (Computer Search Results Folder, Network Computers)")
        }
        "1fa9085f-25a2-489b-85d4-86326eedcd87" => Some("Manage Wireless Networks"),
        "208d2c60-3aea-1069-a2d7-08002b30309d" => Some("My Network Places (Network)"),
        "20d04fe0-3aea-1069-a2d8-08002b30309d" => Some("My Computer (Computer)"),
        "21ec2020-3aea-1069-a2dd-08002b30309d" => Some("Control Panel"),
        "2227a280-3aea-1069-a2de-08002b30309d" => Some("Printers and Faxes (Printers)"),
        "241d7c96-f8bf-4f85-b01f-e2b043341a4b" => {
            Some("Workspaces Center (Remote Application and Desktop Connections)")
        }
        "2559a1f0-21d7-11d4-bdaf-00c04f60b9f0" => Some("Search"),
        "2559a1f1-21d7-11d4-bdaf-00c04f60b9f0" => Some("Help and Support"),
        "2559a1f2-21d7-11d4-bdaf-00c04f60b9f0" => Some("Windows Security"),
        "2559a1f3-21d7-11d4-bdaf-00c04f60b9f0" => Some("Run..."),
        "2559a1f4-21d7-11d4-bdaf-00c04f60b9f0" => Some("Internet"),
        "2559a1f5-21d7-11d4-bdaf-00c04f60b9f0" => Some("E-mail"),
        "2559a1f7-21d7-11d4-bdaf-00c04f60b9f0" => Some("Set Program Access and Defaults"),
        "267cf8a9-f4e3-41e6-95b1-af881be130ff" => Some("Location Folder"),
        "26ee0668-a00a-44d7-9371-beb064c98683" => Some("Control Panel"),
        "2728520d-1ec8-4c68-a551-316b684c4ea7" => Some("Network Setup Wizard"),
        "28803f59-3a75-4058-995f-4ee5503b023c" => Some("Bluetooth Devices"),
        "289978ac-a101-4341-a817-21eba7fd046d" => Some("Sync Center Conflict Folder"),
        "289af617-1cc3-42a6-926c-e6a863f0e3ba" => Some("DLNA Media Servers Data Source"),
        "2965e715-eb66-4719-b53f-1672673bbefa" => Some("Results Folder"),
        "2e9e59c0-b437-4981-a647-9c34b9b90891" => Some("Sync Setup Folder"),
        "2f6ce85c-f9ee-43ca-90c7-8a9bd53a2467" => Some("File History Data Source"),
        "3080f90d-d7ad-11d9-bd98-0000947b0257" => Some("Show Desktop"),
        "3080f90e-d7ad-11d9-bd98-0000947b0257" => Some("Window Switcher"),
        "323ca680-c24d-4099-b94d-446dd2d7249e" => Some("Common Places"),
        "328b0346-7eaf-4bbe-a479-7cb88a095f5b" => Some("Layout Folder"),
        "335a31dd-f04b-4d76-a925-d6b47cf360df" => Some("Backup and Restore Center"),
        "35786d3c-b075-49b9-88dd-029876e11c01" => Some("Portable Devices"),
        "36eef7db-88ad-4e81-ad49-0e313f0c35f8" => Some("Windows Update"),
        "3c5c43a3-9ce9-4a9b-9699-2ac0cf6cc4bf" => Some("Configure Wireless Network"),
        "3f6bc534-dfa1-4ab4-ae54-ef25a74e0107" => Some("System Restore"),
        "4026492f-2f69-46b8-b9bf-5654fc07e423" => Some("Windows Firewall"),
        "418c8b64-5463-461d-88e0-75e2afa3c6fa" => Some("Explorer Browser Results Folder"),
        "4234d49b-0245-4df3-b780-3893943456e1" => Some("Applications"),
        "437ff9c0-a07f-4fa0-af80-84b6c6440a16" => Some("Command Folder"),
        "450d8fba-ad25-11d0-98a8-0800361b1103" => Some("My Documents"),
        "48e7caab-b918-4e58-a94d-505519c795dc" => Some("Start Menu Folder"),
        "5399e694-6ce5-4d6c-8fce-1d8870fdcba0" => {
            Some("Control Panel command object for Start menu and desktop")
        }
        "58e3c745-d971-4081-9034-86e34b30836a" => Some("Speech Recognition Options"),
        "59031a47-3f72-44a7-89c5-5595fe6b30ee" => Some("Shared Documents Folder (Users Files)"),
        "5ea4f148-308c-46d7-98a9-49041b1dd468" => Some("Mobility Center Control Panel"),
        "60632754-c523-4b62-b45c-4172da012619" => Some("User Accounts"),
        "63da6ec0-2e98-11cf-8d82-444553540000" => Some("Microsoft FTP Folder"),
        "640167b4-59b0-47a6-b335-a6b3c0695aea" => Some("Portable Media Devices"),
        "645ff040-5081-101b-9f08-00aa002f954e" => Some("Recycle Bin"),
        "67718415-c450-4f3c-bf8a-b487642dc39b" => Some("Windows Features"),
        "6785bfac-9d2d-4be5-b7e2-59937e8fb80a" => Some("Other Users Folder"),
        "67ca7650-96e6-4fdd-bb43-a8e774f73a57" => Some("Home Group Control Panel (Home Group)"),
        "692f0339-cbaa-47e6-b5b5-3b84db604e87" => Some("Extensions Manager Folder"),
        "6dfd7c5c-2451-11d3-a299-00c04f8ef6af" => Some("Folder Options"),
        "7007acc7-3202-11d1-aad2-00805fc1270e" => {
            Some("Network Connections (Network and Dial-up Connections)")
        }
        "708e1662-b832-42a8-bbe1-0a77121e3908" => Some("Tree property value folder"),
        "71d99464-3b6b-475c-b241-e15883207529" => Some("Sync Results Folder"),
        "72b36e70-8700-42d6-a7f7-c9ab3323ee51" => Some("Search Connector Folder"),
        "78f3955e-3b90-4184-bd14-5397c15f1efc" => Some("Performance Information and Tools"),
        "7a9d77bd-5403-11d2-8785-2e0420524153" => Some("User Accounts (Users and Passwords)"),
        "7b81be6a-ce2b-4676-a29e-eb907a5126c5" => Some("Programs and Features"),
        "7bd29e00-76c1-11cf-9dd0-00a0c9034933" => Some("Temporary Internet Files"),
        "7bd29e01-76c1-11cf-9dd0-00a0c9034933" => Some("Temporary Internet Files"),
        "7be9d83c-a729-4d97-b5a7-1b7313c39e0a" => Some("Programs Folder"),
        "8060b2e3-c9d7-4a5d-8c6b-ce8eba111328" => Some("Proximity CPL"),
        "8343457c-8703-410f-ba8b-8b026e431743" => Some("Feedback Tool"),
        "85bbd920-42a0-1069-a2e4-08002b30309d" => Some("Briefcase"),
        "863aa9fd-42df-457b-8e4d-0de1b8015c60" => Some("Remote Printers"),
        "865e5e76-ad83-4dca-a109-50dc2113ce9a" => Some("Programs Folder and Fast Items"),
        "871c5380-42a0-1069-a2ea-08002b30309d" => Some("Internet Explorer (Homepage)"),
        "87630419-6216-4ff8-a1f0-143562d16d5c" => Some("Mobile Broadband Profile Settings Editor"),
        "877ca5ac-cb41-4842-9c69-9136e42d47e2" => Some("File Backup Index"),
        "88c6c381-2e85-11d0-94de-444553540000" => Some("ActiveX Cache Folder"),
        "896664f7-12e1-490f-8782-c0835afd98fc" => {
            Some("Libraries delegate folder that appears in Users Files Folder")
        }
        "8e908fc9-becc-40f6-915b-f4ca0e70d03d" => Some("Network and Sharing Center"),
        "8fd8b88d-30e1-4f25-ac2b-553d3d65f0ea" => Some("DXP"),
        "9113a02d-00a3-46b9-bc5f-9c04daddd5d7" => Some("Enhanced Storage Data Source"),
        "93412589-74d4-4e4e-ad0e-e0cb621440fd" => Some("Font Settings"),
        "9343812e-1c37-4a49-a12e-4b2d810d956b" => Some("Search Home"),
        "96437431-5a90-4658-a77c-25478734f03e" => Some("Server Manager"),
        "96ae8d84-a250-4520-95a5-a47a7e3c548b" => Some("Parental Controls"),
        "98d99750-0b8a-4c59-9151-589053683d73" => {
            Some("Windows Search Service Media Center Namespace Extension Handler")
        }
        "992cffa0-f557-101a-88ec-00dd010ccc48" => {
            Some("Network Connections (Network and Dial-up Connections)")
        }
        "9a096bb5-9dc3-4d1c-8526-c3cbf991ea4e" => Some("Internet Explorer RSS Feeds Folder"),
        "9c60de1e-e5fc-40f4-a487-460851a8d915" => Some("AutoPlay"),
        "9c73f5e5-7ae7-4e32-a8e8-8d23b85255bf" => Some("Sync Center Folder"),
        "9db7a13c-f208-4981-8353-73cc61ae2783" => Some("Previous Versions"),
        "9f433b7c-5f96-4ce1-ac28-aeaa1cc04d7c" => Some("Security Center"),
        "9fe63afd-59cf-4419-9775-abcc3849f861" => Some("System Recovery (Recovery)"),
        "a3c3d402-e56c-4033-95f7-4885e80b0111" => Some("Previous Versions Results Delegate Folder"),
        "a5a3563a-5755-4a6f-854e-afa3230b199f" => Some("Library Folder"),
        "a5e46e3a-8849-11d1-9d8c-00c04fc99d61" => Some("Microsoft Browser Architecture"),
        "a6482830-08eb-41e2-84c1-73920c2badb9" => Some("Removable Storage Devices"),
        "a8a91a66-3a7d-4424-8d24-04e180695c7a" => Some("Device Center (Devices and Printers)"),
        "aee2420f-d50e-405c-8784-363c582bf45a" => Some("Device Pairing Folder"),
        "afdb1f70-2a4c-11d2-9039-00c04f8eeb3e" => Some("Offline Files Folder"),
        "b155bdf8-02f0-451e-9a26-ae317cfd7779" => {
            Some("Nethood delegate folder (Delegate folder that appears in Computer)")
        }
        "b2952b16-0e07-4e5a-b993-58c52cb94cae" => Some("DB Folder"),
        "b4fb3f98-c1ea-428d-a78a-d1f5659cba93" => Some("Other Users Folder"),
        "b98a2bea-7d42-4558-8bd1-832f41bac6fd" => {
            Some("Backup And Restore (Backup and Restore Center)")
        }
        "bb06c0e4-d293-4f75-8a90-cb05b6477eee" => Some("System"),
        "bb64f8a7-bee7-4e1a-ab8d-7d8273f7fdb6" => Some("Action Center Control Panel"),
        "bc476f4c-d9d7-4100-8d4e-e043f6dec409" => Some("Microsoft Browser Architecture"),
        "bc48b32f-5910-47f5-8570-5074a8a5636a" => Some("Sync Results Delegate Folder"),
        "bd84b380-8ca2-1069-ab1d-08000948f534" => Some("Microsoft Windows Font Folder"),
        "bdeadf00-c265-11d0-bced-00a0c90ab50f" => Some("Web Folders"),
        "be122a0e-4503-11da-8bde-f66bad1e3f3a" => Some("Windows Anytime Upgrade"),
        "bf782cc9-5a52-4a17-806c-2a894ffeeac5" => Some("Language Settings"),
        "c291a080-b400-4e34-ae3f-3d2b9637d56c" => Some("UNCFATShellFolder Class"),
        "c2b136e2-d50e-405c-8784-363c582bf43e" => Some("Device Center Initialization"),
        "c555438b-3c23-4769-a71f-b6d3d9b6053a" => Some("Display"),
        "c57a6066-66a3-4d91-9eb9-41532179f0a5" => Some("Application Suggested Locations"),
        "c58c4893-3be0-4b45-abb5-a63e4b8c8651" => Some("Troubleshooting"),
        "cb1b7f8c-c50a-4176-b604-9e24dee8d4d1" => Some("Welcome Center (Getting Started)"),
        "d2035edf-75cb-4ef1-95a7-410d9ee17170" => Some("DLNA Content Directory Data Source"),
        "d20ea4e1-3957-11d2-a40b-0c5020524152" => Some("Fonts"),
        "d20ea4e1-3957-11d2-a40b-0c5020524153" => Some("Administrative Tools"),
        "d34a6ca6-62c2-4c34-8a7c-14709c1ad938" => Some("Common Places FS Folder"),
        "d426cfd0-87fc-4906-98d9-a23f5d515d61" => {
            Some("Windows Search Service Outlook Express Protocol Handler")
        }
        "d4480a50-ba28-11d1-8e75-00c04fa31a86" => Some("Add Network Place"),
        "d450a8a1-9568-45c7-9c0e-b4f9fb4537bd" => Some("Installed Updates"),
        "d555645e-d4f8-4c29-a827-d93c859c4f2a" => Some("Ease of Access (Ease of Access Center)"),
        "d5b1944e-db4e-482e-b3f1-db05827f0978" => Some("Softex OmniPass Encrypted Folder"),
        "d6277990-4c6a-11cf-8d87-00aa0060f5bf" => Some("Scheduled Tasks"),
        "d8559eb9-20c0-410e-beda-7ed416aecc2a" => Some("Windows Defender"),
        "d9ef8727-cac2-4e60-809e-86f80a666c91" => {
            Some("Secure Startup (BitLocker Drive Encryption)")
        }
        "dffacdc5-679f-4156-8947-c5c76bc0b67f" => {
            Some("Delegate folder that appears in Users Files Folder")
        }
        "e17d4fc0-5564-11d1-83f2-00a0c90dc849" => Some("Search Results Folder"),
        "e211b736-43fd-11d1-9efb-0000f8757fcd" => Some("Scanners and Cameras"),
        "e413d040-6788-4c22-957e-175d1c513a34" => Some("Sync Center Conflict Delegate Folder"),
        "e773f1af-3a65-4866-857d-846fc9c4598a" => Some("Shell Storage Folder Viewer"),
        "e7de9b1a-7533-4556-9484-b26fb486475e" => Some("Network Map"),
        "e7e4bc40-e76a-11ce-a9bb-00aa004ae837" => Some("Shell DocObject Viewer"),
        "e88dcce0-b7b3-11d1-a9f0-00aa0060fa31" => Some("Compressed Folder"),
        "e95a4861-d57a-4be1-ad0f-35267e261739" => Some("Windows SideShow"),
        "e9950154-c418-419e-a90a-20c5287ae24b" => Some("Sensors (Location and Other Sensors)"),
        "ed228fdf-9ea8-4870-83b1-96b02cfe0d52" => Some("My Games (Games Explorer)"),
        "ed50fc29-b964-48a9-afb3-15ebb9b97f36" => Some("PrintHood delegate folder"),
        "ed7ba470-8e54-465e-825c-99712043e01c" => Some("All Tasks"),
        "ed834ed6-4b5a-4bfe-8f11-a626dcb6a921" => Some("Personalization Control Panel"),
        "edc978d6-4d53-4b2f-a265-5805674be568" => Some("Stream Backed Folder"),
        "f02c1a0d-be21-4350-88b0-7367fc96ef3c" => Some("Computers and Devices"),
        "f1390a9a-a3f4-4e5d-9c5f-98f3bd8d935c" => Some("Sync Setup Delegate Folder"),
        "f3f5824c-ad58-4728-af59-a1ebe3392799" => {
            Some("Sticky Notes Namespace Extension for Windows Desktop Search")
        }
        "f5175861-2688-11d0-9c5e-00aa00a45957" => Some("Subscription Folder"),
        "f6b6e965-e9b2-444b-9286-10c9152edbc5" => Some("History Vault"),
        "f8c2ab3b-17bc-41da-9758-339d7dbf2d88" => Some("Previous Versions Results Folder"),
        "f90c627b-7280-45db-bc26-cce7bdd620a4" => Some("All Tasks"),
        "f942c606-0914-47ab-be56-1321b8035096" => Some("Storage Spaces"),
        "fb0c9c8a-6c50-11d1-9f1d-0000f8757fcd" => Some("Scanners & Cameras"),
        "fbf23b42-e3f0-101b-8488-00aa003e56f8" => Some("Internet Explorer"),
        "fe1290f0-cfbd-11cf-a330-00aa00c16e65" => Some("Directory"),
        "ff393560-c2a7-11cf-bff4-444553540000" => Some("History"),
        _ => None,
    };

    if let Some(value) = result {
        return Some(value.to_string());
    } else {
        return None;
    }
}
