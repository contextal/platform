use super::{
    CodePage, CodepageString, ReadVec, UnicodeString, link_target_id_list::IDList,
    shell_link_header::ReadShellLinkHeader,
};
use bitflags::bitflags;
use byteorder::{LE, ReadBytesExt};
use ctxole::oleps::{FromOlepsReader, TypedPropertyValue};
use ctxutils::win32::GUID;
use serde::{Serialize, Serializer, ser::SerializeStruct};
use std::{
    io,
    io::{Cursor, Seek},
};

pub trait ReadExtraData {
    fn read_lnk_extra_data_vec(&mut self, codepage: CodePage) -> Result<Vec<ExtraData>, io::Error>;
}

impl<Reader: std::io::Read + Seek> ReadExtraData for Reader {
    fn read_lnk_extra_data_vec(&mut self, codepage: CodePage) -> Result<Vec<ExtraData>, io::Error> {
        let mut extra_data = Vec::<ExtraData>::new();
        loop {
            let pos = self.stream_position()?;
            let block_size = self.read_u32::<LE>()?;
            if block_size < 4 {
                break;
            }
            let block_signature = self.read_u32::<LE>()?;
            self.seek(io::SeekFrom::Start(pos))?;
            let data = self.read_bytes(block_size as usize)?;
            let block = ExtraData::from_slice(codepage, block_signature, &data)?;
            extra_data.push(block);
        }
        return Ok(extra_data);
    }
}

#[derive(Serialize)]
pub enum ExtraData {
    ConsoleDataBlock(ConsoleDataBlock),
    ConsoleFEDataBlock(ConsoleFEDataBlock),
    DarwinDataBlock(DarwinDataBlock),
    EnvironmentVariableDataBlock(EnvironmentVariableDataBlock),
    IconEnvironmentDataBlock(IconEnvironmentDataBlock),
    KnownFolderDataBlock(KnownFolderDataBlock),
    PropertyStoreDataBlock(PropertyStoreDataBlock),
    ShimDataBlock(ShimDataBlock),
    SpecialFolderDataBlock(SpecialFolderDataBlock),
    TrackerDataBlock(TrackerDataBlock),
    VistaAndAboveIDListDataBlock(VistaAndAboveIDListDataBlock),
    Unsupported(UnsupportedBlock),
}

impl ExtraData {
    pub fn from_slice(
        codepage: CodePage,
        block_signature: u32,
        slice: &[u8],
    ) -> Result<ExtraData, io::Error> {
        let block: ExtraData = match block_signature {
            0xA0000002 => ExtraData::ConsoleDataBlock(ConsoleDataBlock::from_slice(slice)?),
            0xA0000004 => ExtraData::ConsoleFEDataBlock(ConsoleFEDataBlock::from_slice(slice)?),
            0xA0000006 => ExtraData::DarwinDataBlock(DarwinDataBlock::from_slice(codepage, slice)?),
            0xA0000001 => ExtraData::EnvironmentVariableDataBlock(
                EnvironmentVariableDataBlock::from_slice(codepage, slice)?,
            ),
            0xA0000007 => ExtraData::IconEnvironmentDataBlock(
                IconEnvironmentDataBlock::from_slice(codepage, slice)?,
            ),
            0xA000000B => ExtraData::KnownFolderDataBlock(KnownFolderDataBlock::from_slice(slice)?),
            0xA0000009 => ExtraData::PropertyStoreDataBlock(PropertyStoreDataBlock::from_slice(
                slice, codepage,
            )?),
            0xA0000008 => ExtraData::ShimDataBlock(ShimDataBlock::from_slice(slice)?),
            0xA0000005 => {
                ExtraData::SpecialFolderDataBlock(SpecialFolderDataBlock::from_slice(slice)?)
            }
            0xA0000003 => {
                ExtraData::TrackerDataBlock(TrackerDataBlock::from_slice(codepage, slice)?)
            }
            0xA000000C => ExtraData::VistaAndAboveIDListDataBlock(
                VistaAndAboveIDListDataBlock::from_slice(codepage, slice)?,
            ),
            _ => ExtraData::Unsupported(UnsupportedBlock::from_slice(slice)?),
        };

        return Ok(block);
    }
}

