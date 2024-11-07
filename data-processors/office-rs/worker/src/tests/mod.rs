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
fn test_doc_1() {
    let env = TestEnvironment::setup();
    let request = create_request("not_encrypted.doc").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"OLE".to_string()));
    assert!(result.symbols.contains(&"DOC".to_string()));

    verify_encryption_symbols(&result, false, false);
    verify_doc_common_metadata(&result);
    verify_text_child(&result, "plain1");
}

#[test]
fn test_doc_2() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_contextal.doc").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"OLE".to_string()));
    assert!(result.symbols.contains(&"DOC".to_string()));
    verify_encryption_symbols(&result, true, true);

    let encryption = match result.object_metadata.get("encryption").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid encryption format"),
    };
    assert_eq!(
        encryption.get("algorithm"),
        Some(&Value::String("XOR obfuscation".to_string()))
    );
    assert_eq!(
        encryption.get("password"),
        Some(&Value::String("contextal".to_string()))
    );

    verify_doc_common_metadata(&result);
    verify_text_child(&result, "plain1");
}

#[test]
fn test_doc_3() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_otherpassword.doc").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    verify_encryption_symbols(&result, true, false);
    assert!(result.object_metadata.is_empty());
    assert_eq!(result.children.len(), 1);

    let child = result.children.first().unwrap();
    assert_eq!(child.path, None);
    assert_eq!(child.force_type, None);
    let algorithm = child.relation_metadata.get("algorithm").unwrap();
    assert_eq!(
        algorithm,
        &Value::String("Office binary document RC4 encryption".to_string())
    );
}

#[test]
fn test_docx_1() {
    let env = TestEnvironment::setup();
    let request = create_request("not_encrypted.docx").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"DOCX".to_string()));
    assert!(result.symbols.contains(&"LIMITS_REACHED".to_string()));
    verify_encryption_symbols(&result, false, false);

    verify_doc_common_metadata(&result);
    verify_text_child(&result, "plain1");
}

#[test]
fn test_docx_2() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_contextal.docx").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"DOCX".to_string()));
    assert!(result.symbols.contains(&"LIMITS_REACHED".to_string()));
    verify_encryption_symbols(&result, true, true);

    let encryption = match result.object_metadata.get("encryption").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid encryption format"),
    };
    assert_eq!(
        encryption.get("algorithm"),
        Some(&Value::String("AgileEncryption AES SHA512".to_string()))
    );
    assert_eq!(
        encryption.get("password"),
        Some(&Value::String("contextal".to_string()))
    );

    verify_doc_common_metadata(&result);
    verify_text_child(&result, "plain1");
}

#[test]
fn test_docx_2b() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_std.docx").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"DOCX".to_string()));
    verify_encryption_symbols(&result, true, true);

    let encryption = match result.object_metadata.get("encryption").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid encryption format"),
    };
    assert_eq!(
        encryption.get("algorithm"),
        Some(&Value::String("StandardEncryption AES-128".to_string()))
    );
    assert_eq!(
        encryption.get("password"),
        Some(&Value::String("contextal".to_string()))
    );

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
    assert_eq!(document, "encrypted");
}

#[test]
fn test_docx_3() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_otherpassword.docx").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    verify_encryption_symbols(&result, true, false);
    assert!(result.object_metadata.is_empty());
    assert_eq!(result.children.len(), 1);

    let child = result.children.first().unwrap();
    assert_eq!(child.path, None);
    assert_eq!(child.force_type, None);
    let algorithm = child.relation_metadata.get("algorithm").unwrap();
    assert_eq!(
        algorithm,
        &Value::String("AgileEncryption AES SHA512".to_string())
    );
}

#[test]
fn test_xls_1() {
    let env = TestEnvironment::setup();
    let request = create_request("not_encrypted.xls").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"OLE".to_string()));
    assert!(result.symbols.contains(&"XLS".to_string()));
    verify_encryption_symbols(&result, false, false);

    assert!(result.object_metadata.contains_key("sheets"));
    let sheets = match result.object_metadata.get("sheets").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid sheets format"),
    };
    assert_eq!(sheets.len(), 1);
    let sheet = match sheets.first().unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid sheet format"),
    };
    assert_eq!(sheet.get("limit_reached"), Some(&Value::Bool(false)));
    assert_eq!(
        sheet.get("name"),
        Some(&Value::String("Arkusz1".to_string()))
    );
    assert_eq!(
        sheet.get("type"),
        Some(&Value::String("worksheet".to_string()))
    );
    assert_eq!(
        sheet.get("visibility"),
        Some(&Value::String("visible".to_string()))
    );

    verify_xls_common_metadata(&result);
    verify_text_child(&result, "plain2");
}

