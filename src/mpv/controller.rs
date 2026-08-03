use super::{
    MpvLaunch, MpvSession, PlaybackItem, PlaybackKind, VideoRequest, VideoSource,
    ipc::MpvNotification,
};
use crate::channel::VideoMetadata;
use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::MissedTickBehavior,
};

enum PlayRequest {
    Audio {
        metadata: VideoMetadata,
        source: String,
    },
    Video(VideoRequest),
}

impl PlayRequest {
    fn kind(&self) -> PlaybackKind {
        match self {
            PlayRequest::Audio { .. } => PlaybackKind::Audio,
            PlayRequest::Video(_) => PlaybackKind::Video,
        }
    }

    fn metadata(&self) -> &VideoMetadata {
        match self {
            PlayRequest::Audio { metadata, .. } => metadata,
            PlayRequest::Video(request) => &request.metadata,
        }
    }

    fn source(&self) -> &str {
        match self {
            PlayRequest::Audio { source, .. } => source,
            PlayRequest::Video(request) => match &request.source {
                VideoSource::YtDlp(source) => source,
                VideoSource::Direct { video_url, .. } => video_url,
            },
        }
    }
}

enum PlayerCommand {
    Play(PlayRequest),
    Toggle,
    Seek(i32),
    AdjustVolume(i8),
    ToggleMute,
    Stop,
    ReleaseVideo,
}

#[derive(Clone)]
pub struct PlaybackState {
    pub item: Option<PlaybackItem>,
    pub phase: PlaybackPhase,
    pub elapsed: Option<u64>,
    pub duration: Option<u64>,
    pub volume: Option<u64>,
    pub muted: Option<bool>,
}

impl PlaybackState {
    fn idle() -> Self {
        Self {
            item: None,
            phase: PlaybackPhase::Idle,
            elapsed: None,
            duration: None,
            volume: None,
            muted: None,
        }
    }

    fn is_loaded(&self) -> bool {
        matches!(self.phase, PlaybackPhase::Playing | PlaybackPhase::Paused)
    }

    fn is_playing(&self) -> bool {
        matches!(self.phase, PlaybackPhase::Playing)
    }
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self::idle()
    }
}

#[derive(Clone)]
pub enum PlaybackPhase {
    Idle,
    Loading,
    Playing,
    Paused,
    Error(String),
}

struct PlayerController {
    session: Option<MpvSession>,
    state: PlaybackState,
    requested_entry_id: Option<i64>,
    event_entry_id: Option<i64>,
    command_rx: mpsc::Receiver<PlayerCommand>,
    state_tx: watch::Sender<PlaybackState>,
}

impl PlayerController {
    fn new(
        commands: mpsc::Receiver<PlayerCommand>,
        state_tx: watch::Sender<PlaybackState>,
    ) -> Self {
        Self {
            session: None,
            state: PlaybackState::idle(),
            requested_entry_id: None,
            event_entry_id: None,
            command_rx: commands,
            state_tx,
        }
    }