#[derive(Serialize)]
pub struct ConsoleDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub fill_attributes: FillAttributes,
    pub popup_fill_attributes: FillAttributes,
    pub screen_buffer_size_x: i16,
    pub screen_buffer_size_y: i16,
    pub window_size_x: i16,
    pub window_size_y: i16,
    pub window_origin_x: i16,
    pub window_origin_y: i16,
    pub unused1: u32,
    pub unused2: u32,
    pub font_size: u32,
    pub font_family: FontFamily,
    pub font_weight: u32,
    pub face_name: UnicodeString,
    pub cursor_size: u32,
    pub full_screen: u32,
    pub quick_edit: u32,
    pub insert_mode: u32,
    pub auto_position: u32,
    pub history_buffer_size: u32,
    pub number_of_history_buffers: u32,
    pub history_no_dup: u32,
    pub color_table: [u32; 16],
}

impl ConsoleDataBlock {
    pub fn from_slice(slice: &[u8]) -> Result<ConsoleDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size != 0xCC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000002 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        return Ok(ConsoleDataBlock {
            block_size,
            block_signature,
            fill_attributes: FillAttributes::from_bits_retain(reader.read_u16::<LE>()?),
            popup_fill_attributes: FillAttributes::from_bits_retain(reader.read_u16::<LE>()?),
            screen_buffer_size_x: reader.read_i16::<LE>()?,
            screen_buffer_size_y: reader.read_i16::<LE>()?,
            window_size_x: reader.read_i16::<LE>()?,
            window_size_y: reader.read_i16::<LE>()?,
            window_origin_x: reader.read_i16::<LE>()?,
            window_origin_y: reader.read_i16::<LE>()?,
            unused1: reader.read_u32::<LE>()?,
            unused2: reader.read_u32::<LE>()?,
            font_size: reader.read_u32::<LE>()?,
            font_family: FontFamily::new(reader.read_u32::<LE>()?),
            font_weight: reader.read_u32::<LE>()?,
            face_name: reader.read_windows_unicode_string_with_size(64)?,
            cursor_size: reader.read_u32::<LE>()?,
            full_screen: reader.read_u32::<LE>()?,
            quick_edit: reader.read_u32::<LE>()?,
            insert_mode: reader.read_u32::<LE>()?,
            auto_position: reader.read_u32::<LE>()?,
            history_buffer_size: reader.read_u32::<LE>()?,
            number_of_history_buffers: reader.read_u32::<LE>()?,
            history_no_dup: reader.read_u32::<LE>()?,
            color_table: reader.read_u32_array::<16>()?,
        });
    }
}

bitflags! {
    pub struct FillAttributes : u16 {
        const FOREGROUND_BLUE = 0x01;
        const FOREGROUND_GREEN = 0x02;
        const FOREGROUND_RED = 0x04;
        const FOREGROUND_INTENSITY = 0x08;
        const BACKGROUND_BLUE = 0x10;
        const BACKGROUND_GREEN = 0x20;
        const BACKGROUND_RED = 0x40;
        const BACKGROUND_INTENSITY = 0x80;
    }
}

