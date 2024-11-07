#![allow(clippy::needless_return)]

mod config;
mod lnk;

use backend_utils::objects::{
    BackendRequest, BackendResultChild, BackendResultKind, BackendResultOk,
};
use std::path::PathBuf;
use tracing::*;
use tracing_subscriber::prelude::*;

use crate::lnk::LnkFile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = config::Config::new()?;
    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        process_request(request, &config)
    })?;
    unreachable!()
}

//#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());
    let metadata = match LnkFile::load(input_name) {
        Ok(v) => v,
        Err(e)
            if e.kind() == std::io::ErrorKind::InvalidData
                || e.kind() == std::io::ErrorKind::UnexpectedEof =>
        {
            return Ok(BackendResultKind::error(format!("Invalid data ({})", e)))
        }
        Err(e) => return Err(e),
    };

    let symbols = Vec::<String>::new();
    let children = Vec::<BackendResultChild>::new();

    Ok(BackendResultKind::ok(BackendResultOk {
        symbols,
        object_metadata: match serde_json::to_value(metadata).unwrap() {
            serde_json::Value::Object(v) => v,
            _ => unreachable!(),
        },
        children,
    }))
}

#[test]
fn test_ms_example() -> Result<(), std::io::Error> {
    use crate::lnk::link_info::LinkInfoFlags;
    use crate::lnk::shell_link_header::FileAttributesFlags;
    use crate::lnk::shell_link_header::LinkFlags;
    use crate::lnk::{extra_data, link_info::DriveType, LnkFile, StringDataEnum};
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    const EXAMPLE_DATA: &[u8] = &[
        0x4C, 0x00, 0x00, 0x00, 0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x46, 0x9B, 0x00, 0x08, 0x00, 0x20, 0x00, 0x00, 0x00, 0xD0, 0xE9,
        0xEE, 0xF2, 0x15, 0x15, 0xC9, 0x01, 0xD0, 0xE9, 0xEE, 0xF2, 0x15, 0x15, 0xC9, 0x01, 0xD0,
        0xE9, 0xEE, 0xF2, 0x15, 0x15, 0xC9, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xBD, 0x00, 0x14, 0x00, 0x1F, 0x50, 0xE0, 0x4F, 0xD0, 0x20, 0xEA, 0x3A, 0x69, 0x10,
        0xA2, 0xD8, 0x08, 0x00, 0x2B, 0x30, 0x30, 0x9D, 0x19, 0x00, 0x2F, 0x43, 0x3A, 0x5C, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x46, 0x00, 0x31, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x39, 0x69, 0xA3,
        0x10, 0x00, 0x74, 0x65, 0x73, 0x74, 0x00, 0x00, 0x32, 0x00, 0x07, 0x00, 0x04, 0x00, 0xEF,
        0xBE, 0x2C, 0x39, 0x65, 0xA3, 0x2C, 0x39, 0x69, 0xA3, 0x26, 0x00, 0x00, 0x00, 0x03, 0x1E,
        0x00, 0x00, 0x00, 0x00, 0xF5, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x74, 0x00, 0x65, 0x00, 0x73, 0x00, 0x74, 0x00, 0x00, 0x00, 0x14, 0x00, 0x48, 0x00,
        0x32, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x39, 0x69, 0xA3, 0x20, 0x00, 0x61, 0x2E, 0x74,
        0x78, 0x74, 0x00, 0x34, 0x00, 0x07, 0x00, 0x04, 0x00, 0xEF, 0xBE, 0x2C, 0x39, 0x69, 0xA3,
        0x2C, 0x39, 0x69, 0xA3, 0x26, 0x00, 0x00, 0x00, 0x2D, 0x6E, 0x00, 0x00, 0x00, 0x00, 0x96,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x61, 0x00, 0x2E, 0x00,
        0x74, 0x00, 0x78, 0x00, 0x74, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x00,
        0x00, 0x1C, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1C, 0x00, 0x00, 0x00, 0x2D, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3B, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, 0x03,
        0x00, 0x00, 0x00, 0x81, 0x8A, 0x7A, 0x30, 0x10, 0x00, 0x00, 0x00, 0x00, 0x43, 0x3A, 0x5C,
        0x74, 0x65, 0x73, 0x74, 0x5C, 0x61, 0x2E, 0x74, 0x78, 0x74, 0x00, 0x00, 0x07, 0x00, 0x2E,
        0x00, 0x5C, 0x00, 0x61, 0x00, 0x2E, 0x00, 0x74, 0x00, 0x78, 0x00, 0x74, 0x00, 0x07, 0x00,
        0x43, 0x00, 0x3A, 0x00, 0x5C, 0x00, 0x74, 0x00, 0x65, 0x00, 0x73, 0x00, 0x74, 0x00, 0x60,
        0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0xA0, 0x58, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x63, 0x68, 0x72, 0x69, 0x73, 0x2D, 0x78, 0x70, 0x73, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x40, 0x78, 0xC7, 0x94, 0x47, 0xFA, 0xC7, 0x46, 0xB3, 0x56, 0x5C, 0x2D, 0xC6, 0xB6,
        0xD1, 0x15, 0xEC, 0x46, 0xCD, 0x7B, 0x22, 0x7F, 0xDD, 0x11, 0x94, 0x99, 0x00, 0x13, 0x72,
        0x16, 0x87, 0x4A, 0x40, 0x78, 0xC7, 0x94, 0x47, 0xFA, 0xC7, 0x46, 0xB3, 0x56, 0x5C, 0x2D,
        0xC6, 0xB6, 0xD1, 0x15, 0xEC, 0x46, 0xCD, 0x7B, 0x22, 0x7F, 0xDD, 0x11, 0x94, 0x99, 0x00,
        0x13, 0x72, 0x16, 0x87, 0x4A, 0x00, 0x00, 0x00, 0x00,
    ];

    let tmp_dir = tempdir()?;
    let input_path = tmp_dir.path().join("example.lnk");
    let mut temporary_file = File::create(&input_path)?;
    temporary_file.write_all(EXAMPLE_DATA)?;
    temporary_file.flush()?;

    let lnk_file = LnkFile::load(input_path)?;

    let shell_link_header = &lnk_file.shell_link_header;
    assert_eq!(shell_link_header.header_size, 0x4C);
    assert_eq!(
        shell_link_header.link_clsid.to_string().to_uppercase(),
        "00021401-0000-0000-C000-000000000046"
    );
    assert_eq!(
        shell_link_header.link_flags.bits(),
        (LinkFlags::HasLinkTargetIDList
            | LinkFlags::HasLinkInfo
            | LinkFlags::HasRelativePath
            | LinkFlags::HasWorkingDir
            | LinkFlags::IsUnicode
            | LinkFlags::EnableTargetMetadata)
            .bits()
    );
    assert_eq!(
        shell_link_header.file_attributes_flag.bits(),
        FileAttributesFlags::FILE_ATTRIBUTE_ARCHIVE.bits()
    );
    let datetime = time::macros::datetime!(2008-09-12 20:27:17.101 UTC);
    assert_eq!(
        shell_link_header.creation_time.as_datetime(),
        Some(datetime)
    );
    assert_eq!(shell_link_header.access_time.as_datetime(), Some(datetime));
    assert_eq!(shell_link_header.write_time.as_datetime(), Some(datetime));

    assert_eq!(shell_link_header.file_size, 0);
    assert_eq!(shell_link_header.icon_index, 0);
    assert_eq!(shell_link_header.show_command.to_string(), "SW_SHOWNORMAL");
    assert_eq!(shell_link_header.hot_key.to_string(), "None");
    assert_eq!(shell_link_header.reserved1, 0);
    assert_eq!(shell_link_header.reserved2, 0);
    assert_eq!(shell_link_header.reserved3, 0);

    assert!(lnk_file.link_target_id_list.is_some());
    let link_target_id_list = &lnk_file.link_target_id_list.unwrap();
    assert!(link_target_id_list.id_list_size == 0xBD);
    assert!(link_target_id_list.id_list.item_id_list.len() == 4);
    // assert!(link_target_id_list.id_list.item_id_list[0].data.len() == 18);
    // assert!(link_target_id_list.id_list.item_id_list[1].data.len() == 23);
    // assert!(link_target_id_list.id_list.item_id_list[2].data.len() == 68);
    // assert!(link_target_id_list.id_list.item_id_list[3].data.len() == 70);

    assert!(lnk_file.link_info.is_some());
    let link_info = &lnk_file.link_info.unwrap();
    assert!(link_info.link_info_size == 0x3C);
    assert!(link_info.link_info_flags.bits() == LinkInfoFlags::VolumeIDAndLocalBasePath.bits());
    assert!(link_info.volume_id_offset == 0x1C);
    assert!(link_info.local_base_path_offset == 0x2D);
    assert!(link_info.common_network_relative_link_offset == 0);
    assert!(link_info.common_path_suffix_offset == 0x3B);
    assert!(link_info.volume_id.is_some());
    let volume_id = link_info.volume_id.as_ref().unwrap();
    assert!(volume_id.volume_id_size == 0x11);
    assert!(volume_id.drive_type == DriveType::DriveFixed);
    assert!(volume_id.drive_serial_number == 0x307A8A81);
    assert!(volume_id.volume_label_offset == 0x10);
    assert!(volume_id.volume_label_offset_unicode.is_none());
    assert!(volume_id.volume_label.data.is_empty());
    assert!(volume_id.volume_label_unicode.is_none());
    assert!(link_info.local_base_path.is_some());
    assert!(link_info.local_base_path.as_ref().unwrap().to_string() == "C:\\test\\a.txt");
    assert!(link_info.common_path_suffix.to_string() == "");

    assert!(lnk_file.string_data.name_string.is_none());
    assert!(lnk_file.string_data.relative_path.is_some());
    assert!(lnk_file.string_data.working_dir.is_some());
    assert!(lnk_file.string_data.command_line_arguments.is_none());
    assert!(lnk_file.string_data.icon_location.is_none());
    let relative_path = &lnk_file.string_data.relative_path.as_ref().unwrap();
    match &relative_path.string {
        StringDataEnum::WindowsUnicodeString(str) => assert!(str.to_string() == ".\\a.txt"),
        _ => panic!("Invalid StringDataEnum"),
    }
    let working_dir = &lnk_file.string_data.working_dir.as_ref().unwrap();
    match &working_dir.string {
        StringDataEnum::WindowsUnicodeString(str) => assert!(str.to_string() == "C:\\test"),
        _ => panic!("Invalid StringDataEnum"),
    }
    assert!(lnk_file.extra_data.len() == 1);
    let tracker_data_block = match &lnk_file.extra_data[0] {
        extra_data::ExtraData::TrackerDataBlock(block) => block,
        _ => panic!("Invalid ExtraData"),
    };
    assert_eq!(tracker_data_block.block_size, 0x60);
    assert_eq!(tracker_data_block.block_signature, 0xA0000003);
    assert_eq!(tracker_data_block.length, 0x58);
    assert_eq!(tracker_data_block.version, 0);
    assert_eq!(tracker_data_block.machine_id.to_string(), "chris-xps");
    assert_eq!(
        tracker_data_block.droid.0.to_string().to_uppercase(),
        "94C77840-FA47-46C7-B356-5C2DC6B6D115"
    );
    assert_eq!(
        tracker_data_block.droid.1.to_string().to_uppercase(),
        "7BCD46EC-7F22-11DD-9499-00137216874A"
    );
    assert_eq!(
        tracker_data_block.droid_birth.0.to_string().to_uppercase(),
        "94C77840-FA47-46C7-B356-5C2DC6B6D115"
    );
    assert_eq!(
        tracker_data_block.droid_birth.1.to_string().to_uppercase(),
        "7BCD46EC-7F22-11DD-9499-00137216874A"
    );

    return Ok(());
}
