use crate::{BackendState, process_request};
use backend_utils::objects::{BackendRequest, BackendResultKind, Info, Metadata};
use std::collections::HashMap;
use tempfile::TempDir;
use text_rs::config::Config;

fn mock_env_for_file(file: &str) -> (BackendState, BackendRequest, TempDir) {
    if let Err(e) = tensorflow::library::load() {
        panic!("Failed to load tensorflow: {}", e);
    }

    let temp_dir = TempDir::new().unwrap();

    let config = Config {
        objects_path: "tests/test_data".into(),
        output_path: temp_dir.path().to_string_lossy().into(),
        ..Config::new().expect("a valid config")
    };
    let backend_state = BackendState::new(config).expect("failed to construct BackendState");

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

    (backend_state, request, temp_dir)
}

#[test]
fn example_toml() {
    let (backend_state, request, _temp_dir) = mock_env_for_file("example.toml");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &backend_state).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["programming_language"]
            .as_str()
            .expect("a string"),
        "TOML",
        "Unexpected identified programming language"
    );

    assert!(
        backend_result.object_metadata["uris"]
            .as_array()
            .expect("an array")
            .is_empty()
    );

    assert!(backend_result.symbols.contains(&"MANY_NUMBERS".to_string()));
    assert!(backend_result.symbols.contains(&"CC_NUMBER".to_string()));
}

#[test]
fn broken_toml() {
    let (backend_state, request, _temp_dir) = mock_env_for_file("broken.toml");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &backend_state).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.object_metadata["programming_language"].is_null());

    assert!(
        backend_result.object_metadata["uris"]
            .as_array()
            .expect("an array")
            .is_empty()
    );

    assert!(!backend_result.symbols.contains(&"CC_NUMBER".to_string()));
}

#[test]
fn passwords() {
    let (backend_state, request, _temp_dir) = mock_env_for_file("passwords.txt");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &backend_state).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };
    assert_eq!(
        backend_result.object_metadata["natural_language"]
            .as_str()
            .expect("a string"),
        "English",
        "Unexpected natural language"
    );

    assert!(backend_result.object_metadata["programming_language"].is_null());

    assert!(
        backend_result.object_metadata["uris"]
            .as_array()
            .expect("an array")
            .is_empty()
    );

    let mut expected = vec![
        "password1",
        "'password2'",
        "password3.",
        r#""password4""#,
        "password5,",
        "!@#$%^&*()_+|",
    ];
    expected.sort();

    assert_eq!(
        backend_result.object_metadata["possible_passwords"]
            .as_array()
            .expect("an array"),
        &expected
    );
}
