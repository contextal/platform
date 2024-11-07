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
    let (config, request, _temp_dir) = mock_env_for_file("test.cab");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["number_of_files"]
            .as_u64()
            .expect("a number is expected"),
        1
    );

    assert_eq!(
        backend_result.object_metadata["number_of_folders"]
            .as_u64()
            .expect("a number is expected"),
        1
    );

    assert_eq!(backend_result.children.len(), 1);
    assert!(backend_result.children[0].path.is_some());

    assert_eq!(
        backend_result.children[0].relation_metadata["uncompressed_size"]
            .as_u64()                              
            .expect("a number"),
        36864
    );

    assert_eq!(
        backend_result.children[0].relation_metadata["compression_type"]
            .as_str()                              
            .expect("a string"),
        "MSZIP"
    );

    assert_eq!(
        backend_result.children[0].relation_metadata["name"]
            .as_str()                              
            .expect("a string"),
        "Test"
    );
}
