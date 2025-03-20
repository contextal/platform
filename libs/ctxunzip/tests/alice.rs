use ctxunzip::{Zip, ZipRandomAccess};
use num_traits::Num;
use serde_json::Value;
use std::{
    collections::HashMap,
    fmt::Debug,
    fs::{self, File},
    str::FromStr,
};
use time::{PrimitiveDateTime, macros::format_description};

const INPUT_LIMIT: u64 = 100 * 1024 * 1024;

#[test]
fn unpack_decrypt_and_check_intergrity() {
    let (path, expected_entries) = ("tests/test_data/alice.zip", 14);
    let correct_password = "password";
    let wrong_password = "wrongpass";
    let mut extracted_entries = 0;

    let file = File::open(&path).unwrap_or_else(|e| panic!("failed to open {path}: {e:#?}"));
    let zip = Zip::new(&file).unwrap_or_else(|e| panic!("failed to parse {file:#?}: {e:#?}"));

    for entry in zip {
        let mut entry =
            entry.unwrap_or_else(|e| panic!("Failed to get next entry in file {file:#?}: {e:#?}"));
        if let Some(err) = entry.get_error() {
            panic!("{path}::{name}: ERROR: {err:#?}", name = entry.name());
        }
        let name = entry.name().to_string();

        if entry.is_encrypted() {
            assert!(
                !entry.set_password(wrong_password),
                "Wrong password should not match for {path}::{name}"
            );
            assert!(
                entry.set_password(correct_password),
                "Correct password should match for {path}::{name}"
            );
        }

        let mut reader = entry
            .take_reader(INPUT_LIMIT)
            .unwrap_or_else(|e| panic!("Failed to get a reader for entry {path}::{name}: {e:#?}"));
        if let Err(e) = std::io::copy(&mut reader, &mut std::io::sink()) {
            panic!("Extraction failed: {path}::{name}: {e:#?}")
        }

        if reader.integrity_check_ok() == false {
            panic!("{path}::{name} CRCMISMATCH is not expected in this test case");
        }
        eprintln!("{path}::{name} OK");
        extracted_entries += 1;
    }

    assert_eq!(
        expected_entries, extracted_entries,
        "Unexpected amount of entries extracted from {path:?}. \
        Extracted: {extracted_entries}, expected: {expected_entries}",
    );
}

#[test]
fn consuming_and_non_consuming_iter() {
    let (path, expected_entries) = ("tests/test_data/alice.zip", 14);
    let correct_password = "password";
    let wrong_password = "wrongpass";
    let mut extracted_entries = 0;

    let file = File::open(&path).unwrap_or_else(|e| panic!("failed to open {path}: {e:#?}"));
    let zip = Zip::new(&file).unwrap_or_else(|e| panic!("failed to parse {file:#?}: {e:#?}"));

    for entry in &zip {
        let mut entry =
            entry.unwrap_or_else(|e| panic!("Failed to get next entry in file {file:#?}: {e:#?}"));
        if let Some(err) = entry.get_error() {
            panic!("{path}::{name}: ERROR: {err:#?}", name = entry.name());
        }
        let name = entry.name().to_string();

        if entry.is_encrypted() {
            assert!(
                !entry.set_password(wrong_password),
                "Wrong password should not match for {path}::{name}"
            );
            assert!(
                entry.set_password(correct_password),
                "Correct password should match for {path}::{name}"
            );
        }

        let mut reader = entry
            .take_reader(INPUT_LIMIT)
            .unwrap_or_else(|e| panic!("Failed to get a reader for entry {path}::{name}: {e:#?}"));
        if let Err(e) = std::io::copy(&mut reader, &mut std::io::sink()) {
            panic!("Extraction failed: {path}::{name}: {e:#?}")
        }

        if reader.integrity_check_ok() == false {
            panic!("{path}::{name} CRCMISMATCH is not expected in this test case");
        }
        eprintln!("{path}::{name} OK");
        extracted_entries += 1;
    }

    for entry in zip {
        let mut entry =
            entry.unwrap_or_else(|e| panic!("Failed to get next entry in file {file:#?}: {e:#?}"));
        if let Some(err) = entry.get_error() {
            panic!("{path}::{name}: ERROR: {err:#?}", name = entry.name());
        }
        let name = entry.name().to_string();

        if entry.is_encrypted() {
            assert!(
                !entry.set_password(wrong_password),
                "Wrong password should not match for {path}::{name}"
            );
            assert!(
                entry.set_password(correct_password),
                "Correct password should match for {path}::{name}"
            );
        }

        let mut reader = entry
            .take_reader(INPUT_LIMIT)
            .unwrap_or_else(|e| panic!("Failed to get a reader for entry {path}::{name}: {e:#?}"));
        if let Err(e) = std::io::copy(&mut reader, &mut std::io::sink()) {
            panic!("Extraction failed: {path}::{name}: {e:#?}")
        }

        if reader.integrity_check_ok() == false {
            panic!("{path}::{name} CRCMISMATCH is not expected in this test case");
        }
        eprintln!("{path}::{name} OK");
        extracted_entries += 1;
    }

    assert_eq!(
        expected_entries * 2, // once for non-consuming and once for consuming iterator
        extracted_entries,
        "Unexpected amount of entries extracted from {path:?}. \
        Extracted: {extracted_entries}, expected: {expected_entries}",
    );
}

