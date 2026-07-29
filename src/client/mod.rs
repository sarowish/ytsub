use crate::{
    CONFIG, IoEvent,
    api::{Api, ApiBackend, ChannelFeed, invidious::Instance, local::Local},
    channel::RefreshState,
    message::MessageType,
    player::{copy_link, open_in_invidious, open_in_youtube, play_from_formats, play_using_ytdlp},
    ro_cell::RoCell,
    stream_formats::Formats,
    thumbnail::Thumbnail,
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
    },
    task::AbortHandle,
    time::sleep,
};
use tokio_util::sync::CancellationToken;

mod feeds;
mod media;

pub enum ClientRequest {
    SetRefreshState(String, RefreshState),
    SetImportState(String, RefreshState),
    AddChannel(ChannelFeed),
    CheckChannel(String, Sender<bool>),
    FinalizeImport(bool),
    UpdateChannel(ChannelFeed),
    UpdateTitle(String, String),
    SetThumbnail(Option<Thumbnail>),
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
            selected_api: CONFIG.api,
        };

        if matches!(client.selected_api, ApiBackend::Invidious) {
            client.set_instance().await?;
        }

        Ok(client)
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut thumbnail_handle: Option<AbortHandle> = None;

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
                    let instance = self.instance();

                    if let Some(handle) = thumbnail_handle {
                        handle.abort();
                    }

                    thumbnail_handle = Some(
                        tokio::spawn(async move {
                            let data = get_thumbnail(instance, protocol, &video_id).await;
                            TX.send(ClientRequest::SetThumbnail(data.ok()))
                        })
                        .abort_handle(),
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

            self.invidious_instance = Some(Instance::new(invidious_instances));
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

async fn clear_message(token: CancellationToken, duration: u64) -> Result<()> {
    tokio::select! {
        () = token.cancelled() => {}
        () = sleep(std::time::Duration::from_secs(duration)) => emit_msg!(),

    }

    Ok(())
}
