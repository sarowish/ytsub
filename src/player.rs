use crate::TX;
use crate::client::{Client, ClientRequest};
use crate::{OPTIONS, api::Api, app::VideoPlayer, emit_msg, stream_formats::Formats};
use anyhow::Result;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub async fn run_detached(mut command: Command) -> Result<()> {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(())
        })
    };

    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let exit_status = child.wait().await?;

    if let Some(code) = exit_status.code()
        && code != 0
    {
        Err(anyhow::anyhow!("Process exited with status code {code}"))
    } else {
        Ok(())
    }
}

pub async fn play_from_formats(instance: Box<dyn Api>, formats: Formats) -> Result<()> {
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

    let player_command = gen_video_player_command(
        video_url,
        audio_url,
        &captions,
        chapters.as_deref(),
        &formats.title,
    );

    play_video(player_command, &formats.id).await
}

pub async fn play_using_ytdlp(video_id: &str) -> Result<()> {
    let url = format!("{}/watch?v={}", "https://www.youtube.com", video_id);

    let mut player_command = Command::new(&OPTIONS.mpv_path);
    player_command.arg(url);

    play_video(player_command, video_id).await
}

async fn play_video(player_command: Command, video_id: &str) -> Result<()> {
    emit_msg!("Launching video player");
    TX.send(ClientRequest::SetWatched(video_id.to_owned(), true))?;

    if let Err(e) = run_detached(player_command).await {
        emit_msg!(error, e.to_string());
        TX.send(ClientRequest::SetWatched(video_id.to_owned(), false))?;
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
    open_in_browser(&format!("https://www.youtube.com/{url_component}"))
}

pub fn open_in_browser(url: &str) -> Result<()> {
    let commands = open::commands(url);
    let mut last_error = None;

    tokio::spawn(async move {
        for cmd in commands {
            let command = Command::from(cmd);

            match run_detached(command).await {
                Ok(()) => return Ok(()),
                Err(err) => last_error = Some(err),
            }
        }

        emit_msg!(error, &last_error.unwrap().to_string());
        anyhow::Ok(())
    });

    Ok(())
}