    async fn run(&mut self) -> Result<()> {
        let mut progress_tick = tokio::time::interval(Duration::from_millis(250));
        progress_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            if let Some(session) = self.session.as_mut() {
                tokio::select! {
                    _ = progress_tick.tick(), if self.state.is_playing() => {
                        if let Ok(elapsed) = session.ipc.time_pos().await
                            && self.state.elapsed != elapsed
                        {
                            self.state.elapsed = elapsed;
                            self.publish_state();
                        }

                    }
                    command = self.command_rx.recv() => {
                        let Some(command) = command else {
                            return Ok(());
                        };

                        self.handle_command(command).await?;
                    }

                    notification = session.notifications.recv() => {
                        self.handle_notification(notification).await?;
                    }
                }
            } else {
                let Some(command) = self.command_rx.recv().await else {
                    return Ok(());
                };

                self.handle_command(command).await?;
            }
        }
    }

    async fn close_session(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = session.ipc.call(serde_json::json!(["quit"])).await;
        }

        self.requested_entry_id = None;
        self.event_entry_id = None;
    }

    async fn ensure_session(&mut self, request: &PlayRequest) -> Result<&mut MpvSession> {
        let reuse_audio = matches!(request, PlayRequest::Audio { .. })
            && self
                .session
                .as_ref()
                .is_some_and(|session| session.kind == PlaybackKind::Audio);

        if !reuse_audio {
            self.close_session().await;

            let launch = match request {
                PlayRequest::Audio { .. } => MpvLaunch {
                    kind: PlaybackKind::Audio,
                    uses_ytdlp: true,
                    extra_args: Vec::new(),
                },
                PlayRequest::Video(request) => MpvLaunch::from_video(request),
            };

            self.session = Some(MpvSession::new(launch).await?);
        }

        self.session.as_mut().context("mpv session was not created")
    }

    fn notification_is_for_current(&self) -> bool {
        matches!(
            (self.requested_entry_id, self.event_entry_id),
            (Some(requested), Some(event)) if requested == event
        )
    }

    fn set_idle(&mut self) {
        self.requested_entry_id = None;
        self.event_entry_id = None;
        self.state.phase = PlaybackPhase::Idle;
        self.state.elapsed = None;
        self.state.duration = None;
    }

    fn disconnect_session(&mut self) {
        self.session = None;
        self.set_idle();
        self.state.volume = None;
        self.state.muted = None;
    }

    async fn stop_playback(&mut self) -> Result<()> {
        let Some(session) = &self.session else {
            return Ok(());
        };

        match session.kind {
            PlaybackKind::Video => {
                session.ipc.call(serde_json::json!(["quit"])).await?;
            }
            PlaybackKind::Audio => {
                session.ipc.call(serde_json::json!(["stop"])).await?;
            }
        }

        Ok(())
    }

    fn release_video(&mut self) {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.kind == PlaybackKind::Video)
        {
            self.disconnect_session();
            self.publish_state();
        }
    }

    async fn handle_command(&mut self, command: PlayerCommand) -> Result<()> {
        let result: Result<()> = match command {
            PlayerCommand::Play(request) => {
                self.state.item = Some(PlaybackItem {
                    metadata: request.metadata().clone(),
                    kind: request.kind(),
                });
                self.state.phase = PlaybackPhase::Loading;
                self.state.elapsed = None;
                self.state.duration = None;
                self.publish_state();

                async {
                    let session = self.ensure_session(&request).await?;
                    let entry_id = session.ipc.load_file(request.source()).await?;

                    session
                        .ipc
                        .call(serde_json::json!(["set_property", "pause", false]))
                        .await?;

                    self.requested_entry_id = Some(entry_id);

                    Ok(())
                }
                .await
            }
            PlayerCommand::Toggle => {
                if self.state.is_loaded()
                    && let Some(session) = &self.session
                {
                    session
                        .ipc
                        .call(serde_json::json!(["cycle", "pause"]))
                        .await
                        .map(|_| ())
                } else {
                    Ok(())
                }
            }
            PlayerCommand::Seek(sec) => {
                if self.state.is_loaded()
                    && let Some(session) = &self.session
                {
                    session
                        .ipc
                        .call(serde_json::json!(["seek", sec, "relative"]))
                        .await
                        .map(|_| ())
                } else {
                    Ok(())
                }
            }
            PlayerCommand::AdjustVolume(value) => {
                if let Some(session) = &self.session {
                    session
                        .ipc
                        .call(serde_json::json!(["add", "volume", value]))
                        .await
                        .map(|_| ())
                } else {
                    Ok(())
                }
            }
            PlayerCommand::ToggleMute => {
                if let Some(session) = &self.session {
                    session
                        .ipc
                        .call(serde_json::json!(["cycle", "mute"]))
                        .await
                        .map(|_| ())
                } else {
                    Ok(())
                }
            }
            PlayerCommand::Stop => self.stop_playback().await,
            PlayerCommand::ReleaseVideo => {
                self.release_video();
                Ok(())
            }
        };

        if let Err(error) = result {
            self.disconnect_session();
            self.state.phase = PlaybackPhase::Error(error.to_string());
            self.publish_state();
        }

        Ok(())
    }

    async fn handle_notification(&mut self, notification: Option<MpvNotification>) -> Result<()> {
        let Some(notification) = notification else {
            self.disconnect_session();
            self.publish_state();
            return Ok(());
        };

        match notification {
            MpvNotification::Event(event) => match event.get("event").and_then(Value::as_str) {
                Some("start-file") => {
                    self.event_entry_id = event.get("playlist_entry_id").and_then(Value::as_i64);

                    return Ok(());
                }
                Some("property-change") => match event.get("name").and_then(Value::as_str) {
                    Some("pause") if self.notification_is_for_current() => {
                        let Some(paused) = event.get("data").and_then(Value::as_bool) else {
                            return Ok(());
                        };

                        if matches!(
                            &self.state.phase,
                            PlaybackPhase::Playing | PlaybackPhase::Paused
                        ) {
                            self.state.phase = if paused {
                                PlaybackPhase::Paused
                            } else {
                                PlaybackPhase::Playing
                            };
                        }
                    }
                    Some("duration") if self.notification_is_for_current() => {
                        self.state.duration = event
                            .get("data")
                            .and_then(Value::as_f64)
                            .map(|seconds| seconds.round() as u64);
                    }
                    Some("volume") => {
                        self.state.volume = event
                            .get("data")
                            .and_then(Value::as_f64)
                            .map(|seconds| seconds.round() as u64);
                    }
                    Some("mute") => self.state.muted = event.get("data").and_then(Value::as_bool),
                    _ => {}
                },
                Some("seek") if self.notification_is_for_current() => {
                    let Some(session) = &self.session else {
                        return Ok(());
                    };

                    if let Ok(elapsed) = session.ipc.time_pos().await
                        && self.state.elapsed != elapsed
                    {
                        self.state.elapsed = elapsed;
                    } else {
                        return Ok(());
                    }
                }
                Some("file-loaded") if self.notification_is_for_current() => {
                    self.state.phase = PlaybackPhase::Playing;
                }
                Some("end-file") => {
                    let Some(entry_id) = event.get("playlist_entry_id").and_then(Value::as_i64)
                    else {
                        return Ok(());
                    };

                    if self.event_entry_id == Some(entry_id) {
                        self.event_entry_id = None;
                    }

                    if self.requested_entry_id != Some(entry_id) {
                        return Ok(());
                    }

                    let reason = event.get("reason").and_then(Value::as_str);

                    self.set_idle();

                    if reason == Some("error") {
                        let error = event
                            .get("file_error")
                            .and_then(Value::as_str)
                            .unwrap_or("mpv failed to play media");

                        self.state.phase = PlaybackPhase::Error(error.to_owned());
                    }
                }
                _ => return Ok(()),
            },
            MpvNotification::Disconnected(error) => {
                let previous_phase = self.state.phase.clone();

                self.disconnect_session();

                self.state.phase = match previous_phase {
                    PlaybackPhase::Loading | PlaybackPhase::Playing | PlaybackPhase::Paused => {
                        PlaybackPhase::Error(error)
                    }
                    _ => previous_phase,
                }
            }
        }

        self.publish_state();

        Ok(())
    }

    fn publish_state(&self) {
        self.state_tx.send_replace(self.state.clone());
    }
}

