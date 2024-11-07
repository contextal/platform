use crate::process_request;
use backend_utils::objects::{BackendRequest, BackendResultKind, Info, Metadata};
use pdf_rs::config::{Config, OcrMode};
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
            org: "contextal".to_string(),
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
fn smoke_fonts() {
    let (config, request, _temp_dir) = mock_env_for_file("sample.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["fonts"]
            .as_array()
            .expect("an array")
            .iter()
            .map(|v| v.as_str().expect("a string"))
            .collect::<Vec<_>>(),
        vec![
            "ARVFKS+TimesNewRomanPSMT",
            "FTKOCI+Calibri",
            "PQNWMO+Calibri-Italic",
            "PTHMMO+Calibri-LightItalic",
            "PTHMMO+TimesNewRomanPS-ItalicMT",
            "UQWVEE+Calibri-Light",
            "USCAEE+ArialMT",
        ],
        "An unexpected set of fonts"
    );
}

#[test]
fn smoke_builtin_metadata() {
    let (config, request, _temp_dir) = mock_env_for_file("sample.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["builtin_metadata"]["author"]
            .as_str()
            .expect("a string"),
        "User",
        "An unexpected document author"
    );
    assert_eq!(
        backend_result.object_metadata["builtin_metadata"]["creator"]
            .as_str()
            .expect("a string"),
        "Acrobat PDFMaker 23 for Word",
        "An unexpected document creator"
    );
    assert_eq!(
        backend_result.object_metadata["builtin_metadata"]["producer"]
            .as_str()
            .expect("a string"),
        "Adobe PDF Library 23.6.96",
        "An unexpected document producer"
    );
    assert_eq!(
        backend_result.object_metadata["builtin_metadata"]["creation_date"]["raw"]
            .as_str()
            .expect("a string"),
        "D:20231123204446+02'00'",
        "An unexpected raw creation date"
    );
    assert_eq!(
        backend_result.object_metadata["builtin_metadata"]["creation_date"]["parsed"]
            .as_array()
            .expect("an array")
            .iter()
            .map(|v| v.as_i64().expect("number in an array"))
            .collect::<Vec<_>>(),
        [2023, 327, 20, 44, 46, 0, 2, 0, 0],
        "An unexpected parsed creation date"
    );
}

#[test]
fn smoke_annotations() {
    let (config, request, _temp_dir) = mock_env_for_file("sample-with-annotations.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["number_of_annotations"]["total"]
            .as_u64()
            .expect("an u64 integer"),
        12,
        "An unexpected total number of annotations"
    );
    assert_eq!(
        backend_result.object_metadata["number_of_annotations"]["link"]
            .as_u64()
            .expect("an u64 integer"),
        2,
        "An unexpected number of annotations of Link type"
    );
    assert_eq!(
        backend_result.object_metadata["number_of_annotations"]["other"]
            .as_u64()
            .expect("an u64 integer"),
        3,
        "An unexpected number of annotations of other types"
    );
    assert_eq!(
        backend_result.object_metadata["number_of_annotations"]["text"]
            .as_u64()
            .expect("an u64 integer"),
        2,
        "An unexpected number of annotations of Text type"
    );
    assert_eq!(
        backend_result.object_metadata["number_of_annotations"]["popup"]
            .as_u64()
            .expect("an u64 integer"),
        5,
        "An unexpected number of annotations of Pop-up type"
    );
}

#[test]
fn smoke_bookmarks() {
    let (config, request, _temp_dir) = mock_env_for_file("sample.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["number_of_bookmarks"]["total"]
            .as_u64()
            .expect("an u64 integer"),
        8,
        "An unexpected number of bookmarks"
    );
}

#[test]
fn smoke_links() {
    let (config, request, _temp_dir) = mock_env_for_file("sample.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["number_of_links"]["total"]
            .as_u64()
            .expect("an u64 integer"),
        2,
        "An unexpected number of links"
    );
    assert_eq!(
        backend_result.object_metadata["number_of_links"]["with_action_uri"]
            .as_u64()
            .expect("an u64 integer"),
        2,
        "An unexpected number of links with action of URI type"
    );
}

