use crate::TX;
use crate::api::ApiBackend;
use crate::client::{Client, ClientRequest};
use crate::clipboard::{CopyStatus, copy_to_clipboard};
use crate::mpv::{PlayerHandle, VideoRequest, VideoSource};
use crate::process::run_detached;
use crate::{CONFIG, api::Api, app::VideoPlayer, emit_msg, stream_formats::Formats};
use anyhow::Result;
use tokio::process::Command;

pub async fn play_from_formats(
    instance: Box<dyn Api>,
    player: PlayerHandle,
    formats: Formats,
) -> Result<()> {
    let Some((video_url, audio_url)) = formats.get_selected_video_url() else {
        emit_msg!(error, "No playable stream available");
        return Ok(());
    };
    let metadata = &formats.spec.metadata;

    let captions = instance.get_caption_paths(&formats).await;

    let chapters = formats
        .chapters
        .as_ref()
        .and_then(|chapters| chapters.write_to_file(&metadata.video_id).ok());

    emit_msg!("Launching video player");

    match CONFIG.video_player_for_stream_formats {
        VideoPlayer::Mpv => {
            let request = VideoRequest {
                source: VideoSource::Direct {
                    video_url: video_url.to_owned(),
                    audio_url: audio_url.map(str::to_owned),
                    captions,
                    chapters,
                },
                spec: formats.spec,
            };

            if CONFIG.mpv_video_ipc {
                match player.play_video(request) {
                    Ok(()) => emit_msg!(),
                    Err(error) => emit_msg!(error, error.to_string()),
                }
            } else {
                play_mpv_without_ipc(request).await?;
            }

            Ok(())
        }
        VideoPlayer::Vlc => {
            let mut command = Command::new(&CONFIG.vlc_path);
            command
                .arg("--no-video-title-show")
                .arg(format!("--input-title-format={}", metadata.title))
                .arg("--play-and-exit")
                .arg(video_url);

            if let Some(audio_url) = audio_url {
                command.arg(format!("--input-slave={audio_url}"));
            }

            if !captions.is_empty() {
                command.arg(format!("--sub-file={}", captions.join(" ")));
            }

            play_video(command, &metadata.video_id).await
        }
    }
}

pub fn youtube_watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

pub async fn play_mpv_without_ipc(request: VideoRequest) -> Result<()> {
    let video_id = request.spec.metadata.video_id.clone();
    let command = crate::mpv::video_command_without_ipc(&request);

    play_video(command, &video_id).await
}

async fn play_video(player_command: Command, video_id: &str) -> Result<()> {
    TX.send(ClientRequest::SetWatched(video_id.to_owned(), true))?;

    if let Err(e) = run_detached(player_command).await {
        emit_msg!(error, e.to_string());
        TX.send(ClientRequest::SetWatched(video_id.to_owned(), false))?;
    }

    Ok(())
}

async fn invidious_url(client: &mut Client, url_component: &str) -> Result<String> {
    if client.invidious_instance.is_none() {
        client.set_instance().await?;
    }

    let instance = client
        .invidious_instance
        .as_ref()
        .expect("The function should return before if an instance couldn't be set");

    Ok(format!("{}/{}", instance.domain, url_component))
}

pub async fn copy_link(client: &mut Client, url_component: &str, api: ApiBackend) -> Result<()> {
    let url = match api {
        ApiBackend::Local => format!("https://www.youtube.com/{url_component}"),
        ApiBackend::Invidious => match invidious_url(client, url_component).await {
            Ok(url) => url,
            Err(e) => {
                emit_msg!(error, e.to_string());
                return Ok(());
            }
        },
    };

    match copy_to_clipboard(&url) {
        Ok(CopyStatus::Copied) => emit_msg!(format!("Copied: {url}")),
        Ok(CopyStatus::UnconfirmedOsc52) => {
            emit_msg!(format!("OSC52 copy sent: {url}"));
        }
        Err(e) => emit_msg!(error, e.to_string()),
    }

    Ok(())
}

pub fn open_in_youtube(url_component: &str) {
    open_in_browser(&format!("https://www.youtube.com/{url_component}"));
}

pub async fn open_in_invidious(client: &mut Client, url_component: &str) -> Result<()> {
    let url = match invidious_url(client, url_component).await {
        Ok(url) => url,
        Err(e) => {
            emit_msg!(error, e.to_string());
            return Ok(());
        }
    };

    open_in_browser(&url);

    Ok(())
}

pub fn open_in_browser(url: &str) {
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
}
