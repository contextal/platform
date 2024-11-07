use cdfs::{
    iso::*,
    udf::{ecma167::*, *},
};
use std::io::Read;

#[test]
fn iso_level1() {
    let path = "tests/test_data/iso1_ss2048_bs1024.iso";
    let mut file = std::fs::File::open(path).expect("Failed to open test file {path}");
    let mut iso = Iso9660::new(&mut file).expect("Failed to parse {path} as iso9660");
    assert_eq!(iso.image_header_size, 0, "Header size mismatch");
    assert_eq!(iso.raw_sector_size, 2048, "Sector size mismatch");
    assert_eq!(iso.volumes.len(), 1, "Volumes count mismatch");
    let mut volrdr = iso
        .open_volume(0)
        .expect("Volume expeceted but not found")
        .expect("Failed to open volume");
    let info = &volrdr.volume;
    assert!(info.is_primary(), "Exepcted primary descriptor");
    assert_eq!(info.version, 1, "Wrong volume version");
    assert_eq!(info.flags, 0, "Wrong volume flags");
    assert_eq!(info.system_id, "systemid", "Wrong volume system id");
    assert_eq!(info.volume_id, "volumeid", "Wrong volume id");
    assert!(info.escapes.iter().find(|v| **v != 0).is_none());
    assert_eq!(info.volume_set_size, 1, "Wrong volume set size");
    assert_eq!(info.volume_sequence_number, 1, "Wrong volume sequence");
    assert_eq!(info.block_size, 1024, "Wrong block size");
    assert_eq!(info.volume_set_id, "volsetid", "Wrong volume set id");
    assert_eq!(info.publisher_id, "publisher", "Wrong volume publisher id");
    assert_eq!(info.preparer_id, "preparer", "Wrong volume preparer id");
    assert_eq!(
        info.application_id, "application_id",
        "Wrong volume application id"
    );
    assert_eq!(
        info.copyright_file_id, "copyright.file",
        "Wrong volume copyright file id"
    );
    assert_eq!(
        info.abstract_file_id, "abstract.file",
        "Wrong volume abstract file id"
    );
    assert_eq!(
        info.bibliographic_file_id, "biblio.file",
        "Wrong volume bibliographic file id"
    );
    assert_eq!(
        info.volume_creation_dt,
        IsoDate::Valid(time::macros::datetime!(2023-09-16 01:23:45.67 UTC)),
        "Wrong volume creation date"
    );
    assert_eq!(
        info.volume_modification_dt,
        IsoDate::Valid(time::macros::datetime!(2023-10-17 17:42:27.11 +2)),
        "Wrong volume modification date"
    );
    assert_eq!(
        info.volume_expiration_dt,
        IsoDate::Unset,
        "Wrong volume expiratication date"
    );
    assert_eq!(
        info.volume_effective_dt,
        IsoDate::Invalid,
        "Wrong volume effective date"
    );
    assert_eq!(
        info.file_structure_version, 1,
        "Wrong volume structure version"
    );
    assert!(!info.is_joliet, "Wrong joliet flag");
    let (record, mut isofile) = volrdr.open_file(0).expect("Failed to open file #0");
    assert_eq!(record.data_length, 27, "File #0 wrong length");
    assert_eq!(
        record.recording_dt,
        IsoDate::Valid(time::macros::datetime!(2023-09-16 16:20:53.0 +2)),
        "File #0 wrong date"
    );
    assert!(!record.is_interleaved(), "File #0 is interleaved");
    assert_eq!(record.file_id, "/RFILE", "File #0 wrong name");
    let mut buf = String::new();
    isofile
        .read_to_string(&mut buf)
        .expect("Failed to read file #0");
    assert_eq!(
        buf, "this file sits in the root\n",
        "File #0 has wrong content"
    );
    let (record, mut isofile) = volrdr.open_file(1).expect("Failed to open file #1");
    assert_eq!(record.data_length, 10880, "File #1 wrong length");
    assert_eq!(record.file_id, "/DIR/LARGE", "File #1 wrong name");
    let mut buf = String::new();
    isofile
        .read_to_string(&mut buf)
        .expect("Failed to read file #1");
    assert_eq!(
        buf.lines().fold(0usize, |i, l| {
            assert_eq!(
                l, "this file exceeds the sector size",
                "File #1 has wrong content"
            );
            i + 1
        }),
        320,
        "File #1 has wrong number of lines"
    );
    let (record, mut isofile) = volrdr.open_file(2).expect("Failed to open file #2");
    assert_eq!(record.data_length, 18, "File #2 wrong length");
    assert_eq!(record.file_id, "/DIR/SMALL", "File #2 wrong name");
    let mut buf = String::new();
    isofile
        .read_to_string(&mut buf)
        .expect("Failed to read file #1");
    assert_eq!(buf, "i am a small file\n", "File #2 has wrong content");
    assert!(volrdr.open_file(3).is_none(), "Failed to open file #3");
    assert!(iso.open_volume(1).is_none(), "Found unexpected volume");
}

