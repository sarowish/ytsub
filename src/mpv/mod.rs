use self::ipc::{MpvIpc, MpvNotification};
use crate::CONFIG;
use crate::process::detach_process;
use crate::video::PlaybackSpec;
use anyhow::{Context, Result, bail};
pub use controller::{
    PlaybackEndReason, PlaybackPhase, PlaybackState, PlaybackUpdate, PlaybackUpdateCause,
    PlayerHandle,
};
use std::ffi::OsString;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{path::PathBuf, time::Duration};
use tokio::{
    process::Command,
    sync::mpsc,
    time::{Instant, sleep},
};

mod controller;
mod ipc;

static SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackKind {
    Audio,
    Video,
}

pub enum VideoSource {
    YtDlp(String),
    Direct {
        video_url: String,
        audio_url: Option<String>,
        captions: Vec<String>,
        chapters: Option<PathBuf>,
    },
}

pub struct VideoRequest {
    pub spec: PlaybackSpec,
    pub source: VideoSource,
}

struct MpvLaunch {
    kind: PlaybackKind,
    uses_ytdlp: bool,
    extra_args: Vec<OsString>,
}

impl MpvLaunch {
    fn from_video(request: &VideoRequest) -> Self {
        let mut args = Vec::<OsString>::new();

        let uses_ytdlp = match &request.source {
            VideoSource::YtDlp(_) => true,
            VideoSource::Direct {
                audio_url,
                captions,
                chapters,
                ..
            } => {
                args.push("--no-ytdl".into());
                args.push(format!("--force-media-title={}", request.spec.metadata.title).into());

                if let Some(url) = audio_url {
                    args.push(format!("--audio-file={url}").into());
                }

                for caption in captions {
                    args.push(format!("--sub-file={caption}").into());
                }

                if let Some(chapters) = chapters {
                    args.push(format!("--chapters-file={}", chapters.display()).into());
                }

                false
            }
        };

        Self {
            kind: PlaybackKind::Video,
            uses_ytdlp,
            extra_args: args,
        }
    }
}

fn configure_proxy(command: &mut Command, uses_ytdlp: bool) {
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
    std::env::temp_dir().join(format!(
        "ytsub-mpv-{}-{}.sock",
        std::process::id(),
        SESSION_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(windows)]
fn ipc_endpoint() -> PathBuf {
    PathBuf::from(format!(
        r"\\.\pipe\ytsub-mpv-{}-{}",
        std::process::id(),
        SESSION_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

struct MpvSession {
    kind: PlaybackKind,
    _child: tokio::process::Child,
    ipc: MpvIpc,
    notifications: mpsc::UnboundedReceiver<MpvNotification>,
    endpoint: PathBuf,
}

impl MpvSession {
    async fn new(launch: MpvLaunch) -> Result<Self> {
        const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
        const RETRY_INTERVAL: Duration = Duration::from_millis(25);

        let endpoint = ipc_endpoint();
        let MpvLaunch {
            kind,
            uses_ytdlp,
            extra_args,
        } = launch;

        let mut command = Command::new(&CONFIG.mpv_path);
        command
            .arg("--no-terminal")
            .arg(format!("--input-ipc-server={}", endpoint.display()));
        configure_proxy(&mut command, uses_ytdlp);

        match kind {
            PlaybackKind::Audio => {
                command.arg("--idle=yes").arg("--vid=no");
            }
            PlaybackKind::Video => {
                command.arg("--idle=once");
                detach_process(&mut command);
            }
        }

        command.args(extra_args);

        let kill_on_drop = kind == PlaybackKind::Audio;
        let mut child = command.kill_on_drop(kill_on_drop).spawn()?;

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
            kind,
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
