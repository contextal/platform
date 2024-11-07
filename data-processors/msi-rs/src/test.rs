use crate::config::Config;
use crate::process_request;
use backend_utils::objects::{BackendRequest, BackendResultKind, Info, Metadata};
use std::collections::HashMap;
use tempfile::TempDir;

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
    let (config, request, _temp_dir) = mock_env_for_file("test.msi");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["author"]
            .as_str()
            .expect("a string is expected"),
        "Contextal"
    );

    assert_eq!(
        backend_result.object_metadata["arch"]
            .as_str()
            .expect("a string is expected"),
        "Intel"
    );

    assert_eq!(
        backend_result.object_metadata["subject"]
            .as_str()
            .expect("a string is expected"),
        "Contextal Test File"
    );

    assert_eq!(
        backend_result.object_metadata["uuid"]
            .as_str()
            .expect("a string is expected"),
        "2c5cdb9a-c515-4804-86ad-8d6458c3b155"
    );

    assert_eq!(backend_result.children.len(), 1);
    assert!(backend_result.children.iter().all(|v| v.path.is_some()));
}