impl Serialize for FillAttributes {
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

pub struct FontFamily {
    value: u32,
}

impl FontFamily {
    pub fn new(value: u32) -> Self {
        Self { value }
    }
    pub fn font_family(&self) -> String {
        let ff = self.value & 0xFFF0;
        return match ff {
            0x0000 => "FF_DONTCARE",
            0x0010 => "FF_ROMAN",
            0x0020 => "FF_SWISS",
            0x0030 => "FF_MODERN",
            0x0040 => "FF_SCRIPT",
            0x0050 => "FF_DECORATIVE",
            _ => "INVALID",
        }
        .to_string();
    }
    pub fn font_pitch(&self) -> Vec<String> {
        let fp = self.value & 0x0000F;
        if fp == 0 {
            return ["TMPF_NONE".to_string()].to_vec();
        }
        let mut result = Vec::<String>::new();
        if fp & 0x01 != 0 {
            result.push("TMPF_FIXED_PITCH".to_string());
        }
        if fp & 0x02 != 0 {
            result.push("TMPF_VECTOR".to_string());
        }
        if fp & 0x04 != 0 {
            result.push("TMPF_TRUETYPE".to_string());
        }
        if fp & 0x08 != 0 {
            result.push("TMPF_DEVICE".to_string());
        }
        return result;
    }
}

impl Serialize for FontFamily {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("FontFamily", 2)?;
        state.serialize_field("name", &self.font_family())?;
        state.serialize_field("pitch", &self.font_pitch())?;
        return state.end();
    }
}

#[derive(Serialize)]
pub struct ConsoleFEDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub code_page: u32,
}

impl ConsoleFEDataBlock {
    pub fn new(block_size: u32, block_signature: u32, code_page: u32) -> Self {
        Self {
            block_size,
            block_signature,
            code_page,
        }
    }
    pub fn from_slice(slice: &[u8]) -> Result<ConsoleFEDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size != 0x0C {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000004 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        return Ok(ConsoleFEDataBlock::new(
            block_size,
            block_signature,
            reader.read_u32::<LE>()?,
        ));
    }
}
#[derive(Serialize)]
pub struct DarwinDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub darwin_data_ansi: CodepageString,
    pub darwin_data_unicode: UnicodeString,
}

impl DarwinDataBlock {
    pub fn new(
        block_size: u32,
        block_signature: u32,
        darwin_data_ansi: CodepageString,
        darwin_data_unicode: UnicodeString,
    ) -> Self {
        Self {
            block_size,
            block_signature,
            darwin_data_ansi,
            darwin_data_unicode,
        }
    }
    pub fn from_slice(codepage: CodePage, slice: &[u8]) -> Result<DarwinDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size != 0x314 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000006 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        return Ok(DarwinDataBlock::new(
            block_size,
            block_signature,
            reader.read_windows_string_with_size(codepage, 260)?,
            reader.read_windows_unicode_string_with_size(520)?,
        ));
    }
}
#[derive(Serialize)]
pub struct EnvironmentVariableDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub target_ansi: CodepageString,
    pub target_unicode: UnicodeString,
}

impl EnvironmentVariableDataBlock {
    pub fn new(
        block_size: u32,
        block_signature: u32,
        target_ansi: CodepageString,
        target_unicode: UnicodeString,
    ) -> Self {
        Self {
            block_size,
            block_signature,
            target_ansi,
            target_unicode,
        }
    }
    pub fn from_slice(
        codepage: CodePage,
        slice: &[u8],
    ) -> Result<EnvironmentVariableDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size != 0x314 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000001 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        return Ok(EnvironmentVariableDataBlock::new(
            block_size,
            block_signature,
            reader.read_windows_string_with_size(codepage, 260)?,
            reader.read_windows_unicode_string_with_size(520)?,
        ));
    }
}
#[derive(Serialize)]
pub struct IconEnvironmentDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub target_ansi: CodepageString,
    pub target_unicode: UnicodeString,
}

impl IconEnvironmentDataBlock {
    pub fn new(
        block_size: u32,
        block_signature: u32,
        target_ansi: CodepageString,
        target_unicode: UnicodeString,
    ) -> Self {
        Self {
            block_size,
            block_signature,
            target_ansi,
            target_unicode,
        }
    }
    pub fn from_slice(
        codepage: CodePage,
        slice: &[u8],
    ) -> Result<IconEnvironmentDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size != 0x314 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000007 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        return Ok(IconEnvironmentDataBlock::new(
            block_size,
            block_signature,
            reader.read_windows_string_with_size(codepage, 260)?,
            reader.read_windows_unicode_string_with_size(520)?,
        ));
    }
}