#[test]
fn iso_level1_with_header_and_padding() {
    let path = "tests/test_data/header_and_padding.iso";
    let mut file = std::fs::File::open(path).expect("Failed to open test file {path}");
    let mut iso = Iso9660::new(&mut file).expect("Failed to parse {path} as iso9660");
    assert_eq!(iso.image_header_size, 240, "Header size mismatch");
    assert_eq!(iso.raw_sector_size, 2093, "Sector size mismatch");
    assert_eq!(iso.volumes.len(), 1, "Volumes count mismatch");
    iso.open_volume(0)
        .expect("Volume expeceted but not found")
        .expect("Failed to open volume");
}

#[test]
fn joliet() {
    let path = "tests/test_data/joliet.iso";
    let mut file = std::fs::File::open(path).expect("Failed to open test file {path}");
    let mut iso = Iso9660::new(&mut file).expect("Failed to parse {path} as iso9660");
    assert_eq!(iso.image_header_size, 0, "Header size mismatch");
    assert_eq!(iso.raw_sector_size, 2048, "Sector size mismatch");
    assert_eq!(iso.volumes.len(), 2, "Volumes count mismatch");
    let mut volrdr = iso
        .open_volume(0)
        .expect("Volume #0 expeceted but not found")
        .expect("Failed to open volume #0");
    let info = &volrdr.volume;
    assert!(info.is_primary(), "Exepcted primary descriptor");
    assert!(!info.is_joliet, "Wrong joliet flag");
    assert_eq!(info.block_size, 2048, "Wrong block size");
    let (record, _) = volrdr.open_file(0).expect("Failed to open file #0.0");
    assert_eq!(
        record.file_id,
        "/___________________/NON_COMPLIANT_FILE.NAME"
    );
    let mut volrdr = iso
        .open_volume(1)
        .expect("Volume #1 expeceted but not found")
        .expect("Failed to open volume #1");
    let info = &volrdr.volume;
    assert!(!info.is_primary(), "Exepcted primary descriptor");
    assert!(info.is_joliet, "Wrong joliet flag");
    assert_eq!(info.block_size, 2048, "Wrong block size");
    let (record, _) = volrdr.open_file(0).expect("Failed to open file #1.0");
    assert_eq!(record.file_id, "/Ðı®€©ŧø®¥/Non.Compliant file.name");
}

#[test]
fn interleaved() {
    let path = "tests/test_data/interleaved.iso";
    let mut file = std::fs::File::open(path).expect("Failed to open test file {path}");
    let mut iso = Iso9660::new(&mut file).expect("Failed to parse {path} as iso9660");
    let mut volrdr = iso
        .open_volume(0)
        .expect("Volume #0 expeceted but not found")
        .expect("Failed to open volume #0");
    let (record, isofile) = volrdr.open_file(0).expect("Failed to open file #0.0");
    assert!(record.is_interleaved());
    assert_eq!(
        isofile.bytes().fold(0u64, |i, l| {
            assert_eq!(
                l.expect("failed to read from extent"),
                b'a',
                "File #0 has wrong content"
            );
            i + 1
        }),
        4096,
        "File #0 has wrong content length"
    );
}

#[test]
fn cross_sector_directory() {
    let path = "tests/test_data/cross_sector_dir.iso";
    let mut file = std::fs::File::open(path).expect("Failed to open test file {path}");
    let mut iso = Iso9660::new(&mut file).expect("Failed to parse {path} as iso9660");
    let mut volrdr = iso
        .open_volume(0)
        .expect("Volume #0 expeceted but not found")
        .expect("Failed to open volume #0");
    for (i, name) in "ABCDEFG".chars().enumerate() {
        let (record, _) = volrdr.open_file(i).expect("Failed to open file #0.{i}");
        assert_eq!(record.file_id, format!("/{name}"));
    }
}

