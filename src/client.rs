use crate::{
    IoEvent, OPTIONS,
    api::{Api, ChannelFeed},
    channel::RefreshState,
    message::MessageType,
    ro_cell::RoCell,
    utils,
};
use anyhow::Result;
use futures_util::StreamExt;
use std::time::Instant;
use tokio::{
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot::{self, Sender},
    },
    time::sleep,
};
use tokio_util::sync::CancellationToken;

pub enum ClientRequest {
    SetRefreshState(String, RefreshState),
    SetImportState(String, RefreshState),
    AddChannel(ChannelFeed),
    UpdateChannel(ChannelFeed),
    FinalizeImport(bool),
    SetMessage(String, MessageType, Option<u64>),
    SetInstances(Result<Vec<String>>),
    CheckChannel(String, Sender<bool>),
    ClearMessage,
}

const MESSAGE_DURATION: u64 = 5;

macro_rules! emit_msg {
    () => {
        TX.send(ClientRequest::ClearMessage)?
    };
    ($message: expr) => {
        emit_msg!($message, MessageType::Normal)
    };
    (perm, $message: expr) => {
        TX.send(ClientRequest::SetMessage(
            $message.to_owned(),
            MessageType::Normal,
            None,
        ))?
    };
    (error, $message: expr) => {
        emit_msg!($message, MessageType::Error)
    };
    (warning, $message: expr) => {
        emit_msg!($message, MessageType::Warning)
    };
    ($message: expr, $message_type: expr) => {
        TX.send(ClientRequest::SetMessage(
            $message.to_owned(),
            $message_type,
            Some(MESSAGE_DURATION),
        ))?
    };
}

pub static TX: RoCell<UnboundedSender<ClientRequest>> = RoCell::new();

pub struct Client {
    rx: UnboundedReceiver<IoEvent>,
}

impl Client {
    pub fn new(rx: UnboundedReceiver<IoEvent>) -> Self {
        Self { rx }
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(event) = self.rx.recv().await {
            match event {
                IoEvent::SubscribeToChannel(id, instance) => {
                    tokio::spawn(async move { subscribe_to_channel(instance, id).await });
                }
                IoEvent::ImportChannels(ids, instance) => {
                    import_channels(instance, ids).await?;
                }
                IoEvent::RefreshChannels(ids, instance) => {
                    tokio::spawn(async move { refresh_channels(instance, ids).await });
                }
                IoEvent::FetchInstances => {
                    tokio::spawn(async move {
                        let instances = utils::fetch_invidious_instances().await;
                        TX.send(ClientRequest::SetInstances(instances))
                    });
                }
                IoEvent::ClearMessage(token, duration) => {
                    tokio::spawn(async move { clear_message(token, duration).await });
                }
            }
        }

        Ok(())
    }
}

async fn subscribe_to_channel(mut instance: Box<dyn Api>, input: String) -> Result<()> {
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
        Ok(channel_feed) => {
            emit_msg!();
            TX.send(ClientRequest::AddChannel(channel_feed))?;
        }
        Err(e) => emit_msg!(error, format!("Failed to subscribe: {e}")),
    }

    Ok(())
}

async fn import_channels(instance: Box<dyn Api>, channel_ids: Vec<String>) -> Result<()> {
    let start = Instant::now();
    let (mut count, total) = (0, channel_ids.len());

    emit_msg!(perm, format!("Subscribing to channels: {count}/{total}"));

    let streams = futures_util::stream::iter(channel_ids).map(|id| {
        let id = id.clone();
        let mut instance = dyn_clone::clone_box(&*instance);

        TX.send(ClientRequest::SetImportState(
            id.clone(),
            RefreshState::Refreshing,
        ))
        .unwrap();

        tokio::spawn(async move {
            let feed = if total > OPTIONS.rss_threshold {
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
            Ok(feed) => {
                TX.send(ClientRequest::SetImportState(id, RefreshState::Completed))?;
                TX.send(ClientRequest::AddChannel(feed))?;
                emit_msg!(perm, format!("Subscribing to channels: {count}/{total}"));
                count += 1;
            }
            Err(_) => TX.send(ClientRequest::SetImportState(id, RefreshState::Failed))?,
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

async fn refresh_channels(instance: Box<dyn Api>, channel_ids: Vec<String>) -> Result<()> {
    let start = Instant::now();
    let (mut count, total) = (0, channel_ids.len());

    if total == 1 {
        emit_msg!(perm, "Refreshing channel");
    } else {
        emit_msg!(perm, format!("Refreshing channels: {count}/{total}"));
    }

    let streams = futures_util::stream::iter(channel_ids).map(|id| {
        let id = id.clone();
        let mut instance = dyn_clone::clone_box(&*instance);

        TX.send(ClientRequest::SetRefreshState(
            id.clone(),
            RefreshState::Refreshing,
        ))
        .unwrap();

        tokio::spawn(async move {
            let feed = if total > OPTIONS.rss_threshold {
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

async fn clear_message(token: CancellationToken, duration: u64) -> Result<()> {
    tokio::select! {
        () = token.cancelled() => {}
        () = sleep(std::time::Duration::from_secs(duration)) => emit_msg!(),

    }

    Ok(())
}