#[derive(Serialize)]
pub struct KnownFolderDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub known_folder_id: GUID,
    pub offset: u32,
}

impl KnownFolderDataBlock {
    pub fn new(block_size: u32, block_signature: u32, known_folder_id: GUID, offset: u32) -> Self {
        Self {
            block_size,
            block_signature,
            known_folder_id,
            offset,
        }
    }
    pub fn from_slice(slice: &[u8]) -> Result<KnownFolderDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size != 0x1C {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA000000B {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        return Ok(KnownFolderDataBlock::new(
            block_size,
            block_signature,
            reader.read_lnk_guid()?,
            reader.read_u32::<LE>()?,
        ));
    }
}

#[derive(Serialize)]
pub struct PropertyStoreDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub property_store: SerializedPropertyStore,
}

impl PropertyStoreDataBlock {
    pub fn new(
        block_size: u32,
        block_signature: u32,
        property_store: SerializedPropertyStore,
    ) -> Self {
        Self {
            block_size,
            block_signature,
            property_store,
        }
    }
    pub fn from_slice(
        slice: &[u8],
        codepage: CodePage,
    ) -> Result<PropertyStoreDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size < 0x0C {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000009 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        let data = &slice[reader.position() as usize..];
        return Ok(PropertyStoreDataBlock::new(
            block_size,
            block_signature,
            SerializedPropertyStore::from_slice(data, codepage)?,
        ));
    }
}

#[derive(Serialize)]
pub struct SerializedPropertyStore {
    store_size: u32,
    serialized_property_storage: Vec<SerializedPropertyStorage>,
}

impl SerializedPropertyStore {
    pub fn new(
        store_size: u32,
        serialized_property_storage: Vec<SerializedPropertyStorage>,
    ) -> Self {
        Self {
            store_size,
            serialized_property_storage,
        }
    }
    pub fn from_slice(
        slice: &[u8],
        codepage: CodePage,
    ) -> Result<SerializedPropertyStore, io::Error> {
        let mut serialized_property_storage = Vec::<SerializedPropertyStorage>::new();
        let mut reader = Cursor::new(slice);
        loop {
            let pos = reader.stream_position()?;
            let storage_size = reader.read_u32::<LE>()?;
            if storage_size == 0 {
                break;
            }
            reader.seek(io::SeekFrom::Start(pos))?;
            let data = reader.read_bytes(storage_size as usize)?;
            let storage = SerializedPropertyStorage::from_slice(&data, codepage)?;
            serialized_property_storage.push(storage);
        }

        return Ok(SerializedPropertyStore::new(0, serialized_property_storage));
    }
}
#[derive(Serialize)]
pub struct SerializedPropertyStorage {
    storage_size: u32,
    version: u32,
    format_id: GUID,
    serialized_property_value: Vec<SerializedPropertyValue>,
}

impl SerializedPropertyStorage {
    pub fn new(
        storage_size: u32,
        version: u32,
        format_id: GUID,
        serialized_property_value: Vec<SerializedPropertyValue>,
    ) -> Self {
        Self {
            storage_size,
            version,
            format_id,
            serialized_property_value,
        }
    }
    pub fn from_slice(
        slice: &[u8],
        codepage: CodePage,
    ) -> Result<SerializedPropertyStorage, io::Error> {
        let mut reader = Cursor::new(slice);
        let storage_size = reader.read_u32::<LE>()?;
        if storage_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid StorageSize",
            ));
        }
        let version = reader.read_u32::<LE>()?;

        if version != 0x53505331 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid SerializedPropertyStorage Version",
            ));
        }
        let format_id = reader.read_lnk_guid()?;

        fn read_string_name(
            slice: &[u8],
            _codepage: CodePage,
        ) -> Result<SerializedPropertyValue, io::Error> {
            Ok(SerializedPropertyValue::StringName(
                SerializedPropertyValueStringName::from_slice(slice)?,
            ))
        }
        fn read_integer_name(
            slice: &[u8],
            codepage: CodePage,
        ) -> Result<SerializedPropertyValue, io::Error> {
            Ok(SerializedPropertyValue::IntegerName(
                SerializedPropertyValueIntegerName::from_slice(slice, codepage)?,
            ))
        }
        let read_property_value =
            match format_id.to_string() == "D5CDD505-2E9C-101B-9397-08002B2CF9AE" {
                true => read_string_name,
                false => read_integer_name,
            };
        let mut serialized_property_value = Vec::<SerializedPropertyValue>::new();
        loop {
            let pos = reader.stream_position()?;
            let value_size = reader.read_u32::<LE>()?;
            if value_size == 0 {
                break;
            }
            reader.seek(io::SeekFrom::Start(pos))?;
            let data = reader.read_bytes(value_size as usize)?;
            let value = read_property_value(&data, codepage)?;
            serialized_property_value.push(value);
        }
        return Ok(SerializedPropertyStorage::new(
            storage_size,
            version,
            format_id,
            serialized_property_value,
        ));
    }
}
#[derive(Serialize)]
pub enum SerializedPropertyValue {
    StringName(SerializedPropertyValueStringName),
    IntegerName(SerializedPropertyValueIntegerName),
}

