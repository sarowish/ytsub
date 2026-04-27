use crate::ro_cell::RoCell;
use crate::utils::env_var_is_set;
use anyhow::{Result, bail};
use ratatui::layout::Rect;
use std::env;
use std::path::Path;
use std::sync::OnceLock;
use std::{path::PathBuf, process::Stdio};
use tokio::io::AsyncWriteExt;
use tokio::{
    process::{Child, Command},
    sync::mpsc::{self, UnboundedSender},
};

type Cmd = Option<(PathBuf, Rect)>;
static DAEMON: RoCell<Option<UnboundedSender<Cmd>>> = RoCell::new();
pub static METHOD: OnceLock<&'static str> = OnceLock::new();

pub fn start() {
    let mut child = create_daemon().ok();
    let (tx, mut rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            let exit_status = child.as_mut().and_then(|c| c.try_wait().ok());

            if exit_status != Some(None) {
                child = None;
            }

            if child.is_none() {
                child = create_daemon().ok();
            }

            if let Some(child) = &mut child {
                send_command(child, cmd).await.ok();
            }
        }
    });

    DAEMON.init(Some(tx));
}

pub fn display_image(path: &Path, area: Rect) -> Result<()> {
    let Some(tx) = &*DAEMON else {
        bail!("ueberzugpp not initialized");
    };

    tx.send(Some((path.to_owned(), area)))?;

    Ok(())
}

pub fn compositor_support() -> Option<&'static str> {
    if env::var("XDG_SESSION_TYPE").is_ok_and(|var| var == "x11") {
        Some("x11")
    } else if env_var_is_set("SWAYSOCK")
        || env_var_is_set("HYPRLAND_INSTANCE_SIGNATURE")
        || env_var_is_set("WAYFIRE_SOCKET")
    {
        Some("wayland")
    } else {
        None
    }
}

fn create_daemon() -> Result<Child> {
    let child = Command::new("ueberzugpp")
        .arg("layer")
        .arg("-so")
        .arg(
            METHOD
                .get()
                .expect("ueberzugpp listener should be started after `METHOD` has been set."),
        )
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    Ok(child?)
}

async fn send_command(child: &mut Child, cmd: Cmd) -> Result<()> {
    let mut s = if let Some((path, area)) = cmd {
        serde_json::json!({
            "action": "add",
            "identifier": "ytsub",
            "x": area.x,
            "y": area.y,
            "max_width" : area.width,
            "max_height" : area.height,
            "path": path,
        })
        .to_string()
    } else {
        serde_json::json!({
            "action": "remove",
            "identifier": "ytsub",
        })
        .to_string()
    };

    s.push('\n');

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(s.as_bytes())
        .await?;

    Ok(())
}