#[test]
fn udf_ss512() {
    let path = "tests/test_data/udf-512.iso";
    let mut file = std::fs::File::open(path).expect("Failed to open test file {path}");
    let mut udf = Udf::new(&mut file).expect("Failed to parse {path} as UDF");
    assert_eq!(udf.ss, 512, "UDF sector size mismatch");
    assert_eq!(udf.vds.pvds.len(), 1, "Primary volume count mismatch");
    assert_eq!(udf.vds.lvds.len(), 1, "Logical volume count mismatch");
    assert_eq!(udf.vds.pds.len(), 1, "Partition count mismatch");
    assert_eq!(udf.vds.iuvds.len(), 1, "Implementation Use count mismatch");
    let pvd = &udf.vds.pvds[0];
    assert_eq!(
        pvd.identifier.to_string(),
        "vid",
        "Primary volume identifier mismatch"
    );
    assert_eq!(
        pvd.set_identifier.to_string(),
        "0123456789abcdefLinuxUDF",
        "Primary volume set identifier mismatch"
    );
    assert!(
        pvd.desc_charset.is_osta_cs0(),
        "Unexpected Descriptor charset"
    );
    if let UdfDate::ValidTz(t) = &pvd.datetime {
        assert_eq!(
            t,
            &time::macros::datetime!(2024-01-05 18:31:39.302465 +02:00:00),
            "Partition date mismatch"
        );
    } else {
        panic!("Unexpected partition date");
    }
    let iuvd = &udf.vds.iuvds[0];
    assert!(
        iuvd.is_compliant(),
        "Implementation use is not UDF compliant"
    );
    assert!(
        iuvd.lv_charset.is_osta_cs0(),
        "Unexpected Implementation use charset"
    );
    assert_eq!(
        iuvd.lv_identifier.to_string(),
        "lvid",
        "Unexpected Implementation identifier"
    );
    assert_eq!(
        iuvd.lv_info1.to_string(),
        "owner",
        "Unexpected Implementation owner"
    );
    assert_eq!(
        iuvd.lv_info2.to_string(),
        "org",
        "Unexpected Implementation org"
    );
    assert_eq!(
        iuvd.lv_info3.to_string(),
        "contact",
        "Unexpected Implementation contact"
    );
    let mut vol = udf
        .open_volume(0)
        .expect("The volume was not found")
        .expect("Failed to open volume");
    let lvd = vol.lvd();
    assert!(
        lvd.desc_charset.is_osta_cs0(),
        "Unexpected Descriptor charset"
    );
    assert_eq!(
        lvd.identifier.to_string(),
        "lvid",
        "Unexpected Logical Volume identifier"
    );
    assert_eq!(lvd.block_size, 512, "Unexpected Logical Volume block size");
    assert!(
        lvd.domain_identifier.is_osta_udf_compliant(),
        "Domain identifier is not compliant"
    );
    let mut nfile = 0usize;
    let mut mask = 0u8;
    while let Some((fname, mut r)) = vol.open_file(nfile) {
        nfile += 1;
        match fname.as_str() {
            "/sparse" => {
                let mut nbytes = 0u64;
                assert!(r.bytes().all(|b| {
                    nbytes += 1;
                    b.expect("Error reading sparse file") == 0
                }));
                assert_eq!(nbytes, 16384, "Inconsistent sparse file length");
                mask |= 0b0001;
            }
            "/dir1/embedded_ad" => {
                let mut s = String::new();
                r.read_to_string(&mut s).expect("Failed to read file");
                assert_eq!(s, "this text lives inside the allocation descriptor\n");
                mask |= 0b0010;
            }
            "/dir1/allocated_ad" => {
                let mut nbytes = 0u64;
                assert!(r.bytes().all(|b| {
                    nbytes += 1;
                    b.expect("Error reading allocated file") == b'a'
                }));
                assert_eq!(nbytes, 512, "Inconsistent allocated file length");
                mask |= 0b0100;
            }
            "/dir1/Ðì®/€µþŧ¥" => {
                assert_eq!(r.bytes().count(), 0, "Empty file is not empty");
                mask |= 0b1000;
            }
            entry => {
                panic!("Unexpected entry {}", entry);
            }
        }
    }
    assert_eq!(nfile, 4, "Inconsistent file count");
    assert_eq!(mask, 0b1111, "Inconsistent file mask");
}
