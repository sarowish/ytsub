use crate::{
    IoEvent, OPTIONS,
    api::{Api, ApiBackend, ChannelFeed, invidious::Instance, local::Local},
    channel::RefreshState,
    message::MessageType,
    player::{self, open_in_invidious, open_in_youtube, play_from_formats, play_using_ytdlp},
    ro_cell::RoCell,
    stream_formats::Formats,
    utils,
};
use anyhow::Result;
use futures_util::StreamExt;
use std::{collections::HashSet, time::Instant};
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
    CheckChannel(String, Sender<bool>),
    FinalizeImport(bool),
    UpdateChannel(ChannelFeed),
    EnterFormatSelection(Box<Formats>),
    SetWatched(String, bool),
    SetMessage(String, MessageType, Option<u64>),
    ClearMessage,
}

#[macro_export]
macro_rules! emit_msg {
    () => {
        TX.send($crate::client::ClientRequest::ClearMessage)?
    };
    ($message: expr) => {
        emit_msg!($message, $crate::message::MessageType::Normal)
    };
    (perm, $message: expr) => {
        TX.send($crate::client::ClientRequest::SetMessage(
            $message.to_owned(),
            $crate::message::MessageType::Normal,
            None,
        ))?
    };
    (error, $message: expr) => {
        emit_msg!($message, $crate::message::MessageType::Error)
    };
    (warning, $message: expr) => {
        emit_msg!($message, $crate::message::MessageType::Warning)
    };
    ($message: expr, $message_type: expr) => {
        TX.send($crate::client::ClientRequest::SetMessage(
            $message.to_owned(),
            $message_type,
            Some(5),
        ))?
    };
}

pub static TX: RoCell<UnboundedSender<ClientRequest>> = RoCell::new();

pub struct Client {
    rx: UnboundedReceiver<IoEvent>,
    pub invidious_instances: Option<Vec<String>>,
    pub invidious_instance: Option<Instance>,
    local_api: Local,
    pub selected_api: ApiBackend,
}

impl Client {
    pub async fn new(rx: UnboundedReceiver<IoEvent>) -> Result<Self> {
        let mut client = Self {
            rx,
            invidious_instances: utils::read_instances().ok(),
            invidious_instance: None,
            local_api: Local::new(),
            selected_api: OPTIONS.api,
        };

        if let ApiBackend::Invidious = client.selected_api {
            client.set_instance().await?;
        }

        Ok(client)
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(event) = self.rx.recv().await {
            match event {
                IoEvent::SubscribeToChannel(id) => {
                    let instance = self.instance();
                    tokio::spawn(async move { subscribe_to_channel(instance, id).await });
                }
                IoEvent::ImportChannels(ids) => {
                    let instance = self.instance();
                    import_channels(instance, ids).await?;
                }
                IoEvent::RefreshChannels(ids) => {
                    let instance = self.instance();
                    tokio::spawn(async move { refresh_channels(instance, ids).await });
                }
                IoEvent::LoadMoreVideos(id, present_videos) => {
                    let instance = self.instance();
                    tokio::spawn(
                        async move { get_more_videos(instance, &id, present_videos).await },
                    );
                }
                IoEvent::FetchFormats(title, video_id, play_selected) => {
                    let instance = self.instance();
                    tokio::spawn(async move {
                        fetch_formats(instance, title, video_id, play_selected).await
                    });
                }
                IoEvent::PlayFromFormats(formats) => {
                    let instance = self.instance();
                    tokio::spawn(async move { play_from_formats(instance, *formats).await });
                }
                IoEvent::PlayUsingYtdlp(video_id) => {
                    tokio::spawn(async move { play_using_ytdlp(&video_id).await });
                }
                IoEvent::OpenInBrowser(url_component, api) => match api {
                    ApiBackend::Local => open_in_youtube(&url_component),
                    ApiBackend::Invidious => open_in_invidious(self, &url_component).await?,
                },
                IoEvent::ClearMessage(token, duration) => {
                    tokio::spawn(async move { clear_message(token, duration).await });
                }
                IoEvent::SwitchApi => self.switch_api().await?,
            }
        }

        Ok(())
    }

    fn instance(&self) -> Box<dyn Api> {
        match self.selected_api {
            ApiBackend::Invidious => Box::new(self.invidious_instance.as_ref().unwrap().clone()),
            ApiBackend::Local => Box::new(self.local_api.clone()),
        }
    }

    async fn switch_api(&mut self) -> Result<()> {
        self.selected_api = match self.selected_api {
            ApiBackend::Local => ApiBackend::Invidious,
            ApiBackend::Invidious => ApiBackend::Local,
        };

        emit_msg!(format!("Selected API: {}", self.selected_api));

        if self.invidious_instance.is_none()
            && let Err(e) = self.set_instance().await
        {
            self.selected_api = ApiBackend::Local;
            emit_msg!(error, format!("{e} Falling back to the local API."));
        }

        Ok(())
    }

    pub async fn set_instance(&mut self) -> Result<()> {
        if let Some(invidious_instances) = &self.invidious_instances {
            if invidious_instances.is_empty() {
                return Err(anyhow::anyhow!("No Invidious instance available."));
            }

            self.invidious_instance = Some(Instance::new(invidious_instances));
        } else {
            emit_msg!(perm, "Fetching instances");

            if let Ok(instances) = utils::fetch_invidious_instances().await {
                emit_msg!();
                self.invidious_instances = Some(instances);
                Box::pin(self.set_instance()).await?;
            }

            return Err(anyhow::anyhow!("Failed to fetch instances"));
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

async fn get_more_videos(
    mut instance: Box<dyn Api>,
    id: &str,
    present: HashSet<String>,
) -> Result<()> {
    match instance.get_more_videos(id, present).await {
        Ok(feed) => {
            if feed.videos.is_empty() {
                emit_msg!(warning, "There are no videos to load");
            } else {
                emit_msg!();
                TX.send(ClientRequest::UpdateChannel(feed))?;
            }
        }
        Err(e) => emit_msg!(error, &e.to_string()),
    }

    Ok(())
}

async fn fetch_formats(
    instance: Box<dyn Api>,
    title: String,
    video_id: String,
    play_selected: bool,
) -> Result<()> {
    emit_msg!(perm, "Fetching formats");
    let video_info = instance.get_video_formats(&video_id).await;

    let formats = match video_info {
        Ok(video_info) => Formats::new(title, video_id, video_info),
        Err(e) => {
            emit_msg!(error, e.to_string());
            return Ok(());
        }
    };

    if play_selected {
        player::play_from_formats(instance, formats).await?;
    } else {
        emit_msg!();
        TX.send(ClientRequest::EnterFormatSelection(Box::new(formats)))?;
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
