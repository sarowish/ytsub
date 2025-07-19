use crate::ClientRequest;
use crate::TX;
use crate::client::Client;
use crate::{OPTIONS, api::Api, app::VideoPlayer, emit_msg, stream_formats::Formats};
use anyhow::Result;
use std::{path::Path, process::Command};

#[cfg(unix)]
pub fn run_detached<F: FnOnce() -> Result<i32, std::io::Error>>(func: F) -> Result<()> {
    use nix::sys::wait::{WaitStatus, wait};
    use nix::unistd::ForkResult::{Child, Parent};
    use nix::unistd::{close, dup2_stderr, dup2_stdin, dup2_stdout, fork, pipe, setsid};
    use std::fs::File;
    use std::io::prelude::*;
    use std::os::fd::AsFd;
    use std::os::unix::io::{FromRawFd, IntoRawFd};

    let (pipe_r, pipe_w) = pipe()?;
    let pid = unsafe { fork()? };
    match pid {
        Parent { .. } => {
            tokio::spawn(async move {
                if let Ok(WaitStatus::Exited(_, exit_code)) = wait() {
                    if exit_code == 101 {
                        close(pipe_w.into_raw_fd())?;
                        let mut file = unsafe { File::from_raw_fd(pipe_r.into_raw_fd()) };
                        let mut error_message = String::new();
                        file.read_to_string(&mut error_message)?;
                        emit_msg!(error, error_message);
                    } else if exit_code != 0 {
                        emit_msg!(
                            error,
                            format!("Process exited with status code {exit_code}")
                        );
                    }
                }

                anyhow::Ok(())
            });

            Ok(())
        }
        Child => {
            setsid()?;
            let dev_null = std::fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open("/dev/null")?;
            let null_fd = dev_null.as_fd();

            dup2_stdin(null_fd)?;
            dup2_stdout(null_fd)?;
            dup2_stderr(null_fd)?;

            match func() {
                Ok(exit_status) => {
                    std::process::exit(exit_status);
                }
                Err(e) => {
                    close(pipe_r.into_raw_fd())?;
                    dup2_stdout(pipe_w.as_fd())?;
                    println!("{e}");
                    std::process::exit(101);
                }
            }
        }
    }
}

pub async fn play_video(instance: Box<dyn Api>, formats: Formats) -> Result<()> {
    emit_msg!("Launching video player");

    let (video_url, audio_url) = if formats.use_adaptive_streams {
        (
            formats.video_formats.get_selected_item().get_url(),
            Some(formats.audio_formats.get_selected_item().get_url()),
        )
    } else {
        (formats.formats.get_selected_item().get_url(), None)
    };

    let captions = instance.get_caption_paths(&formats).await;

    let chapters = formats
        .chapters
        .and_then(|chapters| chapters.write_to_file(&formats.id).ok());

    let mut video_player_command = gen_video_player_command(
        video_url,
        audio_url,
        &captions,
        chapters.as_deref(),
        &formats.title,
    );

    let video_player_process = || {
        video_player_command
            .spawn()
            .and_then(|mut child| child.wait())
            .map(|status| status.code().unwrap_or_default())
    };

    if let Err(e) = run_detached(video_player_process) {
        emit_msg!(error, e.to_string());
    } else {
        TX.send(ClientRequest::MarkAsWatched(formats.id))?;
    }

    Ok(())
}

fn gen_video_player_command(
    video_url: &str,
    audio_url: Option<&str>,
    captions: &[String],
    chapters: Option<&Path>,
    title: &str,
) -> Command {
    let mut command;
    match OPTIONS.video_player_for_stream_formats {
        VideoPlayer::Mpv => {
            command = Command::new(&OPTIONS.mpv_path);
            command
                .arg(format!("--force-media-title={title}"))
                .arg("--no-ytdl")
                .arg(video_url);

            if let Some(audio_url) = audio_url {
                command.arg(format!("--audio-file={audio_url}"));
            }

            for caption in captions {
                command.arg(format!("--sub-file={caption}"));
            }

            if let Some(path) = chapters {
                command.arg(format!("--chapters-file={}", path.display()));
            }
        }
        VideoPlayer::Vlc => {
            command = Command::new(&OPTIONS.vlc_path);
            command
                .arg("--no-video-title-show")
                .arg(format!("--input-title-format={title}"))
                .arg("--play-and-exit")
                .arg(video_url);

            if let Some(audio_url) = audio_url {
                command.arg(format!("--input-slave={audio_url}"));
            }

            if !captions.is_empty() {
                command.arg(format!("--sub-file={}", captions.join(" ")));
            }
        }
    }

    command
}

pub fn open_in_invidious(client: &mut Client, url_component: &str) -> Result<()> {
    let Some(instance) = &client.invidious_instance else {
        emit_msg!(error, "No Invidious instances available.");
        return Ok(());
    };

    let url = format!("{}/{}", instance.domain, url_component);

    open_in_browser(&url)
}

pub fn open_in_youtube(url_component: &str) -> Result<()> {
    const YOUTUBE_URL: &str = "https://www.youtube.com";

    let url = format!("{YOUTUBE_URL}/{url_component}");

    open_in_browser(&url)
}

pub fn open_in_browser(url: &str) -> Result<()> {
    let browser_process = || webbrowser::open(url).map(|()| 0);

    #[cfg(unix)]
    let res = run_detached(browser_process);
    #[cfg(not(unix))]
    let res = browser_process();

    if let Err(e) = res {
        emit_msg!(error, &e.to_string());
    }

    Ok(())
}
