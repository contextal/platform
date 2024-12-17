use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

static TEMP_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

struct AsyncTempFile {
    path: std::path::PathBuf,
    f: tokio::fs::File,
}

impl AsyncTempFile {
    async fn new(suffix: &str) -> Result<Self, std::io::Error> {
        let (path, f) =
            shared::utils::mktemp(TEMP_DIR.get_or_init(std::env::temp_dir), Some(suffix)).await?;
        Ok(Self { path, f })
    }
}

impl Drop for AsyncTempFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path)
            .inspect_err(|e| {
                warn!(
                    "Failed to remove temporary db file {}: {}",
                    self.path.display(),
                    e
                )
            })
            .ok();
    }
}

struct ClamDatabaseFile(AsyncTempFile);

impl ClamDatabaseFile {
    async fn new() -> Result<Self, std::io::Error> {
        Ok(Self(AsyncTempFile::new(".ndb").await?))
    }
}

pub async fn find_invalid_patttern(query: &str) -> Result<(), super::ScenaryError> {
    let signatures = pgrules::parse_and_extract_clam_signatures(query).map_err(|e| {
        warn!("Failed to extract signatures: {e}");
        super::ScenaryError::Invalid("Invalid signatures in local rule")
    })?;
    if signatures.is_empty() {
        return Ok(());
    }
    let scanme = AsyncTempFile::new(".scanme")
        .await
        .map_err(|_| super::ScenaryError::Internal)?;
    for sig in signatures {
        let db_file = async {
            let mut db_file = ClamDatabaseFile::new().await?;
            db_file.0.f.write_all(sig.as_bytes()).await?;
            db_file.0.f.flush().await?;
            Ok(db_file)
        }
        .await
        .map_err(|e: std::io::Error| {
            warn!("Failed to create temporary database for testing: {}", e);
            super::ScenaryError::Internal
        })?;
        let mut clamscan = tokio::process::Command::new("clamscan")
            .arg("--quiet")
            .arg("-d")
            .arg(&db_file.0.path)
            .arg(&scanme.path)
            .spawn()
            .map_err(|e| {
                warn!("Failed to start clamscan: {}", e);
                super::ScenaryError::Internal
            })?;
        let res = clamscan.wait().await.map_err(|e| {
            warn!("Failed to run clamscan: {}", e);
            super::ScenaryError::Internal
        })?;
        if !res.success() {
            debug!("Signature check failed on: {}", sig);
            if let Some((_, patt)) = sig.rsplit_once(':') {
                return Err(super::ScenaryError::Signature(patt.to_string()));
            } else {
                return Err(super::ScenaryError::Signature(sig));
            }
        }
    }
    Ok(())
}
