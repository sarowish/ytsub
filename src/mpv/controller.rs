use super::{MpvLaunch, MpvSession, PlaybackKind, VideoRequest, ipc::MpvNotification};
use crate::video::{PlaybackSpec, VideoMetadata};
use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;
use tokio::{sync::mpsc, task::JoinHandle, time::MissedTickBehavior};

enum PlayRequest {
    Audio { spec: PlaybackSpec, source: String },
    Video(VideoRequest),
}

impl PlayRequest {
    fn spec(&self) -> &PlaybackSpec {
        match self {
            PlayRequest::Audio { spec, .. } => spec,
            PlayRequest::Video(request) => &request.spec,
        }
    }

    fn source(&self) -> &str {
        match self {
            PlayRequest::Audio { source, .. } => source,
            PlayRequest::Video(request) => request.source(),
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
    pub metadata: Option<VideoMetadata>,
    pub phase: PlaybackPhase,
    pub elapsed: Option<u64>,
    pub duration: Option<u64>,
    pub volume: Option<u64>,
    pub muted: Option<bool>,
}

impl PlaybackState {
    fn idle() -> Self {
        Self {
            metadata: None,
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

pub struct PlaybackUpdate {
    pub state: PlaybackState,
    pub cause: PlaybackUpdateCause,
}

pub enum PlaybackUpdateCause {
    Loading,
    Loaded,
    Progress,
    Paused,
    Resumed,
    Seeked,
    Released,
    Replaced,
    Ended(PlaybackEndReason),
    Failed,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackEndReason {
    Eof,
    Stop,
    Quit,
    Redirect,
    Error,
    Unknown,
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
    update_tx: mpsc::Sender<PlaybackUpdate>,
}

impl PlayerController {
    fn new(
        commands: mpsc::Receiver<PlayerCommand>,
        update_tx: mpsc::Sender<PlaybackUpdate>,
    ) -> Self {
        Self {
            session: None,
            state: PlaybackState::idle(),
            requested_entry_id: None,
            event_entry_id: None,
            command_rx: commands,
            update_tx,
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
                            self.publish_state(PlaybackUpdateCause::Progress).await;
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

    async fn release_video(&mut self) {
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.kind == PlaybackKind::Video)
        {
            let mut released_state = self.state.clone();
            released_state.phase = PlaybackPhase::Idle;

            self.disconnect_session();

            self.publish_update(released_state, PlaybackUpdateCause::Released)
                .await;
        }
    }

    async fn handle_command(&mut self, command: PlayerCommand) -> Result<()> {
        let result: Result<()> = match command {
            PlayerCommand::Play(request) => {
                if self.requested_entry_id.is_some() {
                    self.publish_state(PlaybackUpdateCause::Replaced).await;
                }

                self.state.metadata = Some(request.spec().metadata.clone());
                self.state.phase = PlaybackPhase::Loading;
                self.state.elapsed = None;
                self.state.duration = None;
                self.publish_state(PlaybackUpdateCause::Loading).await;

                async {
                    let session = self.ensure_session(&request).await?;
                    let entry_id = session
                        .ipc
                        .load_file(request.source(), request.spec().start_position)
                        .await?;

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
                self.release_video().await;
                Ok(())
            }
        };

        if let Err(error) = result {
            self.disconnect_session();
            self.state.phase = PlaybackPhase::Error(error.to_string());
            self.publish_state(PlaybackUpdateCause::Ended(PlaybackEndReason::Error))
                .await;
        }

        Ok(())
    }

    async fn handle_notification(&mut self, notification: Option<MpvNotification>) -> Result<()> {
        let Some(notification) = notification else {
            self.disconnect_session();
            self.publish_state(PlaybackUpdateCause::Ended(PlaybackEndReason::Unknown))
                .await;
            return Ok(());
        };

        let mut cause = PlaybackUpdateCause::Other;

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
                            (self.state.phase, cause) = if paused {
                                (PlaybackPhase::Paused, PlaybackUpdateCause::Paused)
                            } else {
                                (PlaybackPhase::Playing, PlaybackUpdateCause::Resumed)
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
                    _ => return Ok(()),
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

                    cause = PlaybackUpdateCause::Seeked;
                }
                Some("file-loaded") if self.notification_is_for_current() => {
                    self.state.phase = PlaybackPhase::Playing;
                    cause = PlaybackUpdateCause::Loaded;
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

                    let reason = match event.get("reason").and_then(Value::as_str) {
                        Some("eof") => PlaybackEndReason::Eof,
                        Some("stop") => PlaybackEndReason::Stop,
                        Some("quit") => PlaybackEndReason::Quit,
                        Some("error") => PlaybackEndReason::Error,
                        Some("redirect") => PlaybackEndReason::Redirect,
                        _ => PlaybackEndReason::Unknown,
                    };

                    self.state.phase = PlaybackPhase::Idle;

                    if matches!(reason, PlaybackEndReason::Error) {
                        let error = event
                            .get("file_error")
                            .and_then(Value::as_str)
                            .unwrap_or("mpv failed to play media");

                        self.state.phase = PlaybackPhase::Error(error.to_owned());
                    }

                    self.publish_state(PlaybackUpdateCause::Ended(reason)).await;
                    self.set_idle();
                    return Ok(());
                }
                _ => return Ok(()),
            },
            MpvNotification::Disconnected(error) => {
                let previous_phase = self.state.phase.clone();
                self.disconnect_session();

                self.state.phase = match previous_phase {
                    PlaybackPhase::Loading | PlaybackPhase::Playing | PlaybackPhase::Paused => {
                        cause = PlaybackUpdateCause::Failed;
                        PlaybackPhase::Error(error)
                    }
                    _ => previous_phase,
                };
            }
        }

        self.publish_state(cause).await;

        Ok(())
    }

    async fn publish_state(&self, cause: PlaybackUpdateCause) {
        self.publish_update(self.state.clone(), cause).await
    }

    async fn publish_update(&self, state: PlaybackState, cause: PlaybackUpdateCause) {
        let _ = self.update_tx.send(PlaybackUpdate { state, cause }).await;
    }
}

#[derive(Clone)]
pub struct PlayerHandle {
    command_tx: mpsc::Sender<PlayerCommand>,
}

impl PlayerHandle {
    pub fn spawn() -> (Self, mpsc::Receiver<PlaybackUpdate>, JoinHandle<Result<()>>) {
        let (command_tx, command_rx) = mpsc::channel(32);
        let (update_tx, update_rx) = mpsc::channel(32);
        let mut controller = PlayerController::new(command_rx, update_tx);
        let task = tokio::spawn(async move { controller.run().await });

        (Self { command_tx }, update_rx, task)
    }

    fn send(&self, command: PlayerCommand) -> Result<()> {
        self.command_tx
            .try_send(command)
            .map_err(|error| anyhow::anyhow!("failed to send player command: {error}"))
    }

    pub fn play_audio(&self, spec: PlaybackSpec, source: String) -> Result<()> {
        self.send(PlayerCommand::Play(PlayRequest::Audio { spec, source }))
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