#[derive(Clone)]
pub struct PlayerHandle {
    command_tx: mpsc::Sender<PlayerCommand>,
}

impl PlayerHandle {
    pub fn spawn() -> (Self, watch::Receiver<PlaybackState>, JoinHandle<Result<()>>) {
        let (command_tx, command_rx) = mpsc::channel(32);

        let initial_state = PlaybackState::idle();
        let (state_tx, state_rx) = watch::channel(initial_state);

        let mut controller = PlayerController::new(command_rx, state_tx);

        let task = tokio::spawn(async move { controller.run().await });

        (Self { command_tx }, state_rx, task)
    }

    fn send(&self, command: PlayerCommand) -> Result<()> {
        self.command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!("failed to send player command: {error}"))
    }

    pub fn play_audio(&self, metadata: VideoMetadata, source: String) -> Result<()> {
        self.send(PlayerCommand::Play(PlayRequest::Audio { metadata, source }))
    }

    pub fn play_video(&self, request: VideoRequest) -> Result<()> {
        self.send(PlayerCommand::Play(PlayRequest::Video(request)))
    }

    pub fn toggle(&self) -> Result<()> {
        self.send(PlayerCommand::Toggle)
    }

    pub fn seek_relative(&self, seconds: i32) -> Result<()> {
        self.send(PlayerCommand::Seek(seconds))
    }

    pub fn adjust_volume(&self, value: i8) -> Result<()> {
        self.send(PlayerCommand::AdjustVolume(value))
    }

    pub fn toggle_mute(&self) -> Result<()> {
        self.send(PlayerCommand::ToggleMute)
    }

    pub fn stop(&self) -> Result<()> {
        self.send(PlayerCommand::Stop)
    }

    pub fn release_video(&self) -> Result<()> {
        self.send(PlayerCommand::ReleaseVideo)
    }
}
