use super::*;
use backend_utils::objects::Info;
use config::Config;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read},
    path::PathBuf,
};
use strsim::normalized_damerau_levenshtein;
use tempfile::TempDir;

#[test]
fn test_odt() {
    let env = TestEnvironment::setup();
    let request = create_request("not_encrypted.odt").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    println!("{result:#?}");

    assert!(result.symbols.contains(&"ODT".to_string()));

    assert!(result.object_metadata.contains_key("hyperlinks"));
    let hyperlinks = match result.object_metadata.get("hyperlinks").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid hyperlinks format"),
    };
    assert_eq!(hyperlinks.len(), 1);
    assert_eq!(
        hyperlinks.first().unwrap(),
        &Value::String("https://example.com/".to_string())
    );
    assert!(result.object_metadata.contains_key("properties"));
    let properties = match result.object_metadata.get("properties").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid properties format"),
    };
    assert_eq!(
        properties.get("generator"),
        Some(&Value::String(
            "LibreOffice/7.4.7.2$Linux_X86_64 LibreOffice_project/40$Build-2".to_string()
        ))
    );
    assert_eq!(
        properties.get("initial-creator"),
        Some(&Value::String("test".to_string()))
    );
    assert_eq!(
        properties.get("title"),
        Some(&Value::String("Word sample file".to_string()))
    );
    assert!(
        result
            .object_metadata
            .contains_key(&"user_properties".to_string())
    );

    let user_properties = match result.object_metadata.get("user_properties").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid user_properties format"),
    };

    assert_eq!(user_properties.len(), 3);

    for property in user_properties {
        let property = match property {
            Value::Object(object) => object,
            _ => panic!("Invalid user property format"),
        };
        assert_eq!(property.len(), 2);
        assert!(property.contains_key("key"));
        assert!(property.contains_key("value"));
    }

    let list = result
        .children
        .iter()
        .filter(|child| child.force_type == Some("Text".to_string()))
        .collect::<Vec<&BackendResultChild>>();
    assert_eq!(list.len(), 1);

    let document_path = list
        .first()
        .unwrap()
        .path
        .as_ref()
        .expect("Expecting child with path");

    let document = load_string_from_file(document_path);
    let control_sample = load_string_from_file(get_sample_path("plain").to_str().unwrap());
    let score = normalized_damerau_levenshtein(&document, &control_sample);
    assert!(score > 0.9);
}

fn load_string_from_file(path: &str) -> String {
    let mut file = File::open(path).unwrap();
    let mut result = String::new();
    file.read_to_string(&mut result).unwrap();
    result
}

fn get_sample_path(name: &str) -> PathBuf {
    let mut result = get_samples_dir();
    result.push(name);
    result
}

fn get_samples_dir() -> PathBuf {
    let result: PathBuf = [env!("CARGO_MANIFEST_DIR"), "src", "tests", "data"]
        .iter()
        .collect();
    fs::canonicalize(result).unwrap()
}

struct TestEnvironment {
    config: Config,
    _temp_dir: TempDir,
}

impl TestEnvironment {
    fn setup() -> Self {
        let temp_dir = TempDir::new().expect("Unable to create temporary dir");
        let objects_path = get_samples_dir()
            .to_str()
            .expect("Invalid objects_path")
            .to_string();
        let output_path = temp_dir
            .path()
            .to_str()
            .expect("Invalid output_path")
            .to_string();

        let config = Config {
            host: None,
            port: None,
            objects_path,
            output_path,
            max_children: 10,
            max_processed_size: 500000,
            max_child_output_size: 50000,
            create_domain_children: true,
        };
        TestEnvironment {
            _temp_dir: temp_dir,
            config,
        }
    }
}

fn create_request(filename: &str) -> Result<BackendRequest, io::Error> {
    let object = Info {
        org: "some org".to_string(),
        object_id: filename.to_string(),
        object_type: "ODF".to_string(),
        object_subtype: None,
        recursion_level: 0,
        size: 0,
        hashes: HashMap::<String, String>::new(),
        ctime: 0.0,
    };
    let reuest = BackendRequest {
        object,
        symbols: Vec::<String>::new(),
        relation_metadata: backend_utils::objects::Metadata::new(),
    };
    Ok(reuest)
}