// The following test reads all the parsed key/value pairs provided by `zipinfo` and verifies that
// appropriate values match.
// All key/value pairs for each archive entry have to be either consumed for comparison, or
// explicitly dropped to ensure that nothing obtained from `zipinfo` have been missed and left
// unchecked.
#[test]
fn compare_with_zipinfo() {
    let (path, zipinfo_path) = (
        "tests/test_data/alice.zip",
        "tests/test_data/alice.zip.zipinfo",
    );

    let file = File::open(&path).unwrap_or_else(|e| panic!("failed to open {path}: {e:#?}"));
    let zip = Zip::new(&file).unwrap_or_else(|e| panic!("failed to parse {file:#?}: {e:#?}"));
    let zipinfo_contents = fs::read_to_string(zipinfo_path)
        .unwrap_or_else(|e| panic!("failed to open and read {zipinfo_path}: {e:#?}"));
    let mut zipinfo_json: Vec<HashMap<String, Value>> =
        serde_json::from_str(&zipinfo_contents).expect("failed to parse json from file");

    let mut entries = zipinfo_json.split_off(1);
    let archive = &mut zipinfo_json.pop().unwrap();

    assert_eq!(
        entries.len() as u64,
        zip.get_entries_total(),
        "Unexpected amount of entries in {path:?}"
    );
    assert_eq!(
        consume_as_number::<u64>(archive, "entries").expect("entries is mandatory"),
        zip.get_entries_total(),
        "Unexpected amount of entries in {path:?}"
    );
    assert_eq!(
        consume_as_number::<u64>(archive, "cent_dir_size").expect("cent_dir_size is mandatory"),
        zip.get_cd_size(),
        "Unexpected central directory size in {path:?}"
    );
    assert_eq!(
        consume_as_number::<u64>(archive, "cent_dir_offset").expect("cent_dir_offset is mandatory"),
        zip.get_cd_offset_on_first_disk(),
        "Unexpected offset of start of central directory in {path:?}"
    );

    // Explicitly consume values which have left in the `archive` HashMap, but which don't have
    // corresponding value to compare:
    let _ = consume_as_string(archive, "actual_end_cent_dir_offset").unwrap();
    let _ = consume_as_string(archive, "expected_end_cent_dir_offset").unwrap();
    let _ = consume_as_string(archive, "zip_archive_size").unwrap();
    let _ = consume_as_string(archive, "filename").unwrap();
    // Artificial record type added by zipinfo parsing script:
    let _ = consume_as_string(archive, "entry_type").unwrap();
    assert!(
        archive.is_empty(),
        "Some key/value pairs have not been consumed for comparison or explicitly dropped \
        for {path}: {archive:#?}"
    );

    let mut expected_iter = entries.iter_mut();
    for entry in zip {
        let entry =
            entry.unwrap_or_else(|e| panic!("Failed to get next entry in file {file:#?}: {e:#?}"));
        if let Some(err) = entry.get_error() {
            panic!("{path}::{name}: ERROR: {err:#?}", name = entry.name());
        }
        let name = entry.name().to_string();
        let local_header = entry
            .get_local_header()
            .unwrap_or_else(|| panic!("failed to obtain a local header for {path}::{name}"));
        let central_header = entry.get_central_header();

        let mut expected_values = expected_iter
            .next()
            .expect("failed to obtain the next entry of expected values");

        let expected_file_name =
            consume_as_string(expected_values, "filename").expect("filename is mandatory");
        assert_eq!(
            central_header.file_name,
            expected_file_name.as_bytes(),
            "File names don't match for {path}::{name} for central header"
        );
        assert_eq!(
            local_header.file_name,
            expected_file_name.as_bytes(),
            "File names don't match for {path}::{name} for local header"
        );

        let expected_crc32: u32 =
            consume_as_number(expected_values, "crc32").expect("crc32 is mandatory");
        assert_eq!(
            local_header.crc32, expected_crc32,
            "CRC32 checksums don't match for {path}::{name} for central header"
        );
        assert_eq!(
            central_header.crc32, expected_crc32,
            "CRC32 checksums don't match for {path}::{name} for local header"
        );

        let expected_compressed_size: u64 =
            consume_as_number(&mut expected_values, "compressed_size")
                .expect("compressed_size is mandatory");
        assert_eq!(
            central_header.compressed_size, expected_compressed_size,
            "Compressed sizes don't match for {path}::{name} for central header"
        );
        assert_eq!(
            local_header.compressed_size, expected_compressed_size,
            "Compressed sizes don't match for {path}::{name} for local header"
        );

        let expected_uncompressed_size: u64 =
            consume_as_number(&mut expected_values, "uncompressed_size")
                .expect("uncompressed_size is mandatory");
        assert_eq!(
            central_header.uncompressed_size, expected_uncompressed_size,
            "Uncompressed sizes don't match for {path}::{name} for central header"
        );
        assert_eq!(
            local_header.uncompressed_size, expected_uncompressed_size,
            "Uncompressed sizes don't match for {path}::{name} for local header"
        );

        // lower byte of central_header.ver_made_by:
        let expected_ver_made_by =
            consume_as_string(expected_values, "ver_made_by").expect("ver_made_by is mandatory");
        assert_eq!(
            byte_to_version_string((central_header.ver_made_by & 0xff) as u8),
            expected_ver_made_by,
            "Versions of encoding software don't match for {path}::{name} for central header"
        );

        // higher byte of central_header.ver_made_by:
        let expected_fs_or_os = consume_as_string(expected_values, "fs_or_os_of_origin")
            .expect("fs_or_os_of_origin is mandatory");
        let fs_or_os = index_to_os_str(entry.origin_os_or_fs());
        assert_eq!(
            fs_or_os, expected_fs_or_os,
            "File system or operating system of origin don't match for {path}::{name} \
            for central header"
        );

        // lower byte of central_header.ver_to_extract && local_header.ver_to_extract:
        let expected_ver_to_extract = consume_as_string(expected_values, "ver_to_extract")
            .expect("ver_to_extract is mandatory");
        assert_eq!(
            byte_to_version_string((central_header.ver_to_extract & 0xff) as u8),
            expected_ver_to_extract,
            "Minimum software versions necessary for extraction don't match for {path}::{name} \
            for central header"
        );
        assert_eq!(
            byte_to_version_string((local_header.ver_to_extract & 0xff) as u8),
            expected_ver_to_extract,
            "Minimum software versions necessary for extraction don't match for {path}::{name} \
            for local header"
        );

        // higher byte of central_header.ver_to_extract && local_header.ver_to_extract:
        let expected_minimum_fs_required =
            consume_as_string(expected_values, "minimum_fs_required")
                .expect("minimum_fs_required is mandatory");
        assert_eq!(
            index_to_os_str((central_header.ver_to_extract >> 8) as u8),
            expected_minimum_fs_required,
            "Minimum file system necessary for extraction don't match for {path}::{name} \
            for central header"
        );
        assert_eq!(
            index_to_os_str((local_header.ver_to_extract >> 8) as u8),
            expected_minimum_fs_required,
            "Minimum file system necessary for extraction don't match for {path}::{name} \
            for local header"
        );

        let expected_compression = consume_as_string(expected_values, "compression_method")
            .expect("compression_method is mandatory");
        assert_eq!(
            index_to_compression_method_str(central_header.compression_method),
            expected_compression,
            "Compression methods don't match for {path}::{name} for central header"
        );
        assert_eq!(
            index_to_compression_method_str(local_header.compression_method),
            expected_compression,
            "Compression methods don't match for {path}::{name} for local header"
        );

        if let Some(expected_implosion_sliding_dictionary) =
            consume_as_string(expected_values, "implosion_sliding_dictionary")
        {
            [
                (central_header.gp_flag, "central"),
                (local_header.gp_flag, "local"),
            ]
            .into_iter()
            .for_each(|(gp_flag, type_str)| {
                let implosion_sliding_dictionary = match (gp_flag >> 1) & 1 {
                    0 => "4K",
                    1 => "8K",
                    _ => unreachable!(),
                };
                assert_eq!(
                    implosion_sliding_dictionary, expected_implosion_sliding_dictionary,
                    "Implosion sliding dictionary sizes don't match for {path}::{name} \
                    for {type_str} header"
                );
            });
        }

        if let Some(expected_implosion_sf_trees) =
            consume_as_number::<u8>(&mut expected_values, "implosion_sf_trees")
        {
            [
                (central_header.gp_flag, "central"),
                (local_header.gp_flag, "local"),
            ]
            .into_iter()
            .for_each(|(gp_flag, type_str)| {
                let implosion_sf_trees = match (gp_flag >> 2) & 1 {
                    0 => 2,
                    1 => 3,
                    _ => unreachable!(),
                };
                assert_eq!(
                    implosion_sf_trees, expected_implosion_sf_trees,
                    "Implosion number of Shannon-Fano trees don't match for {path}::{name} \
                    for {type_str} header"
                );
            });
        }

        if let Some(expected_deflation_sub_type) =
            consume_as_string(expected_values, "deflation_sub_type")
        {
            [
                (central_header.gp_flag, "central"),
                (local_header.gp_flag, "local"),
            ]
            .into_iter()
            .for_each(|(gp_flag, type_str)| {
                let deflation_sub_type = match (gp_flag >> 1) & 3 {
                    0 => "normal",
                    1 => "maximum",
                    2 => "fast",
                    3 => "superfast",
                    _ => unreachable!(),
                };
                assert_eq!(
                    deflation_sub_type, expected_deflation_sub_type,
                    "Deflation compression sub-types don't match for {path}::{name} \
                    for {type_str} header"
                );
            });
        }

        let expected_extended_local_header =
            consume_as_string(expected_values, "extended_local_header")
                .expect("extended_local_header is mandatory");
        {
            [
                (central_header.gp_flag, "central"),
                (local_header.gp_flag, "local"),
            ]
            .into_iter()
            .for_each(|(gp_flag, type_str)| {
                let extended_local_header = match (gp_flag >> 3) & 1 {
                    0 => "no",
                    1 => "yes",
                    _ => unreachable!(),
                };
                assert_eq!(
                    extended_local_header, expected_extended_local_header,
                    "A GP bits signifying presence of extended local header don't match \
                    for {path}::{name} for {type_str} header"
                );
            });
        }

        let expected_file_comment_len: usize =
            consume_as_number(&mut expected_values, "file_comment_length")
                .expect("file_comment_length considered mandatory");
        assert_eq!(
            central_header.comment.len(),
            expected_file_comment_len,
            "Comment lengths don't match for {path}::{name} for central header"
        );

        let expected_mtime = {
            let mtime_str =
                consume_as_string(expected_values, "mtime").expect("mtime is mandatory");
            PrimitiveDateTime::parse(
                &mtime_str,
                &format_description!("[year] [month repr:short] [day] [hour]:[minute]:[second]"),
            )
            .unwrap_or_else(|e| panic!("failed to parse expected mtime value {mtime_str:?}: {e}"))
        };
        assert_eq!(
            central_header.mtime.unwrap(),
            expected_mtime,
            "Modification times don't match for {path}::{name} for central header"
        );
        assert_eq!(
            local_header.mtime.unwrap(),
            expected_mtime,
            "Modification times don't match for {path}::{name} for local header"
        );

        let expected_disk_number_from_one: u32 =
            consume_as_number(&mut expected_values, "disk_number_from_one")
                .expect("disk_number_from_one is mandatory");
        assert_eq!(
            central_header.disk_number,
            expected_disk_number_from_one - 1,
            "Disk numbers don't match for {path}::{name} for central header"
        );

        let expected_local_header_offset: u64 =
            consume_as_number(&mut expected_values, "local_header_offset")
                .expect("local_header_offset is mandatory");
        assert_eq!(
            central_header.local_header_offset, expected_local_header_offset,
            "Local header offsets don't match for {path}::{name} for central header"
        );

        let expected_extras_len: usize =
            consume_as_number(&mut expected_values, "extras_len").expect("extras_len is mandatory");
        assert_eq!(
            central_header.extras.len(),
            expected_extras_len,
            "Zip extra fileds length don't match for {path}::{name} for central header"
        );

        if let Some(ids) = consume_as_string(expected_values, "subfield_id") {
            let ids = ids.to_string();
            ids.split(' ').into_iter().for_each(|id_str| {
                let id = from_hex_str(id_str);
                let data_len: usize =
                    consume_as_number(&mut expected_values, &format!("subfield_{id_str}_data_len"))
                        .unwrap_or_else(|| panic!("subfield_{id_str}_data_len is mandatory"));
                assert_eq!(
                    data_len,
                    central_header
                        .extras
                        .field_data(id)
                        .unwrap_or_else(|| {
                            panic!("presence of extras field {id_str} is expected")
                        })
                        .len(),
                    "Lengths of extras field {id_str} don't match for {path}::{name} for \
                    central header"
                );

                // avoid comparing data of a record for which zipinfo provides just length and type and no data bytes
                if id == 0x5455 {
                    return;
                }

                let data_clipping_len = 20; // max data bytes provided by zipinfo per record
                let clipped = data_len > data_clipping_len;
                let field_id = if clipped {
                    format!("subfield_{id_str}_first_{data_clipping_len}_bytes")
                } else {
                    format!("subfield_{id_str}_bytes")
                };

                let expected_data: Vec<u8> = consume_as_string(expected_values, &field_id)
                    .unwrap_or_else(|| panic!("presence of {field_id:?} is expected"))
                    .split(' ')
                    .into_iter()
                    .map(from_hex_str)
                    .collect();
                let field_data = central_header
                    .extras
                    .field_data(id)
                    .unwrap_or_else(|| panic!("presence of extras field {id_str} is expected"));
                if clipped {
                    assert_eq!(
                        expected_data,
                        &field_data[..data_clipping_len],
                        "Clipped content of extra field {id_str} don't match for {path}::{name} \
                        for central header"
                    );
                } else {
                    assert_eq!(
                        expected_data, field_data,
                        "Content of extra field {id_str} don't match for {path}::{name} \
                        for central header"
                    );
                }
            });
        }

        let expected_file_type =
            consume_as_string(expected_values, "file_type").expect("file_type is mandatory");
        assert_eq!(
            entry.file_type().as_ref(),
            expected_file_type,
            "File types don't match for {path}::{name} for central header"
        );

        if let Some(expected_unix_file_attributes) =
            consume_as_number::<u16>(expected_values, "unix_file_attributes")
        {
            assert_eq!(
                entry.unix_file_attributes(),
                expected_unix_file_attributes,
                "Unix file attributes don't match for {path}::{name} entry central header"
            );
        }

        let expected_msdos_external_attributes: u8 =
            consume_as_number(expected_values, "msdos_external_attributes")
                .expect("msdos_external_attributes is mandatory field");
        assert_eq!(
            entry.msdos_file_attributes(),
            expected_msdos_external_attributes,
            "MS-DOS file attributes don't match for {path}::{name} entry central header"
        );

        if let Some(expected_non_msdos_external_attributes) =
            consume_as_number::<u32>(expected_values, "non_msdos_external_attributes")
        {
            assert_eq!(
                entry.non_msdos_file_attributes(),
                u32::to_le_bytes(expected_non_msdos_external_attributes)[0..3],
                "Non-MSDOS external file attributes don't match for {path}::{name} entry \
                for central header"
            );
        }

        let expected_encrypted = match consume_as_string(expected_values, "encryption")
            .expect("encrypted is mandatory")
            .as_str()
        {
            "encrypted" => true,
            "not encrypted" => false,
            _ => unimplemented!(),
        };
        assert_eq!(
            entry.is_encrypted(),
            expected_encrypted,
            "Encryption status don't match for {path}::{name} entry"
        );

        // Explicitly consume values which might have left "to process" in the `expected_values` HashMap:
        //
        // If filenames match, there is no much reason to compare file name lengths:
        let _ = consume_as_string(expected_values, "file_name_length").unwrap();

        // Artificial record type added by zipinfo parsing script:
        let _ = consume_as_string(expected_values, "entry_type").unwrap();

        // We decided to skip parsing/comparing local extra fields with id 0x5855, as their
        // appearance in zipinfo output doesn't seems to correspond to a specification.
        if let Some(local_extra_field_id) =
            consume_as_string(expected_values, "local_extra_field_id")
        {
            assert_eq!(local_extra_field_id, "0x5855");
            let _ =
                consume_as_string(expected_values, "local_extra_field_0x5855_data_len").unwrap();
        }

        // There is no corresponding marker to account for "There are an extra xxx bytes preceding
        // this file." messages in zipinfo output
        let _ = consume_as_string(expected_values, "extra_preceding").unwrap();

        assert!(
            expected_values.is_empty(),
            "Some key/value pairs have not been consumed for comparison (or explicitly dropped) \
            for {path}::{name} entry: {expected_values:#?}"
        );
    }

    assert!(
        expected_iter.next().is_none(),
        "Some zipinfo entries have not been processed at all"
    );
}