#[derive(Serialize)]
pub struct SerializedPropertyValueStringName {
    value_size: u32,
    names_size: u32,
    reserved: u8,
    name: UnicodeString,
    value: Vec<u8>,
}

impl SerializedPropertyValueStringName {
    pub fn new(
        value_size: u32,
        names_size: u32,
        reserved: u8,
        name: UnicodeString,
        value: Vec<u8>,
    ) -> Self {
        Self {
            value_size,
            names_size,
            reserved,
            name,
            value,
        }
    }
    pub fn from_slice(slice: &[u8]) -> Result<SerializedPropertyValueStringName, io::Error> {
        let mut reader = Cursor::new(slice);
        let value_size = reader.read_u32::<LE>()?;
        if value_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid ValueSize",
            ));
        }
        let names_size = reader.read_u32::<LE>()?;
        return Ok(SerializedPropertyValueStringName::new(
            value_size,
            names_size,
            reader.read_u8()?,
            reader.read_windows_unicode_string_with_size(names_size as usize)?,
            reader.read_bytes(value_size as usize)?.to_vec(),
        ));
    }
}
#[derive(Serialize)]
pub struct SerializedPropertyValueIntegerName {
    value_size: u32,
    id: u32,
    reserved: u8,
    value: TypedPropertyValue,
}

impl SerializedPropertyValueIntegerName {
    pub fn new(value_size: u32, id: u32, reserved: u8, value: TypedPropertyValue) -> Self {
        Self {
            value_size,
            id,
            reserved,
            value,
        }
    }
    pub fn from_slice(
        slice: &[u8],
        codepage: CodePage,
    ) -> Result<SerializedPropertyValueIntegerName, io::Error> {
        let mut reader = Cursor::new(slice);
        let value_size = reader.read_u32::<LE>()?;
        if value_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid ValueSize",
            ));
        }
        return Ok(SerializedPropertyValueIntegerName::new(
            value_size,
            reader.read_u32::<LE>()?,
            reader.read_u8()?,
            TypedPropertyValue::from_oleps_reader(&mut reader, codepage, true)?,
        ));
    }
}

#[derive(Serialize)]
pub struct ShimDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub layer_name: UnicodeString,
}

impl ShimDataBlock {
    pub fn new(block_size: u32, block_signature: u32, layer_name: UnicodeString) -> Self {
        Self {
            block_size,
            block_signature,
            layer_name,
        }
    }
    pub fn from_slice(slice: &[u8]) -> Result<ShimDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size < 0x88 || block_size % 2 == 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000008 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        let layer_name = reader.read_windows_unicode_string_with_size((block_size - 8) as usize)?;
        return Ok(ShimDataBlock::new(block_size, block_signature, layer_name));
    }
}
#[derive(Serialize)]
pub struct SpecialFolderDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub special_folder_id: u32,
    pub offset: u32,
}