#[test]
fn test_xls_2() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_contextal.xls").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"OLE".to_string()));
    assert!(result.symbols.contains(&"XLS".to_string()));
    verify_encryption_symbols(&result, true, true);

    assert!(result.object_metadata.contains_key("sheets"));
    let sheets = match result.object_metadata.get("sheets").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid sheets format"),
    };
    assert_eq!(sheets.len(), 1);
    let sheet = match sheets.first().unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid sheet format"),
    };
    assert_eq!(sheet.get("limit_reached"), Some(&Value::Bool(false)));
    assert_eq!(
        sheet.get("name"),
        Some(&Value::String("Arkusz1".to_string()))
    );
    assert_eq!(
        sheet.get("type"),
        Some(&Value::String("worksheet".to_string()))
    );
    assert_eq!(
        sheet.get("visibility"),
        Some(&Value::String("visible".to_string()))
    );

    let encryption = match result.object_metadata.get("encryption").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid encryption format"),
    };
    assert_eq!(
        encryption.get("algorithm"),
        Some(&Value::String("RC4".to_string()))
    );
    assert_eq!(
        encryption.get("password"),
        Some(&Value::String("contextal".to_string()))
    );

    verify_xls_common_metadata(&result);
    verify_text_child(&result, "plain2");
}

#[test]
fn test_xls_3() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_otherpassword.xls").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    verify_encryption_symbols(&result, true, false);
    assert!(result.object_metadata.is_empty());
    assert_eq!(result.children.len(), 1);

    let child = result.children.first().unwrap();
    assert_eq!(child.path, None);
    assert_eq!(child.force_type, None);
    let algorithm = child.relation_metadata.get("algorithm").unwrap();
    assert_eq!(
        algorithm,
        &Value::String("Office binary document RC4 CryptoApi encryption".to_string())
    );
}

#[test]
fn test_xlsx_1() {
    let env = TestEnvironment::setup();
    let request = create_request("not_encrypted.xlsx").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"XLSX".to_string()));
    assert!(result.symbols.contains(&"LIMITS_REACHED".to_string()));
    verify_encryption_symbols(&result, false, false);

    assert!(result.object_metadata.contains_key("sheets"));
    let sheets = match result.object_metadata.get("sheets").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid sheets format"),
    };
    assert_eq!(sheets.len(), 1);
    let sheet = match sheets.first().unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid sheet format"),
    };
    assert_eq!(sheet.get("limit_reached"), Some(&Value::Bool(false)));
    assert_eq!(
        sheet.get("name"),
        Some(&Value::String("Arkusz1".to_string()))
    );
    assert_eq!(
        sheet.get("type"),
        Some(&Value::String("worksheet".to_string()))
    );
    assert_eq!(
        sheet.get("visibility"),
        Some(&Value::String("visible".to_string()))
    );

    verify_xls_common_metadata(&result);
    verify_text_child(&result, "plain2");
}

#[test]
fn test_xlsx_2() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_contextal.xlsx").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"XLSX".to_string()));
    assert!(result.symbols.contains(&"LIMITS_REACHED".to_string()));
    verify_encryption_symbols(&result, true, true);

    assert!(result.object_metadata.contains_key("sheets"));
    let sheets = match result.object_metadata.get("sheets").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid sheets format"),
    };
    assert_eq!(sheets.len(), 1);
    let sheet = match sheets.first().unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid sheet format"),
    };
    assert_eq!(sheet.get("limit_reached"), Some(&Value::Bool(false)));
    assert_eq!(
        sheet.get("name"),
        Some(&Value::String("Arkusz1".to_string()))
    );
    assert_eq!(
        sheet.get("type"),
        Some(&Value::String("worksheet".to_string()))
    );
    assert_eq!(
        sheet.get("visibility"),
        Some(&Value::String("visible".to_string()))
    );

    let encryption = match result.object_metadata.get("encryption").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid encryption format"),
    };
    assert_eq!(
        encryption.get("algorithm"),
        Some(&Value::String("AgileEncryption AES SHA512".to_string()))
    );
    assert_eq!(
        encryption.get("password"),
        Some(&Value::String("contextal".to_string()))
    );

    verify_xls_common_metadata(&result);
    verify_text_child(&result, "plain2");
}

