use ctxole::{crypto, oleps, Ole};
use sha1::{Digest as _, Sha1};
use std::collections::HashMap;
use std::{
    fs,
    io::{self, Write},
};

struct HashingReader {
    size: usize,
    hash: Sha1,
}

impl HashingReader {
    fn new() -> Self {
        Self {
            size: 0,
            hash: Sha1::new(),
        }
    }

    fn finish(&mut self) -> String {
        let mut res = [0u8; 20];
        self.hash.finalize_into_reset((&mut res).into());
        self.size = 0;
        res.into_iter().map(|v| format!("{:02x}", v)).collect()
    }
}

impl Write for HashingReader {
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        self.hash.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

#[test]
fn test_plain() -> Result<(), io::Error> {
    let f = fs::File::open("tests/data/plain.doc")?;
    let ole = Ole::new(f)?;
    assert_eq!(ole.version(), (3, 62));
    assert!(ole.anomalies().is_empty());
    assert_eq!(ole.num_entries(), 8);
    let entry = ole.get_entry_by_id(0)?;
    assert_eq!(entry.id, 0);
    assert_eq!(entry.objtype, 5);
    assert_eq!(entry.name, "Root Entry");
    assert!(entry.anomalies.is_empty());
    assert!(entry.is_allocated());
    assert!(entry.is_storage());
    let entry = ole.get_entry_by_id(1)?;
    assert_eq!(entry.id, 1);
    assert_eq!(entry.objtype, 2);
    assert_eq!(entry.name, "Data");
    assert_eq!(entry.size, 4096);
    assert!(entry.anomalies.is_empty());
    assert!(entry.is_allocated());
    assert!(!entry.is_storage());
    let entry = ole.get_entry_by_id(2)?;
    assert_eq!(entry.id, 2);
    assert_eq!(entry.objtype, 2);
    assert_eq!(entry.name, "1Table");
    assert_eq!(entry.size, 6427);
    assert!(entry.anomalies.is_empty());
    assert!(entry.is_allocated());
    assert!(!entry.is_storage());
    let entry = ole.get_entry_by_id(3)?;
    assert_eq!(entry.id, 3);
    assert_eq!(entry.objtype, 2);
    assert_eq!(entry.name, "WordDocument");
    assert_eq!(entry.size, 4096);
    assert!(entry.anomalies.is_empty());
    assert!(entry.is_allocated());
    assert!(!entry.is_storage());
    let entry = ole.get_entry_by_id(4)?;
    assert_eq!(entry.id, 4);
    assert_eq!(entry.objtype, 2);
    assert_eq!(entry.name, "\u{5}SummaryInformation");
    assert_eq!(entry.size, 4096);
    assert!(entry.anomalies.is_empty());
    assert!(entry.is_allocated());
    assert!(!entry.is_storage());
    let entry = ole.get_entry_by_id(5)?;
    assert_eq!(entry.id, 5);
    assert_eq!(entry.objtype, 2);
    assert_eq!(entry.name, "\u{5}DocumentSummaryInformation");
    assert_eq!(entry.size, 4096);
    assert!(entry.anomalies.is_empty());
    assert!(entry.is_allocated());
    assert!(!entry.is_storage());
    let entry = ole.get_entry_by_id(6)?;
    assert_eq!(entry.id, 6);
    assert_eq!(entry.objtype, 2);
    assert_eq!(entry.name, "\u{1}CompObj");
    assert_eq!(entry.size, 121);
    assert!(entry.anomalies.is_empty());
    assert!(entry.is_allocated());
    assert!(!entry.is_storage());
    let entry = ole.get_entry_by_id(7)?;
    assert_eq!(entry.id, 7);
    assert_eq!(entry.objtype, 0);
    assert_eq!(entry.name, "");
    assert_eq!(entry.size, 0);
    assert!(entry.anomalies.is_empty());
    assert!(!entry.is_allocated());
    assert!(!entry.is_storage());
    assert!(ole.get_entry_by_id(8).is_err());

    let mut map: HashMap<String, String> = HashMap::new();
    let mut w = HashingReader::new();
    for (name, entry) in ole.ftw() {
        io::copy(&mut ole.get_stream_reader(&entry), &mut w)?;
        let hash = w.finish();
        map.insert(name, hash);
    }
    const REF_HASHES: &[(&str, &str)] = &[
        ("b4fe6cc8fa908f3246d51da9ee68a9211b9b13ff", "1Table"),
        ("1ceaf73df40e531df3bfb26b4fb7cd95fb7bff1d", "Data"),
        ("cbc000e2273f21aec9f99b1e7c7628656dc9f3d5", "WordDocument"),
        ("9b1f0e3d5aa1b3559afe154e8b459d0b8ca339f3", "\u{1}CompObj"),
        (
            "daa671418f0164c52eee0573350350e85dbb98ba",
            "\u{5}DocumentSummaryInformation",
        ),
        (
            "0bf1da509e728467ca1b1e25f3b314e65a6c373f",
            "\u{5}SummaryInformation",
        ),
    ];
    for (ref_hash, ref_name) in REF_HASHES {
        assert_eq!(map.get(*ref_name).unwrap(), *ref_hash);
    }
    for (ref_hash, ref_name) in REF_HASHES {
        let entry = ole.get_entry_by_name(ref_name)?;
        io::copy(&mut ole.get_stream_reader(&entry), &mut w)?;
        assert_eq!(w.finish(), *ref_hash);
    }

    assert!(ole.get_decryptor().is_err());

    let entry = ole.get_entry_by_name("\u{5}SummaryInformation")?;
    let si = oleps::SummaryInformation::new(&mut ole.get_stream_reader(&entry))?;
    assert_eq!(si.title.unwrap(), "title\0\0");
    assert_eq!(si.subject.unwrap(), "subject");
    assert_eq!(si.author.unwrap(), "author\0");
    assert_eq!(si.keywords.unwrap(), "label1 label2\0\0");
    assert_eq!(si.comments.unwrap(), "some\ncomments\nhere\0");
    assert_eq!(si.template.unwrap(), "Normal.dotm");
    assert_eq!(si.last_author.unwrap(), "Windows User\0\0\0");
    assert_eq!(si.pages.unwrap(), 1);
    assert_eq!(si.words.unwrap(), 1);
    assert_eq!(si.chars.unwrap(), 9);
    assert!(!si.has_thumbnail);
    assert_eq!(si.application_name.unwrap(), "Microsoft Office Word\0\0");
    assert!(!si.password_protected);
    assert!(!si.readonly_recommend);
    assert!(!si.readonly_enforced);
    assert!(!si.locked);
    assert!(!si.has_bad_entries);
    assert!(!si.has_dups);
    assert!(!si.has_bad_type);

    let entry = ole.get_entry_by_name("\u{5}DocumentSummaryInformation")?;
    let dsi = oleps::DocumentSummaryInformation::new(&mut ole.get_stream_reader(&entry))?;
    assert_eq!(dsi.category.unwrap(), "category\0\0\0");
    assert_eq!(dsi.manager.unwrap(), "manager");
    assert_eq!(dsi.company.unwrap(), "company");
    assert_eq!(dsi.lines.unwrap(), 1);
    assert_eq!(dsi.paragraphs.unwrap(), 1);
    assert_eq!(dsi.characters.unwrap(), 9);
    assert_eq!(dsi.content_status.unwrap(), "status\0");
    if let oleps::UserDefinedProperty::String(ref v) = dsi.user_defined_properties["custom text"] {
        assert_eq!(v, "custom value\0\0\0");
    } else {
        panic!("missing custom text");
    }
    if let oleps::UserDefinedProperty::Int(v) = dsi.user_defined_properties["custom number"] {
        assert_eq!(v, 42);
    } else {
        panic!("missing custom number");
    }
    if let oleps::UserDefinedProperty::Bool(v) = dsi.user_defined_properties["custom boolean"] {
        assert!(v);
    } else {
        panic!("missing custom boolean");
    }
    Ok(())
}

#[test]
fn test_standard_encryption() -> Result<(), io::Error> {
    let f = fs::File::open("tests/data/protected_book.xlsx")?;
    let ole = Ole::new(f)?;
    let decryptor = ole.get_decryptor()?;
    let ds = decryptor.data_spaces.as_ref().unwrap();
    let vi = &ds.version_info;
    assert_eq!(vi.feature_identifier, "Microsoft.Container.DataSpaces");
    assert_eq!(vi.reader_version.major, 1);
    assert_eq!(vi.reader_version.minor, 0);
    assert_eq!(vi.updater_version.major, 1);
    assert_eq!(vi.updater_version.minor, 0);
    assert_eq!(vi.writer_version.major, 1);
    assert_eq!(vi.writer_version.minor, 0);
    let ti = decryptor.transform_info.as_ref().unwrap();
    assert_eq!(ti.name, "AES 128");
    assert_eq!(ti.block_size, 16);
    let ei = &decryptor.encryption_info;
    assert_eq!(ei.version.major, 3);
    assert_eq!(ei.version.minor, 2);
    if let crypto::EncryptionType::Standard(ref std) = ei.encryption_type {
        assert!(std.flags.crypto_api);
        assert!(std.flags.aes);
        assert!(!std.flags.external);
        assert!(!std.flags.doc_props);
        assert_eq!(std.header.alg_id, 0x660e);
        assert_eq!(std.header.alg_id_hash, 0x8004);
        assert_eq!(std.header.key_size, 16);
        assert!(matches!(
            std.header.algorithm,
            crypto::EncryptionAlgo::Aes128
        ));
    } else {
        panic!("Standard encryption expected");
    }
    assert!(decryptor.get_key("BadPass").is_none());
    let key = decryptor.get_key("VelvetSweatshop").unwrap();
    let mut w = HashingReader::new();
    decryptor.decrypt(&key, &ole, &mut w)?;
    assert_eq!(w.finish(), "6860d845ce7287dfe3dc6014b0ec7a15a22bbc4c");
    Ok(())
}

#[test]
fn test_agile_encryption() -> Result<(), io::Error> {
    let f = fs::File::open("tests/data/encrypted.docx")?;
    let ole = Ole::new(f)?;
    let decryptor = ole.get_decryptor()?;
    eprintln!("{:#?}", decryptor);
    let ds = decryptor.data_spaces.as_ref().unwrap();
    let vi = &ds.version_info;
    assert_eq!(vi.feature_identifier, "Microsoft.Container.DataSpaces");
    assert_eq!(vi.reader_version.major, 1);
    assert_eq!(vi.reader_version.minor, 0);
    assert_eq!(vi.updater_version.major, 1);
    assert_eq!(vi.updater_version.minor, 0);
    assert_eq!(vi.writer_version.major, 1);
    assert_eq!(vi.writer_version.minor, 0);
    let ei = &decryptor.encryption_info;
    assert_eq!(ei.version.major, 4);
    assert_eq!(ei.version.minor, 4);
    if let crypto::EncryptionType::Agile(ref agile) = ei.encryption_type {
        let kd = &agile.key_data;
        assert_eq!(kd.salt_size, 16);
        assert_eq!(kd.block_size, 16);
        assert_eq!(kd.key_bits, 256);
        assert_eq!(kd.hash_size, 64);
        assert_eq!(kd.cipher_algorithm, "AES");
        assert_eq!(kd.hash_algorithm, "SHA512");
        assert_eq!(kd.cipher_chaining, "ChainingModeCBC");
        let ek = &agile.key_encryptors.key_encryptor.encrypted_key;
        assert_eq!(ek.salt_size, 16);
        assert_eq!(ek.block_size, 16);
        assert_eq!(ek.key_bits, 256);
        assert_eq!(ek.hash_size, 64);
        assert_eq!(ek.cipher_algorithm, "AES");
        assert_eq!(ek.hash_algorithm, "SHA512");
        assert_eq!(ek.cipher_chaining, "ChainingModeCBC");
        assert_eq!(ek.spin_count, 100_000);
    } else {
        panic!("Agile encryption expected");
    }
    let key = decryptor.get_key("contextal").unwrap();
    let mut w = HashingReader::new();
    decryptor.decrypt(&key, &ole, &mut w)?;
    assert_eq!(w.finish(), "8cca753a1f41941cc1b6a6f847c42549f5ed96ef");
    Ok(())
}
