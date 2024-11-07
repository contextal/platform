use crate::process_request;
use backend_utils::objects::{BackendRequest, BackendResultKind, Info, Metadata};
use std::collections::HashMap;
use tempfile::TempDir;
use unrar_rs::config::Config;

fn mock_env_for_file(file: &str) -> (Config, BackendRequest, TempDir) {
    let temp_dir = TempDir::new().unwrap();

    let config = Config {
        objects_path: "tests/test_data".into(),
        output_path: temp_dir.path().to_string_lossy().into(),
        ..Config::new().expect("a valid config")
    };

    let request = BackendRequest {
        object: Info {
            org: "test".into(),
            object_id: file.into(),
            object_type: "test".into(),
            object_subtype: None,
            recursion_level: 1,
            size: 1254426,
            hashes: HashMap::new(),
            ctime: 1695645418.7196224,
        },
        symbols: vec![],
        relation_metadata: Metadata::new(),
    };

    (config, request, temp_dir)
}

#[test]
fn plain() {
    let (config, request, _temp_dir) = mock_env_for_file("plain.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(!backend_result.object_metadata["has_comment"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_recovery_record"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["is_locked"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["is_solid"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(backend_result.children.len(), 2);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
}

#[test]
fn blake2_hash() {
    let (config, request, _temp_dir) = mock_env_for_file("blake2.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result
        .children
        .iter()
        .all(|v| !v.relation_metadata["blake2_hash"]
            .as_str()
            .expect("a string is expected")
            .is_empty()));
}

#[test]
fn version4() {
    let (config, request, _temp_dir) = mock_env_for_file("version4.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(!backend_result.object_metadata["has_comment"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_recovery_record"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["is_locked"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["is_solid"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(backend_result.children.len(), 3);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
}

#[test]
fn with_comment() {
    let (config, request, _temp_dir) = mock_env_for_file("with-comment.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.object_metadata["has_comment"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(
        backend_result.object_metadata["comment"]
            .as_str()
            .expect("a string is expected"),
        "A dummy archive comment\n"
    );
}

#[test]
fn with_comment_unicode() {
    let (config, request, _temp_dir) = mock_env_for_file("with-comment-unicode.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.object_metadata["has_comment"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(
        backend_result.object_metadata["comment"]
            .as_str()
            .expect("a string is expected"),
        "Archive коментар 123\n"
    );
}

#[test]
fn with_comment_encrypted() {
    let (config, mut request, _temp_dir) =
        mock_env_for_file("with-comment-encrypted-перевірка123.rar");

    let possible_passwords = ["wrong", "перевірка123"].map(|s| s.to_string()).to_vec();
    let mut global = serde_json::Map::<String, serde_json::Value>::new();
    global.insert("possible_passwords".into(), possible_passwords.into());
    request
        .relation_metadata
        .insert("_global".into(), global.into());

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.object_metadata["has_comment"]
        .as_bool()
        .expect("a bool"));
    assert!(backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(
        backend_result.object_metadata["comment"]
            .as_str()
            .expect("a string is expected"),
        "Перевірка 1-2-3\n"
    );
}

#[test]
fn solid() {
    let (config, request, _temp_dir) = mock_env_for_file("solid.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(!backend_result.object_metadata["has_comment"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_recovery_record"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["is_locked"]
        .as_bool()
        .expect("a bool"));
    assert!(backend_result.object_metadata["is_solid"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(backend_result.children.len(), 3);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
}

#[test]
fn encrypted_data() {
    let (config, mut request, _temp_dir) = mock_env_for_file("encrypted-data-fGzq5yKw.rar");

    let possible_passwords = ["wrong", "fGzq5yKw"].map(|s| s.to_string()).to_vec();
    let mut global = serde_json::Map::<String, serde_json::Value>::new();
    global.insert("possible_passwords".into(), possible_passwords.into());
    request
        .relation_metadata
        .insert("_global".into(), global.into());
    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(backend_result.children.len(), 3);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
    assert!(backend_result
        .children
        .iter()
        .all(|v| v.relation_metadata["is_encrypted"]
            .as_bool()
            .expect("a bool is expected")));
}

#[test]
fn encrypted_headers_and_data() {
    let (config, mut request, _temp_dir) =
        mock_env_for_file("encrypted-headers-and-data-fGzq5yKw.rar");

    let possible_passwords = ["wrong", "fGzq5yKw"].map(|s| s.to_string()).to_vec();
    let mut global = serde_json::Map::<String, serde_json::Value>::new();
    global.insert("possible_passwords".into(), possible_passwords.into());
    request
        .relation_metadata
        .insert("_global".into(), global.into());

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(backend_result.children.len(), 1);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
    assert!(backend_result
        .children
        .iter()
        .all(|v| v.relation_metadata["is_encrypted"]
            .as_bool()
            .expect("a bool is expected")));
}

#[test]
fn encrypted_headers_and_data_unicode_password() {
    let (config, mut request, _temp_dir) =
        mock_env_for_file("encrypted-headers-and-data-перевірка123.rar");

    let possible_passwords = ["wrong", "перевірка123"].map(|s| s.to_string()).to_vec();
    let mut global = serde_json::Map::<String, serde_json::Value>::new();
    global.insert("possible_passwords".into(), possible_passwords.into());
    request
        .relation_metadata
        .insert("_global".into(), global.into());

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(!backend_result.object_metadata["has_comment"]
        .as_bool()
        .expect("a bool"));
    assert!(backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(backend_result.children.len(), 3);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
    assert!(backend_result
        .children
        .iter()
        .all(|v| v.relation_metadata["is_encrypted"]
            .as_bool()
            .expect("a bool is expected")));
}

#[test]
fn encrypted_data_no_password() {
    let (config, request, _temp_dir) = mock_env_for_file("encrypted-data-fGzq5yKw.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(backend_result.children.len(), 3);
    assert!(backend_result.children.iter().all(|v| v.path.is_none()));
    assert!(backend_result
        .children
        .iter()
        .all(|v| v.symbols.contains(&"ENCRYPTED".into())));
    assert!(backend_result
        .children
        .iter()
        .all(|v| v.relation_metadata["is_encrypted"]
            .as_bool()
            .expect("a bool is expected")));
}

#[test]
fn encrypted_headers_and_data_no_password() {
    let (config, request, _temp_dir) = mock_env_for_file("encrypted-headers-and-data-fGzq5yKw.rar");

    let BackendResultKind::error(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::error is expected")
    };

    assert!(backend_result.as_str().contains("missing password"));
}

#[test]
fn encrypted_data_invalid_password() {
    let (config, mut request, _temp_dir) = mock_env_for_file("encrypted-data-fGzq5yKw.rar");

    let possible_passwords = ["wrong1", "wrong2"].map(|s| s.to_string()).to_vec();
    let mut global = serde_json::Map::<String, serde_json::Value>::new();
    global.insert("possible_passwords".into(), possible_passwords.into());
    request
        .relation_metadata
        .insert("_global".into(), global.into());

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));

    assert_eq!(backend_result.children.len(), 3);
    assert!(backend_result.children.iter().all(|v| v.path.is_none()));
    assert!(backend_result
        .children
        .iter()
        .all(|v| v.relation_metadata["is_encrypted"]
            .as_bool()
            .expect("a bool is expected")));
    assert!(backend_result
        .children
        .iter()
        .all(|v| v.symbols.contains(&"ENCRYPTED".into())));
    assert!(backend_result
        .children
        .iter()
        .all(|v| v.symbols.contains(&"INVALID_PASSWORD".into())));
}

#[test]
fn encrypted_headers_and_data_invalid_password() {
    let (config, mut request, _temp_dir) =
        mock_env_for_file("encrypted-headers-and-data-fGzq5yKw.rar");

    let possible_passwords = ["wrong1", "wrong2"].map(|s| s.to_string()).to_vec();
    let mut global = serde_json::Map::<String, serde_json::Value>::new();
    global.insert("possible_passwords".into(), possible_passwords.into());
    request
        .relation_metadata
        .insert("_global".into(), global.into());

    let BackendResultKind::error(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::error is expected")
    };

    assert!(backend_result.as_str().contains("invalid password"));
}

#[test]
fn corrupted() {
    let (config, request, _temp_dir) = mock_env_for_file("corrupted.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(!backend_result.object_metadata["has_comment"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_encrypted_headers"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["has_recovery_record"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["is_locked"]
        .as_bool()
        .expect("a bool"));
    assert!(!backend_result.object_metadata["is_solid"]
        .as_bool()
        .expect("a bool"));

    assert!(backend_result
        .symbols
        .contains(&"HAS_CHECKSUM_INCONSISTENCY".into()));

    assert_eq!(backend_result.children.len(), 2);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));

    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| v.symbols.contains(&"CHECKSUM_MISMATCH".into()))
            .count(),
        1
    );
}

#[test]
fn limit_decompressed_entry_size() {
    for (size_limit, number_ok, number_toobig) in [(63, 0, 2), (64, 1, 1), (95, 1, 1), (96, 2, 0)] {
        let (mut config, request, _temp_dir) = mock_env_for_file("plain.rar"); // two files inside: 64
                                                                               // and 96 bytes long
                                                                               // decompressed
        config.max_child_output_size = size_limit;

        let BackendResultKind::ok(backend_result) =
            process_request(&request, &config).expect("BackendResultKind is expected")
        else {
            panic!("BackendResultKind::ok is expected")
        };

        assert_eq!(
            backend_result.symbols.contains(&"LIMITS_REACHED".into()),
            number_toobig > 0
        );

        assert_eq!(backend_result.children.len(), number_ok + number_toobig);
        assert_eq!(
            backend_result
                .children
                .iter()
                .filter(|v| v.path.is_none() && v.symbols.contains(&"TOOBIG".into()))
                .count(),
            number_toobig
        );
        assert_eq!(
            backend_result
                .children
                .iter()
                .filter(|v| v.path.is_some())
                .count(),
            number_ok
        );
    }
}

#[test]
fn limit_compressed_entry_size() {
    for (size_limit, number_ok, number_toobig) in [(63, 0, 2), (64, 1, 1), (91, 1, 1), (92, 2, 0)] {
        let (mut config, request, _temp_dir) = mock_env_for_file("plain.rar"); // two files inside: 64
                                                                               // and 92 bytes long
                                                                               // compressed
        config.max_child_input_size = size_limit;

        let BackendResultKind::ok(backend_result) =
            process_request(&request, &config).expect("BackendResultKind is expected")
        else {
            panic!("BackendResultKind::ok is expected")
        };

        assert_eq!(
            backend_result.symbols.contains(&"LIMITS_REACHED".into()),
            number_toobig > 0
        );

        assert_eq!(backend_result.children.len(), number_ok + number_toobig);
        assert_eq!(
            backend_result
                .children
                .iter()
                .filter(|v| v.path.is_none() && v.symbols.contains(&"TOOBIG".into()))
                .count(),
            number_toobig
        );
        assert_eq!(
            backend_result
                .children
                .iter()
                .filter(|v| v.path.is_some())
                .count(),
            number_ok
        );
    }
}

#[test]
fn limit_max_children() {
    let (mut config, request, _temp_dir) = mock_env_for_file("plain.rar");
    config.max_children = 1;
    let expected_children = config.max_children as usize;

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.symbols.contains(&"LIMITS_REACHED".into()));

    assert_eq!(backend_result.children.len(), expected_children);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
}

#[test]
fn limit_max_entries_to_process() {
    let (mut config, request, _temp_dir) = mock_env_for_file("plain.rar");
    config.max_entries_to_process = 3; // 1st and 2nd entries are files, the third one is a
                                       // directory
    let expected_entries = config.max_entries_to_process as usize;

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.symbols.contains(&"LIMITS_REACHED".into()));

    assert_eq!(
        backend_result.children.len()
            + backend_result.object_metadata["directories"]
                .as_array()
                .expect("an array is expected")
                .len(),
        expected_entries
    );
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
}

#[test]
fn limit_overall_compressed_size() {
    for (size_limit, number_extracted, limits_reached) in [
        (50, 1, true),
        (64, 1, true),
        (65, 2, true),
        (156, 2, true),
        (157, 2, false),
    ] {
        let (mut config, request, _temp_dir) = mock_env_for_file("plain.rar");
        config.max_processed_size = size_limit; // two files inside: 64 and 92 bytes long
                                                         // compressed

        let BackendResultKind::ok(backend_result) =
            process_request(&request, &config).expect("BackendResultKind is expected")
        else {
            panic!("BackendResultKind::ok is expected")
        };

        assert_eq!(
            backend_result.symbols.contains(&"LIMITS_REACHED".into()),
            limits_reached
        );

        assert_eq!(backend_result.children.len(), number_extracted);
        assert!(backend_result.children.iter().all(|v| v.path.is_some()));
    }
}

#[test]
fn magic_value_of_unknown_uncompressed_size() {
    let (config, request, _temp_dir) = mock_env_for_file("archive.part1.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.children[0].relation_metadata["uncompressed_size"].is_null());
}

#[test]
fn uncompressed_size_over_32_bits() {
    let (config, request, _temp_dir) = mock_env_for_file("huge.rar");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.children[0].relation_metadata["uncompressed_size"]
            .as_u64()
            .expect("a big number"),
        17_179_869_183 // 2^34-1
    );
}

#[test]
fn limit_when_uncompressed_size_is_unknown() {
    let (mut config, request, _temp_dir) =
        mock_env_for_file("60Mb_uncompressed_size_unavailable.rar");
    config.max_child_output_size = 50_000_000;

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.symbols.contains(&"LIMITS_REACHED".into()),);
    assert!(backend_result.children[0]
        .symbols
        .contains(&"TOOBIG".to_string()));
}
