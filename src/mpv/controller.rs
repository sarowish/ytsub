use super::MpvSession;
use crate::{channel::VideoMetadata, mpv::ipc::MpvNotification};
use anyhow::Result;
use serde_json::Value;
use std::time::Duration;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::MissedTickBehavior,
};

enum PlayerCommand {
    Play {
        metadata: VideoMetadata,
        source: String,
    },
    Toggle,
    Seek(i32),
    Stop,
}

#[derive(Clone)]
pub struct PlaybackState {
    pub metadata: Option<VideoMetadata>,
    pub phase: PlaybackPhase,
    pub elapsed: Option<u64>,
    pub duration: Option<u64>,
}

impl PlaybackState {
    fn idle() -> Self {
        Self {
            metadata: None,
            phase: PlaybackPhase::Idle,
            elapsed: None,
            duration: None,
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

    async fn ensure_session(&mut self) -> Result<&mut MpvSession> {
        if self.session.is_none() {
            self.session = Some(MpvSession::new().await?);
        }

        Ok(self.session.as_mut().unwrap())
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
    }

    async fn stop_playback(&mut self) -> Result<()> {
        let Some(session) = &self.session else {
            return Ok(());
        };

        session.ipc.call(serde_json::json!(["stop"])).await?;
        self.publish_state();

        Ok(())
    }

    async fn handle_command(&mut self, command: PlayerCommand) -> Result<()> {
        let result: Result<()> = match command {
            PlayerCommand::Play { metadata, source } => {
                self.state.metadata = Some(metadata);
                self.state.phase = PlaybackPhase::Loading;
                self.state.elapsed = None;
                self.state.duration = None;
                self.publish_state();

                async {
                    let session = self.ensure_session().await?;
                    let entry_id = session.ipc.load_file(&source).await?;

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
            PlayerCommand::Stop => self.stop_playback().await,
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
                Some("property-change") if self.notification_is_for_current() => {
                    match event.get("name").and_then(Value::as_str) {
                        Some("pause") => {
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
                        Some("duration") => {
                            self.state.duration = event
                                .get("data")
                                .and_then(Value::as_f64)
                                .map(|seconds| seconds.round() as u64);
                        }
                        _ => {}
                    }
                }
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
                            .unwrap_or("mpv failed to play audio");

                        self.state.phase = PlaybackPhase::Error(error.to_owned());
                    }
                }
                _ => return Ok(()),
            },
            MpvNotification::Disconnected(error) => {
                self.disconnect_session();
                self.state.phase = PlaybackPhase::Error(error);
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

    pub fn play(&self, metadata: VideoMetadata, source: String) -> Result<()> {
        self.send(PlayerCommand::Play { metadata, source })
    }

    pub fn toggle(&self) -> Result<()> {
        self.send(PlayerCommand::Toggle)
    }

    pub fn seek_relative(&self, seconds: i32) -> Result<()> {
        self.send(PlayerCommand::Seek(seconds))
    }

    pub fn stop(&self) -> Result<()> {
        self.send(PlayerCommand::Stop)
    }
}
