use crate::mpv::{PlaybackEndReason, PlaybackUpdateCause};
use std::time::{Duration, Instant};

const MIN_PROGRESS_SECONDS: u64 = 5;
const SAVE_INTERVAL: Duration = Duration::from_secs(5);

pub struct ProgressSave {
    pub video_id: String,
    pub position: u64,
}

#[derive(Default)]
pub struct ProgressActions {
    pub previous_save: Option<ProgressSave>,
    pub position: Option<u64>,
    pub save_position: Option<u64>,
    pub mark_watched: bool,
}

#[derive(Default)]
pub struct ProgressTracker {
    video_id: Option<String>,
    position: Option<u64>,
    last_saved_position: Option<u64>,
    last_saved_at: Option<Instant>,
}

impl ProgressTracker {
    pub fn handle_update(
        &mut self,
        video_id: &str,
        elapsed: Option<u64>,
        duration: Option<u64>,
        cause: &PlaybackUpdateCause,
        watched_threshold: u8,
    ) -> ProgressActions {
        let previous_save = self.switch_video(video_id);

        let reached_eof = matches!(cause, PlaybackUpdateCause::Ended(PlaybackEndReason::Eof));
        let checkpoint = matches!(
            cause,
            PlaybackUpdateCause::Paused
                | PlaybackUpdateCause::Seeked
                | PlaybackUpdateCause::Released
                | PlaybackUpdateCause::Replaced
                | PlaybackUpdateCause::Ended(_)
                | PlaybackUpdateCause::Failed
        );

        let position = if reached_eof {
            None
        } else if elapsed.is_some() {
            elapsed
        } else if checkpoint {
            self.position
        } else {
            None
        };
        let mark_watched = should_mark_watched(cause, position, duration, watched_threshold);

        let Some(position) = position else {
            return ProgressActions {
                previous_save,
                mark_watched,
                ..ProgressActions::default()
            };
        };
        self.position = Some(position);

        let save_position = self
            .should_save(position, checkpoint, Instant::now())
            .then_some(position);

        ProgressActions {
            previous_save,
            position: Some(position),
            save_position,
            mark_watched,
        }
    }

    pub fn mark_saved(&mut self, position: u64) {
        self.mark_saved_at(position, Instant::now());
    }

    fn mark_saved_at(&mut self, position: u64, saved_at: Instant) {
        self.last_saved_position = Some(position);
        self.last_saved_at = Some(saved_at);
    }

    fn switch_video(&mut self, video_id: &str) -> Option<ProgressSave> {
        if self.video_id.as_deref() == Some(video_id) {
            return None;
        }

        let previous_save = self.unsaved_progress();

        *self = Self {
            video_id: Some(video_id.to_owned()),
            ..Self::default()
        };

        previous_save
    }

    fn unsaved_progress(&mut self) -> Option<ProgressSave> {
        let position = self.position?;

        if !Self::is_meaningful(position) || self.last_saved_position == Some(position) {
            return None;
        }

        Some(ProgressSave {
            video_id: self.video_id.take()?,
            position,
        })
    }

    fn should_save(&self, position: u64, checkpoint: bool, observed_at: Instant) -> bool {
        if !checkpoint && !Self::is_meaningful(position) {
            return false;
        }

        if self.last_saved_position == Some(position) {
            return false;
        }

        let interval_elapsed = self.last_saved_at.is_none_or(|last_saved_at| {
            observed_at.saturating_duration_since(last_saved_at) >= SAVE_INTERVAL
        });

        checkpoint || interval_elapsed
    }

    fn is_meaningful(position: u64) -> bool {
        position >= MIN_PROGRESS_SECONDS
    }
}
fn should_mark_watched(
    cause: &PlaybackUpdateCause,
    position: Option<u64>,
    duration: Option<u64>,
    threshold_percent: u8,
) -> bool {
    match cause {
        PlaybackUpdateCause::Ended(PlaybackEndReason::Eof) => true,
        PlaybackUpdateCause::Released
        | PlaybackUpdateCause::Replaced
        | PlaybackUpdateCause::Ended(PlaybackEndReason::Stop | PlaybackEndReason::Quit) => {
            position.zip(duration).is_some_and(|(position, duration)| {
                reached_threshold(position, duration, threshold_percent)
            })
        }
        _ => false,
    }
}

pub fn reached_threshold(value: u64, duration: u64, threshold_percent: u8) -> bool {
    duration > 0 && u128::from(value) * 100 >= u128::from(duration) * u128::from(threshold_percent)
}
