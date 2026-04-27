use anyhow::Result;
use ratatui::layout::Rect;
use std::{
    path::Path,
    process::{Command, Stdio},
};

pub fn show_image(path: &Path, area: Rect) -> Result<Vec<u8>> {
    let child = Command::new("chafa")
        .arg("-f")
        .arg("symbols")
        .arg("--probe")
        .arg("off")
        .arg("--polite")
        .arg("on")
        .arg("--passthrough")
        .arg("none")
        .arg("--view-size")
        .arg(format!("{}x{}", area.width, area.height))
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let output = child.wait_with_output()?;

    Ok(output.stdout)
}
