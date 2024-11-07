//! Object definitions
use crate::utils::{get_queue_for, random_string};
use digest::{Digest, DynDigest};
use md5::Md5;
use serde::{ser::SerializeMap, ser::SerializeSeq, Deserialize, Serialize, Serializer};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use std::collections::HashMap;
use std::fmt::Write;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// The type of the object_metadata and relation_metadata
pub type Metadata = serde_json::Map<String, serde_json::Value>;

/// Sanitize metadata keys
pub fn sanitize_meta_keys(meta: &mut Metadata) {
    let bad_keys = meta
        .keys()
        .filter_map(|k| {
            if k.contains(' ') {
                Some(k.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<String>>();
    for key in bad_keys {
        let value = meta.remove(&key).unwrap();
        let sanitized_key = key.replace(
            |c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "_",
        );
        if meta.contains_key(&sanitized_key) {
            warn!(
                "Cannot sanitize metadata key \"{}\" to \"{}\" because duplicate exists",
                key, sanitized_key
            );
        } else {
            warn!(
                "Sanitized metadata key \"{}\" to \"{}\"",
                key, sanitized_key
            );
            meta.insert(sanitized_key, value);
        }
    }
    for v in meta.values_mut() {
        if let serde_json::Value::Object(sub_object) = v {
            sanitize_meta_keys(sub_object)
        }
    }
}

/// The full object descriptor (as received in job requests)
#[derive(Deserialize, Debug)]
pub struct Descriptor {
    pub info: Info,
    pub symbols: Vec<String>,
    pub relation_metadata: Metadata,
    pub max_recursion: u32,
}

/// Same as [`Descriptor`] but using references (used in publishing job requests)
#[derive(Serialize, Debug)]
pub struct DescriptorRef<'a> {
    pub info: &'a Info,
    pub symbols: &'a Vec<String>,
    #[serde(serialize_with = "serialize_meta")]
    pub relation_metadata: &'a Metadata,
    pub max_recursion: u32,
}

impl<'a> From<&'a Descriptor> for DescriptorRef<'a> {
    fn from(d: &'a Descriptor) -> Self {
        Self {
            info: &d.info,
            symbols: &d.symbols,
            relation_metadata: &d.relation_metadata,
            max_recursion: d.max_recursion,
        }
    }
}

/// The hash type to use as the object id
pub const OBJECT_ID_HASH_TYPE: &str = "sha256";

/// The JSON representing the object to perform work upon
#[derive(Debug, Serialize, Deserialize)]
pub struct Info {
    /// The object origin
    pub org: String,
    /// The object ID
    pub object_id: String,
    /// The determined object type
    pub object_type: String,
    /// The determined object subtype
    pub object_subtype: Option<String>,
    /// The recursion level
    pub recursion_level: u32,
    /// The object size
    pub size: u64,
    /// Digests for of the object
    pub hashes: HashMap<String, String>,
    /// The creation time of the object
    pub ctime: f64,
}

impl Info {
    /// Turns a (temp)file into an untyped Info
    pub async fn new_from_file(
        org: &str,
        src_file: &str,
        objects_path: &str,
        recursion_level: u32,
        ctime: f64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Open the input file
        let inf = tokio::fs::File::open(src_file).await.map_err(|e| {
            error!("Failed to open \"{}\" for hashing: {}", src_file, e);
            e
        })?;
        let (dst_file, outf) = mktemp(objects_path).await?;
        let res = Self::hash_and_copy(src_file, inf, &dst_file, outf).await;
        if let Err(e) = res {
            tokio::fs::remove_file(&dst_file).await.ok();
            return Err(e);
        }
        let (size, hashes) = res.unwrap();
        let object_id = hashes[OBJECT_ID_HASH_TYPE].clone();
        let obj_fname = format!("{}/{}", objects_path, object_id);
        let move_res = tokio::fs::rename(&dst_file, &obj_fname).await;
        let object_type = if size == 0 {
            String::from("EMPTY")
        } else {
            String::new()
        };
        if let Err(e) = move_res {
            error!(
                "Failed to rename \"{}\" to \"{}\": {}",
                dst_file, obj_fname, e
            );
            tokio::fs::remove_file(&dst_file).await.ok();
            Err(e.into())
        } else {
            Ok(Self {
                org: org.to_string(),
                object_id,
                object_type,
                object_subtype: None,
                recursion_level,
                size,
                hashes,
                ctime,
            })
        }
    }

    async fn hash_and_copy(
        src_file: &str,
        mut inf: tokio::fs::File,
        dst_file: &str,
        mut outf: tokio::fs::File,
    ) -> Result<(u64, HashMap<String, String>), Box<dyn std::error::Error>> {
        let mut hasher = Hasher::new();
        let mut buf = [0u8; 4096];
        let mut size = 0u64;
        loop {
            match inf.read(&mut buf[..]).await {
                Ok(0) => break,
                Ok(len) => {
                    size += u64::try_from(len).unwrap();
                    hasher.update(&buf[0..len]);
                    outf.write_all(&buf[0..len]).await.map_err(|e| {
                        error!("Failed to write to temp object \"{}\": {}", dst_file, e);
                        e
                    })?;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    error!("Failed to read from \"{}\": {}", src_file, e);
                    return Err(e.into());
                }
            }
        }
        outf.flush().await.map_err(|e| {
            error!("Failed to flush temp object \"{}\": {}", dst_file, e);
            e
        })?;
        // FIXME: sync or don't?
        //outf.sync_all().await.map_err(|e| {
        //    error!("Failed to sync temp object \"{}\": {}", dst_file, e);
        //    e
        //})?;
        Ok((size, hasher.into_map()))
    }

    /// Returns a failed child
    pub fn new_failed(org: &str, recursion_level: u32, ctime: f64) -> Self {
        let hashes = HashMap::from([
            (
                "md5".to_string(),
                "00000000000000000000000000000000".to_string()
            ),
            (
                "sha1".to_string(),
                "0000000000000000000000000000000000000000".to_string()
            ),
            (
                "sha256".to_string(),
                "0000000000000000000000000000000000000000000000000000000000000000".to_string()
            ),
            (
                "sha512".to_string(),
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".to_string()
            )
        ]);
        let object_id = hashes[OBJECT_ID_HASH_TYPE].clone();
        Self {
            org: org.to_string(),
            object_id,
            object_type: "SKIPPED".to_string(),
            object_subtype: None,
            recursion_level,
            size: 0,
            hashes,
            ctime,
        }
    }

    /// Returns if the object is SKIPPED
    pub fn is_skipped(&self) -> bool {
        self.object_type == "SKIPPED"
    }

    /// Returns if the object is empty
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Convenience fn to retrieve the proper request queue for an object
    pub fn request_queue(&self) -> String {
        get_queue_for(&self.object_type)
    }

    /// Returns the work creation time of this object as [`SystemTime`](std::time::SystemTime)
    pub fn work_creation_time(&self) -> std::time::SystemTime {
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs_f64(self.ctime)
    }

    /// Sets the object type and subtype
    pub fn set_type(&mut self, object_type: &str) {
        let (main, sub) = match object_type.split_once('/') {
            Some((main, sub)) => (main, Some(sub.to_string())),
            None => (object_type, None),
        };
        self.object_type = main.to_string();
        self.object_subtype = sub;
    }
}

// Create a tempfile in the objects path
// Note: leaking tempfiles is possible if we get killed; a proper approach
// would use O_TMPFILE + re-linking via /proc but it has too strong requirements
// in terms of OS and FS
pub async fn mktemp(
    objects_path: &str,
) -> Result<(String, tokio::fs::File), Box<dyn std::error::Error>> {
    loop {
        let fname = format!("{}/{}.tmp", objects_path, random_string(32));
        let open_res = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&fname)
            .await;
        match open_res {
            Ok(res) => return Ok((fname, res)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                error!("Failed to create new temp object {}: {}", fname, e);
                return Err(e.into());
            }
        }
    }
}

pub struct Hasher(Vec<(&'static str, Box<dyn DynDigest>)>);

impl Hasher {
    pub fn new() -> Self {
        Self(vec![
            ("md5", Box::new(Md5::new())),
            ("sha1", Box::new(Sha1::new())),
            ("sha256", Box::new(Sha256::new())),
            ("sha512", Box::new(Sha512::new())),
        ])
    }

    pub fn update(&mut self, buf: &[u8]) {
        for (_, h) in self.0.iter_mut() {
            h.update(buf);
        }
    }

    pub fn into_map(self) -> HashMap<String, String> {
        self.0
            .into_iter()
            .map(|h| {
                (
                    h.0.to_string(),
                    h.1.finalize().iter().fold(String::new(), |mut acc, v| {
                        write!(acc, "{:02x}", v).unwrap();
                        acc
                    }),
                )
            })
            .collect()
    }
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

struct MapWrapper<'a>(&'a Metadata);

impl<'a> Serialize for MapWrapper<'a> {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let meta = self.0;
        let mut map = s.serialize_map(Some(meta.len()))?;
        for (k, v) in meta {
            match v {
                serde_json::Value::Null => {}
                serde_json::Value::Object(inmap) => {
                    let wrapper = MapWrapper(inmap);
                    map.serialize_entry(k, &wrapper)?;
                }
                serde_json::Value::Array(values) => {
                    let wrapper = ArrayWrapper(values);
                    map.serialize_entry(k, &wrapper)?;
                }
                _ => map.serialize_entry(k, v)?,
            }
        }
        map.end()
    }
}

struct ArrayWrapper<'a>(&'a Vec<serde_json::Value>);

impl<'a> Serialize for ArrayWrapper<'a> {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for v in self.0 {
            match v {
                serde_json::Value::Object(inmap) => {
                    let wrapper = MapWrapper(inmap);
                    seq.serialize_element(&wrapper)?;
                }
                serde_json::Value::Array(values) => {
                    let wrapper = ArrayWrapper(values);
                    seq.serialize_element(&wrapper)?;
                }
                _ => seq.serialize_element(v)?,
            }
        }
        seq.end()
    }
}

