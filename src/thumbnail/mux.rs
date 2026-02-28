use std::{borrow::Cow, env, process::Command, sync::LazyLock};

pub static IS_TMUX: LazyLock<bool> = LazyLock::new(detect_tmux);
pub static ESCAPE: LazyLock<&str> = LazyLock::new(|| if *IS_TMUX { "\x1b\x1b" } else { "\x1b" });
pub static START: LazyLock<&str> = LazyLock::new(|| {
    if *IS_TMUX {
        "\x1bPtmux;\x1b\x1b"
    } else {
        "\x1b"
    }
});
pub static END: LazyLock<&str> = LazyLock::new(|| if *IS_TMUX { "\x1b\\" } else { "" });

pub fn detect_tmux() -> bool {
    if !env::var("TERM").is_ok_and(|term| term.starts_with("tmux"))
        && !env::var("TERM_PROGRAM").is_ok_and(|term_program| term_program == "tmux")
    {
        return false;
    }

    let _ = Command::new("tmux")
        .args(["set", "-p", "allow-passthrough", "on"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| child.wait());

    true
}

pub fn read_term() -> Option<String> {
    let Ok(output) = Command::new("tmux").arg("show-environment").output() else {
        return None;
    };

    for (key, value) in String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().split_once('='))
    {
        if key == "TERM_PROGRAM" {
            return Some(value.to_owned());
        }
    }

    None
}

pub fn csi(s: &str) -> Cow<'_, str> {
    if *IS_TMUX {
        Cow::Owned(format!(
            "{}{}{}",
            *START,
            s.trim_start_matches('\x1b').replace('\x1b', *ESCAPE),
            *END,
        ))
    } else {
        Cow::Borrowed(s)
    }
}