fn consume_as_string(map: &mut HashMap<String, Value>, key: &str) -> Option<String> {
    match map.remove(key)? {
        Value::String(string) => Some(string),
        _ => panic!("all Values in the HashMap are expected to be json Strings"),
    }
}

fn consume_as_number<T>(map: &mut HashMap<String, Value>, key: &str) -> Option<T>
where
    T: FromStr + Num,
    <T as FromStr>::Err: Debug,
    <T as Num>::FromStrRadixErr: Debug,
{
    let string = consume_as_string(map, key)?;
    if string.starts_with("0x") {
        Some(<T>::from_str_radix(&string[2..], 16).unwrap_or_else(|e| {
            panic!("failed to convert hex string {string:?} to decimal: {e:?}")
        }))
    } else if string.starts_with("0o") {
        Some(<T>::from_str_radix(&string[2..], 8).unwrap_or_else(|e| {
            panic!("failed to convert octal string {string:?} to decimal: {e:?}")
        }))
    } else {
        Some(
            string
                .parse::<T>()
                .unwrap_or_else(|e| panic!("failed to parse string {string:?} to decimal: {e:?}")),
        )
    }
}

fn from_hex_str<T>(hex_str: &str) -> T
where
    T: FromStr + Num,
    <T as Num>::FromStrRadixErr: Debug,
{
    let hex_str = {
        if hex_str.starts_with("0x") {
            &hex_str[2..]
        } else {
            hex_str
        }
    };
    <T>::from_str_radix(hex_str, 16)
        .unwrap_or_else(|e| panic!("failed to convert hex {hex_str:?} to decimal: {e:?}"))
}

