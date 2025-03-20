use crate::{config, process_request};
use backend_utils::objects::*;
use std::collections::HashMap;
use tempfile::TempDir;

fn mock_env_for_file(file: &str) -> (config::Config, BackendRequest, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = config::Config {
        objects_path: "tests/test_data".into(),
        output_path: temp_dir.path().to_string_lossy().into(),
        ..config::Config::new().unwrap()
    };
    let request = BackendRequest {
        object: Info {
            org: "test".to_string(),
            object_id: file.into(),
            object_type: "test".into(),
            object_subtype: None,
            recursion_level: 1,
            size: 1254426,
            hashes: HashMap::new(),
            ctime: 1695645418.7196224,
        },
        symbols: Vec::new(),
        relation_metadata: Metadata::new(),
    };

    (config, request, temp_dir)
}

#[test]
fn longer_uncompressed_stream_size() {
    let (config, request, _temp_dir) = mock_env_for_file("stream_longer_than_claimed.zip");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::Ok is expected")
    };

    assert!(
        backend_result.children[0]
            .symbols
            .iter()
            .any(|sym| *sym == "ZIP_DSIZE_MISMATCH")
    );
}

#[test]
fn shorter_uncompressed_stream_size() {
    let (config, request, _temp_dir) = mock_env_for_file("stream_shorter_than_claimed.zip");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::Ok is expected")
    };

    assert!(
        backend_result.children[0]
            .symbols
            .iter()
            .any(|sym| *sym == "ZIP_DSIZE_MISMATCH")
    );
}

#[test]
fn valid_uncompressed_stream_sizes() {
    let (config, request, _temp_dir) = mock_env_for_file("alice.zip");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::Ok variant is expected")
    };

    assert!(
        backend_result
            .children
            .iter()
            .all(|child| child.symbols.iter().all(|sym| *sym != "ZIP_DSIZE_MISMATCH"))
    );
}

#[test]
fn much_longer_compressed_stream_size() {
    let (config, request, _temp_dir) = mock_env_for_file("compressed_size_longer_than_claimed.zip");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::Ok is expected")
    };

    assert!(
        backend_result.children[0]
            .symbols
            .iter()
            .any(|sym| *sym == "ZIP_CSIZE_MISMATCH")
    );
}

#[test]
fn encryption() {
    let (config, mut request, _temp_dir) = mock_env_for_file("encryption.zip");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::Ok is expected")
    };
    assert_eq!(backend_result.children.len(), 3);
    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|c| c.path.is_some())
            .count(),
        1
    );

    let possible_passwords = ["wrong", "password1", "password2"]
        .map(|s| s.to_string())
        .to_vec();
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
    assert_eq!(backend_result.children.len(), 3);
    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|c| c.path.is_some())
            .count(),
        3
    );
}