#[test]
fn smoke_objects() {
    let (config, request, _temp_dir) = mock_env_for_file("sample.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["number_of_objects"]["total"]
            .as_u64()
            .expect("an u64 integer"),
        80,
        "An unexpected number of objects"
    );
    assert_eq!(
        backend_result.object_metadata["number_of_objects"]["images"]
            .as_u64()
            .expect("an u64 integer"),
        2,
        "An unexpected number of objects of Image type"
    );
    assert_eq!(
        backend_result.object_metadata["number_of_objects"]["texts"]
            .as_u64()
            .expect("an u64 integer"),
        76,
        "An unexpected number of objects of Text type"
    );
    assert_eq!(
        backend_result.object_metadata["number_of_objects"]["vector_paths"]
            .as_u64()
            .expect("an u64 integer"),
        2,
        "An unexpected number of objects of Vector Path type"
    );
}

#[test]
fn smoke_paper_sizes() {
    let (config, request, _temp_dir) = mock_env_for_file("sample.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["paper_sizes_mm"]
            .as_array()
            .expect("an array")
            .len(),
        1,
        "An unexpected number of distinct page sizes"
    );
    assert_eq!(
        backend_result.object_metadata["paper_sizes_mm"]
            .as_array()
            .expect("an array")[0]["height"]
            .as_u64()
            .expect("an u64 integer"),
        297,
        "An unexpected page height"
    );
    assert_eq!(
        backend_result.object_metadata["paper_sizes_mm"]
            .as_array()
            .expect("an array")[0]["width"]
            .as_u64()
            .expect("an u64 integer"),
        210,
        "An unexpected page width"
    );
    assert_eq!(
        backend_result.object_metadata["paper_sizes_mm"]
            .as_array()
            .expect("an array")[0]["standard_name"]
            .as_str()
            .expect("a string"),
        "A4",
        "An unexpected page size standard name"
    );
}

#[test]
fn smoke_children_count() {
    let (mut config, request, _temp_dir) = mock_env_for_file("sample.pdf");
    config.ocr_mode = OcrMode::Always;
    config.save_image_objects = true;

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.children.len(),
        4,
        "An unexpected number of children objects"
    );

    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| { v.relation_metadata.get("Image").is_some() })
            .count(),
        2,
        "An unexpected number of children objects of Image type"
    );

    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| { v.relation_metadata.get("DocumentText").is_some() })
            .count(),
        2,
        "An unexpected number of children objects of DocumentText type"
    );
}

#[test]
fn smoke_limits() {
    let (mut config, request, _temp_dir) = mock_env_for_file("sample.pdf");
    config.ocr_mode = OcrMode::Always;
    config.save_image_objects = true;
    config.max_children = 2;

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };
    assert_eq!(
        backend_result.children.len(),
        2,
        "An unexpected number of children objects"
    );
    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| { v.path.is_none() })
            .count(),
        0,
        "An unexpected number of failed children"
    );
    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| { v.relation_metadata.get("Image").is_some() })
            .count(),
        2,
        "An unexpected number of children objects of Image type"
    );

    config.max_child_output_size = 500000;
    config.max_children = 10;
    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };
    assert_eq!(
        backend_result.children.len(),
        4,
        "An unexpected number of children objects"
    );
    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| { v.path.is_none() })
            .count(),
        1,
        "An unexpected number of failed children"
    );
    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| { v.relation_metadata.get("Image").is_some() })
            .count(),
        2,
        "An unexpected number of children objects of Image type"
    );

    config.max_processed_size = 600000;
    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };
    assert_eq!(
        backend_result.children.len(),
        2,
        "An unexpected number of children objects"
    );
    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| { v.path.is_none() })
            .count(),
        1,
        "An unexpected number of failed children"
    );
    assert_eq!(
        backend_result
            .children
            .iter()
            .filter(|v| { v.relation_metadata.get("Image").is_some() })
            .count(),
        2,
        "An unexpected number of children objects of Image type"
    );
}

#[test]
fn smoke_version_and_number_of_pages() {
    let (mut config, request, _temp_dir) = mock_env_for_file("sample.pdf");
    config.ocr_mode = OcrMode::Always;

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["number_of_pages"]
            .as_u64()
            .expect("an u64 integer"),
        2,
        "An unexpected number of pages"
    );

    assert_eq!(
        backend_result.object_metadata["version"]
            .as_str()
            .expect("a string"),
        "1.6",
        "An unexpected PDF document version number"
    );
}