/// A metadata serializer that omits nulls
pub fn serialize_meta<S: Serializer>(meta: &Metadata, s: S) -> Result<S::Ok, S::Error> {
    let wrapper = MapWrapper(meta);
    wrapper.serialize(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct MetaHolder {
        #[serde(serialize_with = "serialize_meta")]
        meta: Metadata,
    }

    #[test]
    fn serlialize_metadata() -> Result<(), Box<dyn std::error::Error>> {
        let data = r#"
        {
            "key": "value",
            "none": null,
            "bool": true,
            "number": 42,
            "array": [ 1, "two", null, false, { "arkey": "arvalue", "arnone": null} ],
            "emptyar": [ ],
            "map": {
                "subkey": "subval",
                "subbool": false,
                "subnone": null,
                "subarr": [ -1, "minus two", null, true ],
                "subemtpymap": {}
            },
            "emptymap": {}
        }"#;
        let mh = MetaHolder {
            meta: serde_json::from_str(data)?,
        };
        let ser_mh = serde_json::to_string_pretty(&mh)?;
        println!("Data: {}", ser_mh);
        let de_mh: MetaHolder = serde_json::from_str(&ser_mh)?;

        let ref_data = r#"
        {
            "key": "value",
            "bool": true,
            "number": 42,
            "array": [ 1, "two", null, false, { "arkey": "arvalue"} ],
            "emptyar": [ ],
            "map": {
                "subkey": "subval",
                "subbool": false,
                "subarr": [ -1, "minus two", null, true ],
                "subemtpymap": {}
            },
            "emptymap": {}
        }"#;
        let ref_mh = MetaHolder {
            meta: serde_json::from_str(ref_data)?,
        };
        println!("Reference: {}", serde_json::to_string_pretty(&ref_mh)?);
        assert_eq!(de_mh, ref_mh);

        Ok(())
    }
}
