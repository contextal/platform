use crate::{config::Config, process_request};
use backend_utils::objects::{BackendRequest, BackendResultKind, Info, Metadata};
use ocr_rs::{Dpi, TessBaseApi, TessPageSegMode};
use std::{collections::HashMap, fs::File, io::BufReader};
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
            org: "ctx".into(),
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
fn string1_png() {
    let (config, request, _temp_dir) = mock_env_for_file("string1.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["text"]
            .as_str()
            .expect("string"),
        "LOREM IPSUM DOLOR SIT AMET"
    );
}

#[test]
fn all_word_confidences() {
    let image = image::ImageReader::new(BufReader::new(
        File::open("tests/test_data/string1.png").unwrap(),
    ))
    .with_guessed_format()
    .expect("failed to read an image signature")
    .decode()
    .expect("failed to decode an image")
    .into_rgba8();

    let api = TessBaseApi::new("eng", TessPageSegMode::PSM_AUTO, Some(Dpi(150)))
        .expect("failed to create Tesseract API instance");

    api.set_rgba_image(&image)
        .expect("failed to pass an image to Tesseract");

    api.recognize().expect("text recognition step has failed");

    let text = api.get_text().expect("failed to get recognized text");
    let confidences = api
        .get_all_word_confidences()
        .expect("failed to obtain word confidences");

    assert_eq!(
        text.split(' ').count(),
        confidences.len(),
        "mismatch between number of words and number of word confidences"
    );

    assert!(
        confidences.iter().all(|&v| v > 80),
        "word confidence is lower than anticipated: {confidences:?}"
    );
}

#[test]
fn string2_png() {
    let (config, request, _temp_dir) = mock_env_for_file("string2.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["text"]
            .as_str()
            .expect("string"),
        "Lorem ipsum dolor sit amet"
    );
}

#[test]
fn url1_png() {
    let (config, request, _temp_dir) = mock_env_for_file("url1.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    let ocr_url = backend_result.object_metadata["text"]
        .as_str()
        .expect("string");
    let actual_url = "https://contextal.com/some/weird/path?v1=42&hello=world";
    let allowed_number_of_mismatches = 1;

    assert!(
        ocr_url
            .chars()
            .zip(actual_url.chars())
            .filter(|(a, b)| a != b)
            .count()
            <= allowed_number_of_mismatches,
        "Too many mismatching symbols between \n{ocr_url}\n and \n{actual_url}",
    );
}

#[test]
fn url2_png() {
    let (config, request, _temp_dir) = mock_env_for_file("url2.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    let ocr_url = backend_result.object_metadata["text"]
        .as_str()
        .expect("string");
    let actual_url = "https://gate.sc/?url=http%3A%2F%2Fgithub.com%2Fgordol%2Fmalloc-ld_preload-sounds&token=a58998-1-1696604726816";
    let allowed_number_of_mismatches = 1; // Single mistake with version 4.1, no mistakes with
                                          // version 5.3
    assert!(
        ocr_url
            .chars()
            .zip(actual_url.chars())
            .filter(|(a, b)| a != b)
            .count()
            <= allowed_number_of_mismatches,
        "Too many mismatching symbols between \n{ocr_url}\n and \n{actual_url}",
    );
}

#[test]
fn page_region_png() {
    let (config, request, _temp_dir) = mock_env_for_file("page_region.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["text"]
            .as_str()
            .expect("string"),
        "designed to connect the dots\n\
        and deal with Al-powered cyberattacks"
    );
}

#[test]
#[ignore = "recognition accuracy is not even close yet in this example"]
fn password1_png() {
    let (config, request, _temp_dir) = mock_env_for_file("password1.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["text"]
            .as_str()
            .expect("string"),
        "Hi!\n\n\
        ZIP p a s s : lsvlMklrs123"
    );
}

#[test]
fn password2_png() {
    let (config, request, _temp_dir) = mock_env_for_file("password2.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    let ocr_pass = backend_result.object_metadata["text"]
        .as_str()
        .expect("string");
    let actual_pass = "Password: u5ANF2B";
    let allowed_number_of_mismatches = 2;

    assert!(
        ocr_pass
            .chars()
            .zip(actual_pass.chars())
            .filter(|(a, b)| a != b)
            .count()
            <= allowed_number_of_mismatches,
        "Too many mismatching symbols between \n{ocr_pass}\n and \n{actual_pass}",
    );
}

#[test]
fn password3_png() {
    let (config, request, _temp_dir) = mock_env_for_file("password3.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["text"]
            .as_str()
            .expect("string"),
        "Your password: 888"
    );
}

#[test]
fn password4_png() {
    let (config, request, _temp_dir) = mock_env_for_file("password4.png");

    let BackendResultKind::ok(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::ok is expected")
    };

    assert_eq!(
        backend_result.object_metadata["text"]
            .as_str()
            .expect("string"),
        "attachment password is 7845"
    );
}

#[test]
fn corrupted_gif() {
    let (config, request, _temp_dir) = mock_env_for_file("corrupted.gif");

    let BackendResultKind::error(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::error is expected")
    };

    assert!(backend_result.contains("invalid image file"));
}

#[test]
fn zero_area() {
    // Image crate identifies "zero_area.gif" as an image with zero dimensions (GIF's "Logical
    // Screen" specified in the file has 0 pixel width and 0 pixel height).
    // Most browsers and viewers still can display it.
    let (config, request, _temp_dir) = mock_env_for_file("zero_area.gif");

    let BackendResultKind::error(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::error is expected")
    };

    assert!(backend_result.contains("image has zero area"));
}

#[test]
fn unsupported_tiff() {
    let (config, request, _temp_dir) = mock_env_for_file("unsupported.tiff");

    let BackendResultKind::error(backend_result) =
        process_request(&request, &config).expect("BackendResultKind is expected")
    else {
        panic!("BackendResultKind::error is expected")
    };

    assert!(backend_result.contains("failed to decode an image"));
}