impl SpecialFolderDataBlock {
    pub fn new(block_size: u32, block_signature: u32, special_folder_id: u32, offset: u32) -> Self {
        Self {
            block_size,
            block_signature,
            special_folder_id,
            offset,
        }
    }
    pub fn from_slice(slice: &[u8]) -> Result<SpecialFolderDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size != 0x10 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000005 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        return Ok(SpecialFolderDataBlock::new(
            block_size,
            block_signature,
            reader.read_u32::<LE>()?,
            reader.read_u32::<LE>()?,
        ));
    }
}
#[derive(Serialize)]
pub struct TrackerDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub length: u32,
    pub version: u32,
    pub machine_id: CodepageString,
    pub droid: (GUID, GUID),
    pub droid_birth: (GUID, GUID),
}

impl TrackerDataBlock {
    pub fn new(
        block_size: u32,
        block_signature: u32,
        length: u32,
        version: u32,
        machine_id: CodepageString,
        droid: (GUID, GUID),
        droid_birth: (GUID, GUID),
    ) -> Self {
        Self {
            block_size,
            block_signature,
            length,
            version,
            machine_id,
            droid,
            droid_birth,
        }
    }
    pub fn from_slice(codepage: CodePage, slice: &[u8]) -> Result<TrackerDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size != 0x60 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA0000003 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        let length = reader.read_u32::<LE>()?;
        if length != 0x58 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid Length"));
        }
        let version = reader.read_u32::<LE>()?;
        if version != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid Version",
            ));
        }
        return Ok(TrackerDataBlock::new(
            block_size,
            block_signature,
            length,
            version,
            reader.read_windows_string_with_size(codepage, 16)?,
            (reader.read_lnk_guid()?, reader.read_lnk_guid()?),
            (reader.read_lnk_guid()?, reader.read_lnk_guid()?),
        ));
    }
}
#[derive(Serialize)]
pub struct VistaAndAboveIDListDataBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub id_list: IDList,
}

impl VistaAndAboveIDListDataBlock {
    pub fn new(block_size: u32, block_signature: u32, id_list: IDList) -> Self {
        Self {
            block_size,
            block_signature,
            id_list,
        }
    }
    pub fn from_slice(
        codepage: CodePage,
        slice: &[u8],
    ) -> Result<VistaAndAboveIDListDataBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        if block_size < 0x0A {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSize",
            ));
        }
        if block_signature != 0xA000000C {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid BlockSignature",
            ));
        }
        return Ok(VistaAndAboveIDListDataBlock::new(
            block_size,
            block_signature,
            IDList::from_slice(
                codepage,
                &slice[reader.position() as usize..],
                (block_size - 8) as u16,
            )?,
        ));
    }
}

#[derive(Serialize)]
pub struct UnsupportedBlock {
    pub block_size: u32,
    pub block_signature: u32,
    pub data: Vec<u8>,
}

impl UnsupportedBlock {
    pub fn new(block_size: u32, block_signature: u32, data: Vec<u8>) -> Self {
        Self {
            block_size,
            block_signature,
            data,
        }
    }
    pub fn from_slice(slice: &[u8]) -> Result<UnsupportedBlock, io::Error> {
        let mut reader = Cursor::new(slice);
        let block_size = reader.read_u32::<LE>()?;
        let block_signature = reader.read_u32::<LE>()?;
        let data = reader.read_bytes((block_size - 8) as usize)?.to_vec();
        return Ok(UnsupportedBlock::new(block_size, block_signature, data));
    }
}

trait ReadArray {
    fn read_u32_array<const SIZE: usize>(&mut self) -> Result<[u32; SIZE], io::Error>;
}

impl<Reader: std::io::Read> ReadArray for Reader {
    fn read_u32_array<const SIZE: usize>(&mut self) -> Result<[u32; SIZE], io::Error> {
        let mut result: [u32; SIZE] = [0; SIZE];
        for i in result.iter_mut() {
            *i = self.read_u32::<LE>()?;
        }
        return Ok(result);
    }
}
