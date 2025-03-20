use tokio::{io::AsyncReadExt, io::AsyncWriteExt, net, time};

use std::sync::Arc;
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

pub struct Clamd {
    clamd: tokio::process::Child,
    ping_task: tokio::task::JoinHandle<()>,
    reload_notice: Arc<tokio::sync::Notify>,
}

impl Clamd {
    const MAX_PING_FAILURES: u32 = 3;
    const MAX_START_TIME_SECS: u32 = 90;

    pub async fn new(
        reload_notice: Arc<tokio::sync::Notify>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let clamd = tokio::process::Command::new("clamd")
            .arg("--foreground")
            .spawn()
            .inspect_err(|e| error!("Failed to start clamd: {}", e))?;
        let mut dt = 0u32;
        debug!("Awaiting Clamd startup...");
        while dt < Self::MAX_START_TIME_SECS {
            if let Ok(Ok(_)) = Self::comm("PING", "PONG").await {
                break;
            }
            time::sleep(time::Duration::from_secs(2)).await;
            dt += 2;
        }
        if dt >= Self::MAX_START_TIME_SECS {
            error!(
                "Clamd failed to start in {} seconds",
                Self::MAX_START_TIME_SECS
            );
            return Err("Clamd failed to start".into());
        }
        let ping_task = tokio::spawn(async move {
            let mut failed_count = 0u32;
            while failed_count < Self::MAX_PING_FAILURES {
                time::sleep(time::Duration::from_secs(20)).await;
                match Self::comm("PING", "PONG").await {
                    Ok(Ok(_)) => {
                        if failed_count != 0 {
                            debug!("Clamd is alive again");
                        }
                        failed_count = 0;
                        continue;
                    }
                    Ok(Err(e)) => {
                        warn!(
                            "Clamd PING command failed (failures: {}/{}): {}",
                            failed_count,
                            Self::MAX_PING_FAILURES,
                            e
                        );
                    }
                    Err(_) => {
                        warn!(
                            "Clamd PING time out (failures: {}/{})",
                            failed_count,
                            Self::MAX_PING_FAILURES,
                        );
                    }
                }
                failed_count += 1;
            }
        });
        info!("Clamd started");
        Ok(Self {
            clamd,
            ping_task,
            reload_notice,
        })
    }

    pub async fn watch(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            tokio::select!(
                _ = self.clamd.wait() => {
                    error!("Clamd exited");
                    return Err("Clamd exited".into());
                }
                res = &mut self.ping_task => {
                    error!("Clamd lost");
                    if let Err(e) = res {
                        return Err(e.into());
                    } else {
                        return Err("Clamd ping time out".into());
                    }
                }
                _ = self.reload_notice.notified() => {
                    self.reload().await?;
                }
            )
        }
    }

    async fn reload(&self) -> Result<(), Box<dyn std::error::Error>> {
        match Self::comm("RELOAD", "RELOADING").await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                error!("Clamd reload error");
                Err(e)
            }
            Err(e) => {
                error!("Clamd reload response time out");
                Err(e.into())
            }
        }
    }

    async fn comm(
        send: &str,
        expect: &str,
    ) -> Result<Result<(), Box<dyn std::error::Error>>, time::error::Elapsed> {
        time::timeout(time::Duration::from_secs(5), async {
            debug!("Sending command {}...", send);
            let mut stream = net::TcpStream::connect(("localhost", 3310)).await?;
            stream.set_nodelay(true).ok();
            stream.set_linger(None).ok();
            let send = format!("z{}\0", send);
            let expect = format!("{}\0", expect);
            let mut buf: Vec<u8> = vec![0; expect.len()];
            stream.write_all(send.as_bytes()).await?;
            stream.read_exact(&mut buf).await?;
            if buf == expect.as_bytes() {
                debug!("Correct command reply received");
                Ok(())
            } else {
                debug!("Unexpected command reply received");
                Err("Invalid reply from clamd".into())
            }
        })
        .await
    }
}

impl Drop for Clamd {
    fn drop(&mut self) {
        self.clamd.start_kill().ok();
    }
}

pub struct Freshclam(Option<tokio::process::Child>);

impl Freshclam {
    pub fn new(disable_freshclam: bool) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self(if disable_freshclam {
            None
        } else {
            let freshclam = tokio::process::Command::new("freshclam")
                .arg("--daemon")
                .arg("--foreground")
                .spawn()
                .inspect_err(|e| error!("Failed to start clamd: {}", e))?;
            Some(freshclam)
        }))
    }

    pub async fn watch(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut freshclam) = self.0 {
            freshclam.wait().await.ok();
            error!("Freshclam exited");
            Err("Freshclam exited".into())
        } else {
            futures::future::pending::<tokio::process::Child>().await;
            unreachable!();
        }
    }
}

impl Drop for Freshclam {
    fn drop(&mut self) {
        if let Some(ref mut freshclam) = self.0 {
            freshclam.start_kill().ok();
        }
    }
}
