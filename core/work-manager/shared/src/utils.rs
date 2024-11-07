//! Shared utility functions
use rand::distributions::{Alphanumeric, DistString};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

/// Reads all data from a socket, until the remote end is closed
pub async fn read_all(
    stream: &mut tokio::net::TcpStream,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut reply: Vec<u8> = Vec::new();
    let mut buf = [0; 4096];
    loop {
        stream.readable().await?;
        match stream.try_read(&mut buf) {
            Ok(0) => break,
            Ok(len) => reply.extend_from_slice(&buf[0..len]),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(reply)
}

// FIXME: how random is random?
/// Generates a random alphanumeric [`String`] of the given length
pub fn random_string(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), len)
}

/// Utility fn to retrieve the proper request queue for a given object type
pub fn get_queue_for(object_type: &str) -> String {
    format!("CTX-JobReq-{}", object_type.to_uppercase())
}
