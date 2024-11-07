//! Backend TCP server
use crate::objects::{BackendRequest, BackendResultKind};
use std::io::Read;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

/// A listening TCP socket
#[derive(Debug)]
pub struct TcpServer {
    listener: std::net::TcpListener,
    stream: Option<std::net::TcpStream>,
}

impl TcpServer {
    /// Creates a new listening socket
    pub fn new(addr: &str, port: u16) -> Result<Self, std::io::Error> {
        // Note: TcpListener internally sets SO_REUSEADDR
        let listener = std::net::TcpListener::bind((addr, port)).map_err(|e| {
            error!("Failed to bind socket to {}:{}: {}", addr, port, e);
            e
        })?;

        info!("Backend listening on {}:{}", addr, port);
        Ok(Self {
            listener,
            stream: None,
        })
    }

    /// Retrieves the first available job request
    #[instrument]
    pub fn get_job_request(&mut self) -> BackendRequest {
        loop {
            let (mut stream, remote_address) = match self.listener.accept() {
                Ok(v) => v,
                Err(e) => {
                    warn!("Error accepting incoming connection: {}", e);
                    continue;
                }
            };
            debug!("Incoming connection from {}", remote_address);
            let mut breq: Vec<u8> = Vec::new();
            match stream.read_to_end(&mut breq) {
                Ok(_) => {}
                Err(e) => {
                    warn!("Failed to read request from {}: {}", remote_address, e);
                    continue;
                }
            }
            match serde_json::from_slice::<BackendRequest>(&breq) {
                Ok(request) => {
                    self.stream = Some(stream);
                    return request;
                }
                Err(e) => warn!("Invalid request from {}: {}", remote_address, e),
            }
        }
    }

    /// Sends back the job result
    pub fn send_job_result(
        &mut self,
        result: &BackendResultKind,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self.stream.take() {
            Some(stream) => Ok(serde_json::to_writer(stream, result)?),
            None => Err("Internal error: called out of context".into()),
        }
    }

    /// Inform the client that the job failed
    ///
    /// In fact it just closes the connection (the client will retry the job)
    pub fn send_job_failure(&mut self) {
        self.stream.take();
    }
}