fn index_to_os_str(id: u8) -> &'static str {
    match id {
        0 => "MS-DOS, OS/2 or NT FAT",
        3 => "Unix",
        _ => unimplemented!(),
    }
}

fn index_to_compression_method_str(id: u16) -> &'static str {
    match id {
        0 => "none (stored)",
        1 => "shrunk",
        5 => "reduced (factor 4)",
        6 => "imploded",
        8 => "deflated",
        9 => "deflated (enhanced-64k)",
        12 => "bzipped",
        14 => "LZMA-ed",
        93 => "unknown (93)",
        95 => "unknown (95)",
        99 => "unknown (99)",
        _ => unimplemented!(),
    }
}

fn byte_to_version_string(version: u8) -> String {
    format!("{}.{}", version / 10, version % 10)
}

#[test]
fn random_access() -> Result<(), std::io::Error> {
    let zip = ZipRandomAccess::new(File::open("tests/test_data/alice.zip")?)?;
    println!("{:?}", zip.names().collect::<Vec<&str>>());
    assert!(zip.get_entry_by_name("not present").is_none());
    let entry = zip
        .get_entry_by_name("IMPLODE.TXT")
        .expect("IMPLODE.TXT not found");
    assert!(entry.get_error().is_none());
    assert_eq!(entry.get_compression_method(), 6);
    assert_eq!(entry.get_compressed_size(), 0xed8c);
    let entry = zip
        .get_entry_by_name("REDUCE.TXT")
        .expect("REDUCE.TXT not found");
    assert!(entry.get_error().is_none());
    assert_eq!(entry.get_compression_method(), 5);
    assert_eq!(entry.get_compressed_size(), 0x11911);
    let entry = zip
        .get_entry_by_name("SHRINK.TXT")
        .expect("SHRINK.TXT not found");
    assert!(entry.get_error().is_none());
    assert_eq!(entry.get_compression_method(), 1);
    assert_eq!(entry.get_compressed_size(), 0x0000fdf6);
    Ok(())
}
