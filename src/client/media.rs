use super::{ClientRequest, FormatAction, TX};
use crate::{
    api::Api,
    channel::VideoMetadata,
    emit_msg,
    mpv::PlayerHandle,
    player,
    stream_formats::Formats,
    thumbnail::{Thumbnail, protocols::GraphicsProtocol},
    utils,
};
use anyhow::Result;
use std::{
    fs::File,
    io::{Read, Write},
    time::Duration,
};
use tokio::time::sleep;

pub async fn get_thumbnail(
    instance: Box<dyn Api>,
    protocol: GraphicsProtocol,
    video_id: &str,
) -> Result<Thumbnail> {
    let dir_path = utils::get_cache_dir()?.join("thumbnail");
    let path = dir_path.join(format!("{video_id}.jpg"));

    let mut bytes = Vec::new();

    if path.exists()
        && let Ok(mut file) = File::open(&path)
    {
        sleep(Duration::from_millis(10)).await;
        file.read_to_end(&mut bytes)?;
    } else {
        sleep(Duration::from_millis(69)).await;
        bytes = instance.get_thumbnail(video_id).await?;

        if !dir_path.exists() {
            std::fs::create_dir_all(&dir_path)?;
        }

        let mut file = File::create(&path)?;
        file.write_all(&bytes)?;
    }

    let image = image::load_from_memory(&bytes)?;
    let width = image.width() as u16;
    let height = image.height() as u16;
    let data = protocol.display_image(image, path)?;

    Ok(Thumbnail::new(data, width, height))
}

pub async fn fetch_formats(
    instance: Box<dyn Api>,
    audio_player: PlayerHandle,
    metadata: VideoMetadata,
    action: FormatAction,
) -> Result<()> {
    emit_msg!(perm, "Fetching formats");
    let video_info = instance.get_video_formats(&metadata.video_id).await;

    let formats = match video_info {
        Ok(video_info) => Formats::new(
            metadata.title.clone(),
            metadata.video_id.clone(),
            video_info,
        ),
        Err(e) => {
            emit_msg!(error, e.to_string());
            return Ok(());
        }
    };

    match action {
        FormatAction::Select => {
            emit_msg!();
            TX.send(ClientRequest::EnterFormatSelection(Box::new(formats)))?;
        }
        FormatAction::PlayVideo => {
            player::play_from_formats(instance, formats).await?;
        }
        FormatAction::PlayAudio => {
            let Some(source) = formats.get_selected_audio_url().map(str::to_owned) else {
                emit_msg!(error, "No playable audio stream available");
                return Ok(());
            };

            match audio_player.play(metadata, source) {
                Ok(()) => emit_msg!(),
                Err(error) => emit_msg!(error, error.to_string()),
            }
        }
    }

    Ok(())
}
