use super::{ClientRequest, TX};
use crate::{
    CONFIG,
    api::{Api, local::Local},
    channel::{ChannelTab, RefreshState},
    emit_msg,
};
use anyhow::Result;
use futures_util::StreamExt;
use std::{collections::HashSet, time::Instant};
use tokio::sync::oneshot;

pub async fn subscribe_to_channel(mut instance: Box<dyn Api>, input: String) -> Result<()> {
    let res = instance.resolve_channel_id(&input).await;

    let channel_id = match res {
        Ok(id) => id,
        Err(e) => {
            emit_msg!(error, format!("Failed to subscribe: {e}"));
            return Ok(());
        }
    };

    let (tx, rx) = oneshot::channel();
    TX.send(ClientRequest::CheckChannel(channel_id.clone(), tx))?;

    if rx.await? {
        emit_msg!(warning, "Already subscribed to the channel");
        return Ok(());
    }

    emit_msg!(perm, "Subscribing to channel");

    let channel_feed = instance.get_videos_for_the_first_time(&channel_id).await;

    match channel_feed {
        Ok(channel_feed) if channel_feed.channel_title.is_some() => {
            emit_msg!();
            TX.send(ClientRequest::AddChannel(channel_feed))?;
        }
        Err(e) => emit_msg!(error, format!("Failed to subscribe: {e}")),
        _ => emit_msg!(
            error,
            format!("Failed to subscribe: no channel title present")
        ),
    }

    Ok(())
}

pub async fn import_channels(instance: Box<dyn Api>, channel_ids: Vec<String>) -> Result<()> {
    let start = Instant::now();
    let (mut count, total) = (0, channel_ids.len());

    emit_msg!(perm, format!("Subscribing to channels: {count}/{total}"));

    let streams = futures_util::stream::iter(channel_ids).map(|id| {
        let mut instance = dyn_clone::clone_box(&*instance);

        TX.send(ClientRequest::SetImportState(
            id.clone(),
            RefreshState::Refreshing,
        ))
        .unwrap();

        tokio::spawn(async move {
            let feed = if total > CONFIG.rss_threshold {
                instance.get_rss_feed_of_channel(&id)
            } else {
                instance.get_videos_for_the_first_time(&id)
            };

            (feed.await, id)
        })
    });

    let mut buffered = streams.buffer_unordered(num_cpus::get());

    while let Some(Ok((feed, id))) = buffered.next().await {
        match feed {
            Ok(feed) if feed.channel_title.is_some() => {
                TX.send(ClientRequest::SetImportState(id, RefreshState::Completed))?;
                TX.send(ClientRequest::AddChannel(feed))?;
                emit_msg!(perm, format!("Subscribing to channels: {count}/{total}"));
                count += 1;
            }
            _ => TX.send(ClientRequest::SetImportState(id, RefreshState::Failed))?,
        }
    }

    let elapsed = start.elapsed().as_secs_f64();

    match count {
        0 => emit_msg!(error, "Failed to refresh channel"),
        count => emit_msg!(format!(
            "Subscribed to {count} out of {total} channels in {elapsed:.2}s"
        )),
    }

    TX.send(ClientRequest::FinalizeImport(count == total))?;

    Ok(())
}

pub async fn refresh_channels(instance: Box<dyn Api>, channel_ids: Vec<String>) -> Result<()> {
    let start = Instant::now();
    let (mut count, total) = (0, channel_ids.len());

    if total == 1 {
        emit_msg!(perm, "Refreshing channel");
    } else {
        emit_msg!(perm, format!("Refreshing channels: {count}/{total}"));
    }

    let streams = futures_util::stream::iter(channel_ids).map(|id| {
        let mut instance = dyn_clone::clone_box(&*instance);

        TX.send(ClientRequest::SetRefreshState(
            id.clone(),
            RefreshState::Refreshing,
        ))
        .unwrap();

        tokio::spawn(async move {
            let feed = if total > CONFIG.rss_threshold {
                instance.get_rss_feed_of_channel(&id)
            } else {
                instance.get_videos_of_channel(&id)
            };

            (feed.await, id)
        })
    });

    let mut buffered = streams.buffer_unordered(num_cpus::get());

    while let Some(Ok((feed, id))) = buffered.next().await {
        match feed {
            Ok(feed) => {
                TX.send(ClientRequest::SetRefreshState(id, RefreshState::Completed))?;
                TX.send(ClientRequest::UpdateChannel(feed))?;
                emit_msg!(perm, format!("Refreshing channels: {count}/{total}"));
                count += 1;
            }
            Err(_) => TX.send(ClientRequest::SetRefreshState(id, RefreshState::Failed))?,
        }
    }

    let elapsed = start.elapsed().as_secs_f64();

    match (count, total) {
        (0, 1) => emit_msg!(error, "Failed to refresh channel"),
        (0, _) => emit_msg!(error, "Failed to refresh channels"),
        (1, 1) => emit_msg!(format!("Refreshed channel in {elapsed:.2}s")),
        (count, total) => emit_msg!(format!(
            "Refreshed {count} out of {total} channels in {elapsed:.2}s"
        )),
    }

    Ok(())
}

pub async fn get_more_videos(
    mut instance: Box<dyn Api>,
    id: &str,
    tab: ChannelTab,
    present: HashSet<String>,
    get_all: bool,
) -> Result<()> {
    let start = Instant::now();

    match instance.get_more_videos(id, tab, &present, get_all).await {
        Ok(feed) => {
            if feed.get_videos(tab).is_empty() {
                emit_msg!(warning, "There are no videos to load");
            } else {
                let elapsed = start.elapsed().as_secs_f64();
                let new_count = feed
                    .get_videos(tab)
                    .iter()
                    .filter(|v| !present.contains(&v.video_id))
                    .count();

                let video_label = if new_count == 1 { "video" } else { "videos" };
                emit_msg!(format!(
                    "Loaded {new_count} more {video_label} in {elapsed:.2}s"
                ));

                TX.send(ClientRequest::UpdateChannel(feed))?;
            }
        }
        Err(e) => emit_msg!(error, &e.to_string()),
    }

    Ok(())
}

pub async fn get_video_title(local: Local, video_id: &str) -> Result<()> {
    let title = local.get_original_title(video_id).await?;
    TX.send(ClientRequest::UpdateTitle(video_id.to_owned(), title))?;

    Ok(())
}
