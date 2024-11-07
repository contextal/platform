use actix_multipart::form;
use actix_web::web;
use futures::stream::TryStreamExt;
use shared::object::{mktemp, Hasher, Info, OBJECT_ID_HASH_TYPE};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tracing::{debug, error};

/// Utility to turn temporary files into Objects
///
/// Temporary files as returned by the API enpoint need to be hashed and
/// moved to the proper storage
///
/// This struct provides the facilities to do this atomically
pub struct TempObject {
    f: tokio::fs::File,
    name: String,
    hashes: Hasher,
    objects_path: String,
    ctime: f64,
}

impl TempObject {
    pub async fn new(objects_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let ctime = std::time::SystemTime::UNIX_EPOCH
            .elapsed()
            .unwrap()
            .as_secs_f64();
        let (name, f) = mktemp(objects_path).await?;
        Ok(Self {
            f,
            name,
            hashes: Hasher::new(),
            objects_path: objects_path.to_string(),
            ctime,
        })
    }

    pub async fn into_object(mut self, org: &str) -> Result<Info, Box<dyn std::error::Error>> {
        self.f.flush().await.map_err(|e| {
            error!("Failed to flush temp file \"{}\": {}", self.name, e);
            e
        })?;
        let size = self
            .f
            .metadata()
            .await
            .map_err(|e| {
                error!("Failed to get size of temp file \"{}\": {}", self.name, e);
                e
            })?
            .len();
        let hashes = self.hashes.into_map();
        let object_id = hashes[OBJECT_ID_HASH_TYPE].clone();
        let obj_fname = format!("{}/{}", self.objects_path, object_id);
        let move_res = tokio::fs::rename(&self.name, &obj_fname).await;
        if let Err(e) = move_res {
            error!(
                "Failed to rename \"{}\" to \"{}\": {}",
                self.name, obj_fname, e
            );
            tokio::fs::remove_file(&self.name).await.ok();
            Err(e.into())
        } else {
            Ok(Info {
                org: org.to_string(),
                object_id,
                object_type: String::new(),
                object_subtype: None,
                recursion_level: 1,
                size,
                hashes,
                ctime: self.ctime,
            })
        }
    }

    pub async fn remove(self) {
        tokio::fs::remove_file(self.name).await.ok();
    }
}

impl AsyncWrite for TempObject {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let me = self.get_mut();
        let res = Pin::new(&mut me.f).poll_write(cx, buf);
        if let Poll::Ready(res) = &res {
            match res {
                Ok(sz) => me.hashes.update(&buf[0..*sz]),
                Err(e) => error!("Failed to write to temp file \"{}\": {}", me.name, e),
            }
        }
        res
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().f).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().f).poll_shutdown(cx)
    }
}

impl<'t> form::FieldReader<'t> for TempObject {
    type Future = Pin<Box<dyn Future<Output = Result<Self, actix_multipart::MultipartError>> + 't>>;

    fn read_field(
        req: &'t actix_web::HttpRequest,
        mut field: actix_multipart::Field,
        limits: &'t mut form::Limits,
    ) -> Self::Future {
        fn map_err<E: Into<actix_web::error::Error>>(
            name: &str,
            e: E,
        ) -> actix_multipart::MultipartError {
            actix_multipart::MultipartError::Field {
                field_name: name.to_string(),
                source: e.into(),
            }
        }

        async fn dump_field(
            tmpf: &mut TempObject,
            field: &mut actix_multipart::Field,
            limits: &mut form::Limits,
        ) -> Result<(), Box<dyn std::error::Error>> {
            while let Some(chunk) = field.try_next().await? {
                limits.try_consume_limits(chunk.len(), false)?;
                tmpf.write_all(chunk.as_ref()).await?;
            }
            Ok(())
        }

        Box::pin(async move {
            let objects_path = req.app_data::<web::Data<String>>().unwrap().as_ref();
            let mut tmpf = TempObject::new(objects_path.as_ref()).await.map_err(|e| {
                error!("Failed to create temporary file: {e}");
                map_err(field.name(), e)
            })?;
            debug!(
                "Dumping field {} to temporary location {}",
                field.name(),
                tmpf.name
            );
            if let Err(e) = dump_field(&mut tmpf, &mut field, limits).await {
                error!("Failed to write temporary file: {e}");
                tmpf.remove().await;
                Err(map_err(field.name(), e))
            } else {
                Ok(tmpf)
            }
        })
    }
}
