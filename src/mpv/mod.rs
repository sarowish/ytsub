use self::ipc::{MpvIpc, MpvNotification};
use crate::CONFIG;
use anyhow::{Context, Result, bail};
pub use controller::{PlaybackPhase, PlaybackState, PlayerHandle};
use std::{path::PathBuf, time::Duration};
use tokio::{
    process::Command,
    sync::mpsc,
    time::{Instant, sleep},
};

mod controller;
mod ipc;

pub fn configure_proxy(command: &mut Command, uses_ytdlp: bool) {
    let Some(proxy) = CONFIG.mpv_proxy.as_deref() else {
        return;
    };

    command.arg(format!("--http-proxy={proxy}"));

    if uses_ytdlp {
        command.arg(format!("--ytdl-raw-options-append=proxy={proxy}"));
    }
}

#[cfg(unix)]
fn ipc_endpoint() -> PathBuf {
    std::env::temp_dir().join(format!("ytsub-mpv-{}.sock", std::process::id()))
}

#[cfg(windows)]
fn ipc_endpoint() -> PathBuf {
    PathBuf::from(format!(r"\\.\pipe\ytsub-mpv-{}", std::process::id()))
}

struct MpvSession {
    _child: tokio::process::Child,
    ipc: MpvIpc,
    notifications: mpsc::UnboundedReceiver<MpvNotification>,
    endpoint: PathBuf,
}

impl MpvSession {
    async fn new() -> Result<Self> {
        const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
        const RETRY_INTERVAL: Duration = Duration::from_millis(25);

        let endpoint = ipc_endpoint();

        let mut command = Command::new(&CONFIG.mpv_path);
        command
            .arg("--idle=yes")
            .arg("--no-terminal")
            .arg("--vid=no")
            .arg(format!("--input-ipc-server={}", endpoint.display()));
        configure_proxy(&mut command, true);

        let mut child = command.kill_on_drop(true).spawn()?;

        let deadline = Instant::now() + CONNECT_TIMEOUT;

        let (ipc, notifications) = loop {
            match MpvIpc::connect(&endpoint).await {
                Ok(connection) => break connection,
                Err(error) => {
                    if let Some(status) = child.try_wait()? {
                        bail!("mpv exited before opening its IPC endpoint: {status}");
                    }

                    if Instant::now() >= deadline {
                        return Err(error).context("timed out waiting for mpv IPC endpoint");
                    }

                    sleep(RETRY_INTERVAL).await;
                }
            }
        };

        ipc.observe_property("pause").await?;
        ipc.observe_property("duration").await?;
        ipc.observe_property("volume").await?;
        ipc.observe_property("mute").await?;

        Ok(Self {
            _child: child,
            ipc,
            notifications,
            endpoint,
        })
    }
}

#[cfg(unix)]
impl Drop for MpvSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.endpoint);
    }
}