#[test]
fn test_xlsx_2b() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_std.xlsx").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"XLSX".to_string()));
    verify_encryption_symbols(&result, true, true);

    assert!(result.object_metadata.contains_key("sheets"));
    let sheets = match result.object_metadata.get("sheets").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid sheets format"),
    };
    assert_eq!(sheets.len(), 3);
    let sheet = match sheets.first().unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid sheet format"),
    };
    assert_eq!(sheet.get("limit_reached"), Some(&Value::Bool(false)));
    assert_eq!(
        sheet.get("name"),
        Some(&Value::String("Sheet1".to_string()))
    );
    assert_eq!(
        sheet.get("type"),
        Some(&Value::String("worksheet".to_string()))
    );
    assert_eq!(
        sheet.get("visibility"),
        Some(&Value::String("visible".to_string()))
    );

    let encryption = match result.object_metadata.get("encryption").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid encryption format"),
    };
    assert_eq!(
        encryption.get("algorithm"),
        Some(&Value::String("StandardEncryption AES-128".to_string()))
    );
    assert_eq!(
        encryption.get("password"),
        Some(&Value::String("contextal".to_string()))
    );

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
    assert_eq!(document, "test,cell\n");
}

#[test]
fn test_xlsx_3() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_otherpassword.xlsx").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    verify_encryption_symbols(&result, true, false);
    assert!(result.object_metadata.is_empty());
    assert_eq!(result.children.len(), 1);

    let child = result.children.first().unwrap();
    assert_eq!(child.path, None);
    assert_eq!(child.force_type, None);
    let algorithm = child.relation_metadata.get("algorithm").unwrap();
    assert_eq!(
        algorithm,
        &Value::String("AgileEncryption AES SHA512".to_string())
    );
}

#[test]
fn test_xlsb_1() {
    let env = TestEnvironment::setup();
    let request = create_request("not_encrypted.xlsb").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"XLSB".to_string()));
    assert!(result.symbols.contains(&"LIMITS_REACHED".to_string()));
    verify_encryption_symbols(&result, false, false);

    assert!(result.object_metadata.contains_key("sheets"));
    let sheets = match result.object_metadata.get("sheets").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid sheets format"),
    };
    assert_eq!(sheets.len(), 1);
    let sheet = match sheets.first().unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid sheet format"),
    };
    assert_eq!(sheet.get("limit_reached"), Some(&Value::Bool(false)));
    assert_eq!(
        sheet.get("name"),
        Some(&Value::String("Arkusz1".to_string()))
    );
    assert_eq!(
        sheet.get("type"),
        Some(&Value::String("worksheet".to_string()))
    );
    assert_eq!(
        sheet.get("visibility"),
        Some(&Value::String("visible".to_string()))
    );

    verify_xls_common_metadata(&result);
    verify_text_child(&result, "plain2");
}

#[test]
fn test_xlsb_2() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_contextal.xlsb").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    assert!(result.symbols.contains(&"XLSB".to_string()));
    assert!(result.symbols.contains(&"LIMITS_REACHED".to_string()));
    verify_encryption_symbols(&result, true, true);

    assert!(result.object_metadata.contains_key("sheets"));
    let sheets = match result.object_metadata.get("sheets").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid sheets format"),
    };
    assert_eq!(sheets.len(), 1);
    let sheet = match sheets.first().unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid sheet format"),
    };
    assert_eq!(sheet.get("limit_reached"), Some(&Value::Bool(false)));
    assert_eq!(
        sheet.get("name"),
        Some(&Value::String("Arkusz1".to_string()))
    );
    assert_eq!(
        sheet.get("type"),
        Some(&Value::String("worksheet".to_string()))
    );
    assert_eq!(
        sheet.get("visibility"),
        Some(&Value::String("visible".to_string()))
    );

    let encryption = match result.object_metadata.get("encryption").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid encryption format"),
    };
    assert_eq!(
        encryption.get("algorithm"),
        Some(&Value::String("AgileEncryption AES SHA512".to_string()))
    );
    assert_eq!(
        encryption.get("password"),
        Some(&Value::String("contextal".to_string()))
    );

    verify_xls_common_metadata(&result);
    verify_text_child(&result, "plain2");
}