#[test]
fn smoke_correct_password() {
    let (mut config, mut request, _temp_dir) = mock_env_for_file("sample-with-view-password.pdf");
    config.save_image_objects = true;

    let possible_passwords = ["wrong", "password"].map(|s| s.to_string()).to_vec();
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

    assert!(backend_result.symbols.contains(&"ENCRYPTED".to_string()));
    assert!(backend_result.symbols.contains(&"DECRYPTED".to_string()));

    assert_eq!(
        backend_result.object_metadata["version"]
            .as_str()
            .expect("a string"),
        "1.7",
        "An unexpected PDF document version number"
    );

    assert_eq!(
        backend_result.children.len(),
        5,
        "An unexpected number of children objects"
    );
}

#[test]
fn smoke_wrong_password() {
    let (config, request, _temp_dir) = mock_env_for_file("sample-with-view-password.pdf");

    let response = process_request(&request, &config);

    let Ok(BackendResultKind::ok(result)) = response else {
        panic!("Unexpected result in case of no password for password-protected PDF");
    };
    assert!(result.symbols.contains(&"ENCRYPTED".to_string()));
    assert!(!result.symbols.contains(&"DECRYPTED".to_string()));
    assert_eq!(result.children.len(), 1);
    let child = result.children.first().unwrap();
    assert!(child.path.is_none());
    assert!(child.symbols.contains(&"ENCRYPTED".to_string()));
    assert!(!child.symbols.contains(&"DECRYPTED".to_string()));

    // Another try, now with a wrong password.
    let (config, mut request, _temp_dir) = mock_env_for_file("sample-with-view-password.pdf");
    let possible_passwords = ["wrong"].map(|s| s.to_string()).to_vec();
    let mut global = serde_json::Map::<String, serde_json::Value>::new();
    global.insert("possible_passwords".into(), possible_passwords.into());
    request
        .relation_metadata
        .insert("_global".into(), global.into());

    let response = process_request(&request, &config);

    let Ok(BackendResultKind::ok(result)) = response else {
        panic!("Unexpected result in case of no password for password-protected PDF");
    };
    assert!(result.symbols.contains(&"ENCRYPTED".to_string()));
    assert!(!result.symbols.contains(&"DECRYPTED".to_string()));
    assert_eq!(result.children.len(), 1);
    let child = result.children.first().unwrap();
    assert!(child.path.is_none());
    assert!(child.symbols.contains(&"ENCRYPTED".to_string()));
    assert!(!child.symbols.contains(&"DECRYPTED".to_string()));
}

#[test]
fn smoke_symbols_from_limits() {
    let (mut config, request, _temp_dir) = mock_env_for_file("sample.pdf");
    config.max_pages = 1;
    config.max_objects = 1;
    config.max_bookmarks = 1;
    config.max_fonts_per_page = 1;

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    let mut expected_symbols = vec![
        "LIMITS_REACHED",
        "MAX_BOOKMARKS_REACHED",
        "MAX_FONTS_PER_PAGE_REACHED",
        "MAX_OBJECTS_REACHED",
        "MAX_PAGES_REACHED",
    ];
    expected_symbols.sort();
    let mut actual_symbols = backend_result.symbols;
    actual_symbols.sort();
    assert_eq!(
        expected_symbols, actual_symbols,
        "Lists of expected and actual symbols don't match"
    );

    // Another try, now with no limit on number of pages to test/trigger limits on objects from the
    // second page.
    let (mut config, request, _temp_dir) = mock_env_for_file("sample.pdf");
    config.max_links = 1;
    config.max_annotations = 1;

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    let mut expected_symbols = vec![
        "LIMITS_REACHED",
        "MAX_ANNOTATIONS_REACHED",
        "MAX_LINKS_REACHED",
    ];
    expected_symbols.sort();
    let mut actual_symbols = backend_result.symbols;
    actual_symbols.sort();
    assert_eq!(
        expected_symbols, actual_symbols,
        "Lists of expected and actual symbols don't match"
    );
}

#[test]
fn pre_header_and_post_trailer_symbols() {
    let (config, request, _temp_dir) = mock_env_for_file("sample.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert!(backend_result.symbols.is_empty());

    // Now with extra data before PDF header and after PDF trailer:
    let (config, request, _temp_dir) =
        mock_env_for_file("sample-with-pre-header-and-post-trailer.pdf");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    let mut expected_symbols = vec!["ISSUES"];
    expected_symbols.sort();
    let mut actual_symbols = backend_result.symbols;
    actual_symbols.sort();
    assert_eq!(
        expected_symbols, actual_symbols,
        "Lists of expected and actual symbols don't match"
    );
}
