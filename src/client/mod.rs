use std::collections::HashSet;

use crate::{
    CONFIG,
    api::{Api, ApiBackend, ChannelFeed, invidious::Instance, local::Local},
    channel::{ChannelTab, RefreshState, VideoMetadata},
    message::MessageType,
    mpv::{PlayerHandle, VideoRequest, VideoSource},
    player::{copy_link, open_in_invidious, open_in_youtube, play_from_formats, youtube_watch_url},
    ro_cell::RoCell,
    stream_formats::Formats,
    thumbnail::{Thumbnail, protocols::GraphicsProtocol},
    utils,
};
use anyhow::{Result, bail};
use feeds::{
    get_more_videos, get_video_title, import_channels, refresh_channels, subscribe_to_channel,
};
use media::{fetch_formats, get_thumbnail};
use tokio::{
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot::Sender,
        watch,
    },
    time::sleep,
};
use tokio_util::sync::CancellationToken;

mod feeds;
mod media;

pub enum FormatAction {
    Select,
    PlayVideo,
    PlayAudio,
}

struct ThumbnailRequest {
    instance: Box<dyn Api>,
    protocol: GraphicsProtocol,
    video_id: String,
}

impl Clone for ThumbnailRequest {
    fn clone(&self) -> Self {
        Self {
            instance: dyn_clone::clone_box(self.instance.as_ref()),
            protocol: self.protocol,
            video_id: self.video_id.clone(),
        }
    }
}

pub enum IoEvent {
    SubscribeToChannel(String),
    ImportChannels(Vec<String>),
    RefreshChannels(Vec<String>),
    LoadMoreVideos(String, ChannelTab, HashSet<String>, bool),
    GetVideoTitle(String),
    GetThumbnail(GraphicsProtocol, String),
    FetchFormats(VideoMetadata, FormatAction),
    PlayFromFormats(Box<Formats>),
    PlayUsingYtdlp(VideoMetadata),
    PlayAudioUsingYtdlp(VideoMetadata),
    TogglePlayback,
    SeekPlayback(i32),
    AdjustVolume(i8),
    ToggleMute,
    StopPlayback,
    ReleaseVideo,
    CopyLink(String, ApiBackend),
    OpenInBrowser(String, ApiBackend),
    ClearMessage(CancellationToken, u64),
    SwitchApi,
}

pub enum ClientRequest {
    SetRefreshState(String, RefreshState),
    SetImportState(String, RefreshState),
    AddChannel(ChannelFeed),
    CheckChannel(String, Sender<bool>),
    FinalizeImport(bool),
    UpdateChannel(ChannelFeed),
    UpdateTitle(String, String),
    SetThumbnail(String, Option<Thumbnail>),
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
    player: PlayerHandle,
    pub invidious_instances: Option<Vec<String>>,
    pub invidious_instance: Option<Instance>,
    local_api: Local,
    pub selected_api: ApiBackend,
}

impl Client {
    pub async fn new(rx: UnboundedReceiver<IoEvent>, player: PlayerHandle) -> Result<Self> {
        let mut client = Self {
            rx,
            player,
            invidious_instances: utils::read_instances().ok(),
            invidious_instance: None,
            local_api: Local::new()?,
            selected_api: CONFIG.api,
        };

        if matches!(client.selected_api, ApiBackend::Invidious) {
            client.set_instance().await?;
        }

        Ok(client)
    }

    pub async fn run(&mut self) -> Result<()> {
        let (thumbnail_tx, thumbnail_rx) = watch::channel(None);

        tokio::spawn(thumbnail_worker(thumbnail_rx));

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
                IoEvent::LoadMoreVideos(id, tab, present_videos, load_all) => {
                    let instance = self.instance();
                    tokio::spawn(async move {
                        get_more_videos(instance, &id, tab, present_videos, load_all).await
                    });
                }
                IoEvent::GetVideoTitle(video_id) => {
                    let local = self.local_api.clone();
                    tokio::spawn(async move { get_video_title(local, &video_id).await });
                }
                IoEvent::GetThumbnail(protocol, video_id) => {
                    thumbnail_tx.send_replace(Some(ThumbnailRequest {
                        instance: self.instance(),
                        protocol,
                        video_id,
                    }));
                }
                IoEvent::FetchFormats(metadata, action) => {
                    let instance = self.instance();
                    let player = self.player.clone();

                    tokio::spawn(
                        async move { fetch_formats(instance, player, metadata, action).await },
                    );
                }
                IoEvent::PlayFromFormats(formats) => {
                    let instance = self.instance();
                    let player = self.player.clone();

                    tokio::spawn(
                        async move { play_from_formats(instance, player, *formats).await },
                    );
                }
                IoEvent::PlayUsingYtdlp(metadata) => {
                    let url = youtube_watch_url(&metadata.video_id);

                    let request = VideoRequest {
                        metadata,
                        source: VideoSource::YtDlp(url),
                    };

                    if let Err(error) = self.player.play_video(request) {
                        emit_msg!(error, error.to_string());
                    }
                }
                IoEvent::PlayAudioUsingYtdlp(metadata) => {
                    let source = youtube_watch_url(&metadata.video_id);

                    if let Err(error) = self.player.play_audio(metadata, source) {
                        emit_msg!(error, error.to_string());
                    }
                }
                IoEvent::TogglePlayback => {
                    if let Err(error) = self.player.toggle() {
                        emit_msg!(error, error.to_string());
                    }
                }
                IoEvent::SeekPlayback(seconds) => {
                    if let Err(error) = self.player.seek_relative(seconds) {
                        emit_msg!(error, error.to_string());
                    }
                }
                IoEvent::AdjustVolume(value) => {
                    if let Err(error) = self.player.adjust_volume(value) {
                        emit_msg!(error, error.to_string());
                    }
                }
                IoEvent::ToggleMute => {
                    if let Err(error) = self.player.toggle_mute() {
                        emit_msg!(error, error.to_string());
                    }
                }
                IoEvent::StopPlayback => {
                    if let Err(error) = self.player.stop() {
                        emit_msg!(error, error.to_string());
                    }
                }
                IoEvent::ReleaseVideo => {
                    if let Err(error) = self.player.release_video() {
                        emit_msg!(error, error.to_string());
                    }
                }
                IoEvent::CopyLink(url_component, api) => {
                    copy_link(self, &url_component, api).await?;
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
                bail!("No Invidious instance available.");
            }

            self.invidious_instance = Some(Instance::new(invidious_instances)?);
        } else {
            emit_msg!(perm, "Fetching instances");

            if let Ok(instances) = utils::fetch_invidious_instances().await {
                emit_msg!();
                self.invidious_instances = Some(instances);
                Box::pin(self.set_instance()).await?;
            } else {
                bail!("Failed to fetch instances.");
            }
        }

        Ok(())
    }
}

async fn thumbnail_worker(mut rx: watch::Receiver<Option<ThumbnailRequest>>) {
    while rx.changed().await.is_ok() {
        let Some(request) = rx.borrow_and_update().clone() else {
            continue;
        };

        let video_id = request.video_id.clone();

        let data = get_thumbnail(request.instance, request.protocol, &video_id).await;

        if !rx.has_changed().unwrap_or(true)
            && TX
                .send(ClientRequest::SetThumbnail(video_id, data.ok()))
                .is_err()
        {
            return;
        }
    }
}

async fn clear_message(token: CancellationToken, duration: u64) -> Result<()> {
    tokio::select! {
        () = token.cancelled() => {}
        () = sleep(std::time::Duration::from_secs(duration)) => emit_msg!(),

    }

    Ok(())
}