#[test]
fn test_xlsb_3() {
    let env = TestEnvironment::setup();
    let request = create_request("encrypted_otherpassword.xlsb").unwrap();
    let result = match process_request(&request, &env.config).unwrap() {
        backend_utils::objects::BackendResultKind::ok(result) => result,
        backend_utils::objects::BackendResultKind::error(error) => panic!("{error}"),
    };

    verify_encryption_symbols(&result, true, false);
    assert!(result.object_metadata.is_empty());
    assert_eq!(result.children.len(), 1);

    let child = result.children.first().unwrap();
    assert_eq!(child.path, None);
    assert_eq!(child.force_type, None);
    let algorithm = child.relation_metadata.get("algorithm").unwrap();
    assert_eq!(
        algorithm,
        &Value::String("AgileEncryption AES SHA512".to_string())
    );
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
    let result: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "test_data"]
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
            sheet_size_limit: 50000,
            shared_strings_cache_limit: 50000,
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
        object_type: "Office".to_string(),
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

fn verify_text_child(result: &BackendResultOk, control_sample_name: &str) {
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
    let control_sample =
        load_string_from_file(get_sample_path(control_sample_name).to_str().unwrap());
    let score = normalized_damerau_levenshtein(&document, &control_sample);
    assert!(score > 0.9);
}

fn verify_doc_common_metadata(result: &BackendResultOk) {
    assert!(result.object_metadata.contains_key("encryption"));
    assert!(result.object_metadata.contains_key("hyperlinks"));
    let hyperlinks = match result.object_metadata.get("hyperlinks").unwrap() {
        Value::Array(array) => array,
        _ => panic!("Invalid hyperlinks format"),
    };
    assert_eq!(hyperlinks.len(), 1);
    assert_eq!(
        hyperlinks.first().unwrap(),
        &Value::String("https://example.com".to_string())
    );
    assert!(result.object_metadata.contains_key("properties"));
    let properties = match result.object_metadata.get("properties").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid properties format"),
    };
    assert_eq!(
        properties.get("application"),
        Some(&Value::String("Microsoft Office Word".to_string()))
    );
    assert_eq!(
        properties.get("creator"),
        Some(&Value::String("test".to_string()))
    );
    assert_eq!(
        properties.get("title"),
        Some(&Value::String("Word sample file".to_string()))
    );
    assert!(result
        .object_metadata
        .contains_key(&"user_properties".to_string()));
    assert!(result.object_metadata.contains_key(&"vba".to_string()));
}

fn verify_xls_common_metadata(result: &BackendResultOk) {
    assert!(result.object_metadata.contains_key("encryption"));
    assert!(result.object_metadata.contains_key("properties"));
    let properties = match result.object_metadata.get("properties").unwrap() {
        Value::Object(object) => object,
        _ => panic!("Invalid properties format"),
    };
    assert_eq!(
        properties.get("application"),
        Some(&Value::String("Microsoft Excel".to_string()))
    );
    assert_eq!(
        properties.get("creator"),
        Some(&Value::String("test".to_string()))
    );
    assert!(result
        .object_metadata
        .contains_key(&"user_properties".to_string()));
    assert!(result.object_metadata.contains_key(&"vba".to_string()));
}

fn verify_encryption_symbols(result: &BackendResultOk, encrypted: bool, decrypted: bool) {
    assert_eq!(result.symbols.contains(&"ENCRYPTED".to_string()), encrypted);
    assert_eq!(result.symbols.contains(&"DECRYPTED".to_string()), decrypted);
    for child in &result.children {
        assert_eq!(child.symbols.contains(&"ENCRYPTED".to_string()), encrypted);
        assert_eq!(child.symbols.contains(&"DECRYPTED".to_string()), decrypted);
    }
}
