use crate::emulator::mux::{END, ESCAPE, START};
use crate::ro_cell::RoCell;
use anyhow::Result;
use base64::{Engine, prelude::BASE64_STANDARD};
use crossterm::execute;

#[cfg(unix)]
static CLIPBOARD_COMMAND: std::sync::LazyLock<Option<(&'static str, Vec<&'static str>)>> =
    std::sync::LazyLock::new(get_clipboard_provider);
pub static OSC52_SUPPORTED: RoCell<bool> = RoCell::with_const(false);

pub enum CopyStatus {
    Copied,
    UnconfirmedOsc52,
}

pub struct SetClipboard(String);

impl SetClipboard {
    pub fn new(content: &str) -> Self {
        Self(BASE64_STANDARD.encode(content))
    }
}

impl crossterm::Command for SetClipboard {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "{}]52;c;{}{}\\{}", *START, self.0, *ESCAPE, *END)
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        use std::io;

        // OSC 52 is used for fallback; so, just return an error if `write_ansi` isn't available.
        Err(io::Error::new(io::ErrorKind::Other, "Copy command failed"))
    }
}

fn copy_osc52(text: &str) -> Result<CopyStatus> {
    execute!(std::io::stdout(), SetClipboard::new(text))?;

    let status = if *OSC52_SUPPORTED {
        CopyStatus::Copied
    } else {
        CopyStatus::UnconfirmedOsc52
    };

    Ok(status)
}

#[cfg(unix)]
fn get_clipboard_provider() -> Option<(&'static str, Vec<&'static str>)> {
    use crate::utils::{binary_exists, env_var_is_set};

    let res = if binary_exists("pbcopy") {
        ("pbcopy", Vec::new())
    } else if env_var_is_set("WAYLAND_DISPLAY") && binary_exists("wl-copy") {
        ("wl-copy", vec!["--type", "text/plain"])
    } else if env_var_is_set("WAYLAND_DISPLAY") && binary_exists("waycopy") {
        ("waycopy", vec![])
    } else if env_var_is_set("DISPLAY") && binary_exists("xclip") {
        ("xclip", vec!["-i", "-selection", "clipboard"])
    } else if env_var_is_set("DISPLAY") && binary_exists("xsel") {
        ("xsel", vec!["--nodetach", "-i", "-b"])
    } else {
        return None;
    };

    Some(res)
}

#[cfg(unix)]
pub fn copy_to_clipboard(text: &str) -> Result<CopyStatus> {
    use std::{io::Write, process::Stdio};

    let Some(command) = CLIPBOARD_COMMAND.as_ref() else {
        return copy_osc52(text);
    };

    let mut child = std::process::Command::new(command.0)
        .args(&command.1)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }

    let status = child.wait()?;
    if status.success() {
        Ok(CopyStatus::Copied)
    } else {
        copy_osc52(text)
    }
}

#[cfg(windows)]
pub fn copy_to_clipboard(text: &str) -> Result<CopyStatus> {
    use clipboard_win::set_clipboard_string;

    set_clipboard_string(text)
        .map(|_| CopyStatus::Copied)
        .or_else(|_| copy_osc52(text))
}
