use crate::api::{ApiBackend, ChannelFeed};
use crate::channel::{Channel, ChannelTab, HideVideos, RefreshState, tabs_to_be_loaded};
use crate::client::FormatAction;
use crate::emulator::Emulator;
use crate::help::HelpWindowState;
use crate::import::{self, ImportItem};
use crate::input::{Input, InputChange, InputMode};
use crate::list::{ListItem, Selectable, SelectionItem, SelectionList, StatefulList};
use crate::message::Message;
use crate::mpv::{PlaybackPhase, PlaybackState, PlaybackUpdate};
use crate::progress::{ProgressActions, ProgressTracker};
use crate::search::{Search, SearchDirection, SearchUpdate};
use crate::stream_formats::Formats;
use crate::thumbnail::Thumbnail;
use crate::video::{FetchedVideo, PlaybackSpec, Video, VideoListItem, VideoMetadata};
use crate::{CLAP_ARGS, CONFIG, IoEvent, database, utils};
use anyhow::{Context, Result};
use ratatui::widgets::{ListState, TableState};
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use tokio::sync::mpsc::UnboundedSender;

pub struct App {
    pub emulator: Option<Emulator>,
    pub channels: StatefulList<Channel, ListState>,
    pub tabs: Tabs,
    pub tags: SelectionList<String>,
    pub selected: Selected,
    pub mode: Mode,
    pub conn: Connection,
    pub thumbnail: Option<Thumbnail>,
    pub message: Message,
    pub playback_state: PlaybackState,
    progress_tracker: ProgressTracker,
    pub input: Input,
    pub input_mode: InputMode,
    pub prev_input_mode: InputMode,
    pub help_window_state: HelpWindowState,
    pub import_state: SelectionList<ImportItem>,
    new_video_ids: HashSet<String>,
    channels_with_new_videos: HashSet<String>,
    search: Search,
    pub hide_videos: HideVideos,
    io_tx: UnboundedSender<IoEvent>,
    pub channel_selection: SelectionList<Channel>,
    pub stream_formats: Formats,
}

impl App {
    pub fn new(io_tx: UnboundedSender<IoEvent>) -> Result<Self> {
        let hide_videos = match (CONFIG.hide_watched, CONFIG.hide_members_only) {
            (true, true) => HideVideos::all(),
            (true, false) => HideVideos::WATCHED,
            (false, true) => HideVideos::MEMBERS_ONLY,
            (false, false) => HideVideos::empty(),
        };
        let mut app = Self {
            emulator: None,
            channels: StatefulList::with_items(Vec::default()),
            tabs: Tabs::default(),
            tags: SelectionList::default(),
            selected: Selected::default(),
            mode: Mode::default(),
            conn: database::open_db(&CONFIG.database)?,
            thumbnail: None,
            message: Message::new(),
            playback_state: PlaybackState::default(),
            progress_tracker: ProgressTracker::default(),
            input: Input::default(),
            input_mode: InputMode::Normal,
            prev_input_mode: InputMode::Normal,
            search: Search::default(),
            new_video_ids: HashSet::default(),
            channels_with_new_videos: HashSet::default(),
            hide_videos,
            io_tx,
            help_window_state: HelpWindowState::new(),
            import_state: SelectionList::default(),
            channel_selection: SelectionList::default(),
            stream_formats: Formats::default(),
        };

        if CLAP_ARGS.contains_id("tick_rate")
            || CLAP_ARGS.contains_id("highlight_symbol")
            || CLAP_ARGS.contains_id("request_timeout")
        {
            app.set_warning_message(
                "--tick-rate, --request-timeout and --highlight-symbol arguments are deprecated. \
                Set them in the config file.",
            );
        }

        app.load_channels();

        match CONFIG.mode {
            Mode::Subscriptions => {
                app.set_mode_subs();
                app.on_change_channel();
            }
            Mode::LatestVideos => {
                app.set_mode_latest_videos();
                app.on_change_video();
            }
        }

        if CONFIG.refresh_on_launch {
            app.refresh_channels();
        }

        app.tags = SelectionList::new(database::get_tags(&app.conn)?);

        Ok(app)
    }

    pub fn add_channel(&mut self, channel_feed: ChannelFeed) {
        let channel = Channel::new(
            channel_feed.channel_id.clone().unwrap(),
            channel_feed.channel_title.clone().unwrap(),
            crate::utils::now().ok(),
        );

        if let Err(e) = database::create_channel(&self.conn, &channel) {
            self.set_error_message(&e.to_string());
            return;
        }
        self.channels.items.push(channel);
        self.add_tabs(channel_feed);
    }

    pub fn add_tabs(&mut self, mut channel_feed: ChannelFeed) {
        self.add_videos(&mut channel_feed, ChannelTab::Videos);
        self.add_videos(&mut channel_feed, ChannelTab::Shorts);
        self.add_videos(&mut channel_feed, ChannelTab::Streams);
    }

    fn add_videos(&mut self, channel_feed: &mut ChannelFeed, tab: ChannelTab) {
        let videos = match tab {
            ChannelTab::Videos => &mut channel_feed.videos,
            ChannelTab::Shorts => &mut channel_feed.shorts,
            ChannelTab::Streams => &mut channel_feed.live_streams,
        };

        if videos.is_empty() {
            return;
        }

        let channel_id = channel_feed.channel_id.as_ref().unwrap();

        let present_videos: Vec<VideoListItem> =
            match database::get_videos(&self.conn, channel_id, tab) {
                Ok(videos) => videos,
                Err(e) => {
                    self.set_error_message(&e.to_string());
                    return;
                }
            };

        // Videos sharing the same published text has the same unix time. Because of this, to
        // preserve a new video's order relative to the other videos sharing the same published
        // text, they need to be replaced in the database.
        let mut timestamps: HashMap<u64, Vec<FetchedVideo>> = HashMap::new();
        let mut to_be_added = HashSet::new();
        let mut added_new_video = false;

        for video in videos.drain(..) {
            if let Some(p_video) = present_videos
                .iter()
                .find(|p_video| p_video.video_id == video.video_id)
            {
                if p_video.needs_update(&video) {
                    to_be_added.insert(video.published);
                }
            } else {
                if CONFIG.prefer_original_titles {
                    self.dispatch(IoEvent::GetVideoTitle(video.video_id.clone()));
                }

                self.new_video_ids.insert(video.video_id.clone());
                added_new_video = true;
                to_be_added.insert(video.published);
            }

            timestamps.entry(video.published).or_default().push(video);
        }

        let videos = timestamps
            .into_iter()
            .filter(|(date, _)| to_be_added.contains(date))
            .flat_map(|(_, video)| video)
            .map(|FetchedVideo { video, .. }| video)
            .collect::<Vec<Video>>();

        if videos.is_empty() {
            return;
        }

        if let Err(e) = database::add_videos(&self.conn, channel_id, &videos, tab) {
            self.set_error_message(&e.to_string());
            return;
        }

        if added_new_video {
            if self.channels.find_by_id(channel_id).is_some() {
                self.move_channel_to_top(channel_id);
                self.reload_videos();
            } else {
                self.channels_with_new_videos.insert(channel_id.clone());
            }
        } else if !videos.is_empty() {
            self.load_videos(true);
        }
    }

    pub fn get_more_videos(&mut self, get_all: bool) {
        if let Some(current_channel) = self.channels.get_selected()
            && let Some(tab) = self.tabs.get_selected()
        {
            let channel_id = current_channel.channel_id.clone();
            let present_videos = if self.hide_videos.is_empty() {
                tab.videos
                    .items
                    .iter()
                    .map(|video| video.video_id.clone())
                    .collect()
            } else {
                match database::get_videos(&self.conn, &current_channel.channel_id, tab.variant) {
                    Ok(videos) => videos
                        .into_iter()
                        .map(|VideoListItem { video, .. }| video.video_id)
                        .collect(),
                    Err(e) => {
                        self.set_error_message(&e.to_string());
                        return;
                    }
                }
            };

            self.dispatch(IoEvent::LoadMoreVideos(
                channel_id,
                tab.variant,
                present_videos,
                get_all,
            ));
        }
    }

    pub fn delete_selected_video(&mut self) {
        if let Some(videos) = self.tabs.get_videos_mut()
            && let Some(idx) = videos.state.selected()
        {
            if let Err(e) = database::delete_video(&self.conn, &videos.items[idx].video_id) {
                self.set_error_message(&e.to_string());
                return;
            }
            videos.items.remove(idx);
            videos.check_bounds();
            self.on_change_video();
        }
    }

    fn move_channel_to_top(&mut self, channel_id: &str) {
        let id_of_current_channel = self
            .get_current_channel()
            .map(|channel| channel.channel_id.clone());
        let index = self.channels.find_by_id(channel_id).unwrap();
        let mut channel = self.channels.items.remove(index);
        channel.new_video |= true;
        self.channels_with_new_videos
            .insert(channel.channel_id.clone());
        self.channels.items.insert(0, channel);
        if let Some(id) = id_of_current_channel {
            let index = self.channels.find_by_id(&id).unwrap();
            self.channels.select_with_index(index);
        }
    }

    pub fn load_channels(&mut self) {
        let selected_tags: Vec<&str> = self
            .tags
            .get_selected_items()
            .iter()
            .map(|tag| tag.as_str())
            .collect();

        match database::get_channels(&self.conn, &selected_tags) {
            Ok(mut channels) => {
                for channel in &mut channels {
                    channel.new_video = self.channels_with_new_videos.contains(&channel.channel_id);
                }

                self.channels = channels.into();
            }
            Err(e) => self.set_error_message(&e.to_string()),
        }
    }

    fn reload_channels(&mut self) {
        let id_of_current_channel = self
            .get_current_channel()
            .map(|channel| channel.channel_id.clone());

        self.load_channels();

        if let Some(id) = id_of_current_channel {
            if let Some(index) = self.channels.find_by_id(&id) {
                self.channels.select_with_index(index);
            } else {
                self.channels.check_bounds();
            }
        }

        self.on_change_channel();
    }

    pub fn set_mode_subs(&mut self) {
        if !matches!(self.mode, Mode::Subscriptions) {
            self.mode = Mode::Subscriptions;
            self.selected = Selected::Channels;
            self.channels.state.select(None);
            self.select_first();
        }
    }

    pub fn set_mode_latest_videos(&mut self) {
        if !matches!(self.mode, Mode::LatestVideos) {
            self.mode = Mode::LatestVideos;
            self.selected = Selected::Videos;
            self.load_videos(false);
            self.on_change_video();
        }
    }

    pub fn get_current_channel(&self) -> Option<&Channel> {
        self.channels.get_selected()
    }

    pub fn get_current_video(&self) -> Option<&VideoListItem> {
        self.tabs.get_selected_video()
    }

    pub fn set_watched(&mut self, video_id: &str, is_watched: bool) {
        if let Some(video) = self.tabs.get_video_mut_by_id(video_id) {
            video.watched = is_watched;
        }

        if let Err(e) = database::set_watched(&self.conn, video_id, is_watched) {
            self.set_error_message(&e.to_string());
        }
    }

    pub fn handle_playback_update(&mut self, update: PlaybackUpdate) {
        let PlaybackUpdate { state, cause } = update;

        let video_id = state
            .metadata
            .as_ref()
            .map(|metadata| metadata.video_id.clone());
        let duration = video_id
            .as_deref()
            .and_then(|id| self.tabs.get_video_by_id(id))
            .and_then(|video| video.length.map(u64::from))
            .or(state.duration);
        let actions = video_id.as_deref().map(|video_id| {
            self.progress_tracker.handle_update(
                video_id,
                state.elapsed,
                duration,
                &cause,
                CONFIG.watched_threshold,
            )
        });

        self.playback_state = state;

        if let Some(video_id) = video_id
            && let Some(actions) = actions
        {
            self.apply_progress_actions(&video_id, actions);
        }
    }

    fn apply_progress_actions(&mut self, video_id: &str, actions: ProgressActions) {
        if let Some(save) = actions.previous_save {
            self.persist_progress(&save.video_id, save.position);
        }

        if let Some(position) = actions.position
            && let Some(video) = self.tabs.get_video_mut_by_id(video_id)
        {
            video.position = Some(position);
        }

        if let Some(position) = actions.save_position
            && self.persist_progress(video_id, position)
        {
            self.progress_tracker.mark_saved(position);
        }

        if CONFIG.auto_mark_watched && actions.mark_watched {
            self.set_watched(video_id, true);
        }
    }

    fn persist_progress(&mut self, video_id: &str, position: u64) -> bool {
        match database::set_position(&self.conn, video_id, position) {
            Ok(()) => true,
            Err(error) => {
                self.set_error_message(&error.to_string());
                false
            }
        }
    }

    pub fn toggle_watched(&mut self) {
        let Some((video_id, watched)) = self
            .get_current_video()
            .map(|video| (video.id().to_owned(), video.watched))
        else {
            return;
        };

        self.set_watched(&video_id, !watched);
    }

    pub fn toggle_hide(&mut self) {
        self.hide_videos.toggle(HideVideos::WATCHED);
        self.reload_videos();
    }

    pub fn play_video(&mut self) {
        if let Some(spec) = self.get_current_video_spec() {
            self.dispatch(IoEvent::PlayUsingYtdlp(spec));
        }
    }

    pub fn get_current_video_spec(&self) -> Option<PlaybackSpec> {
        let video = self.get_current_video()?;

        let channel = match video.channel_name.as_deref() {
            Some(channel) => channel,
            None => &self.get_current_channel()?.channel_name,
        };

        Some(PlaybackSpec {
            metadata: VideoMetadata {
                video_id: video.video_id.clone(),
                title: video.title.clone(),
                channel: channel.to_owned(),
            },
            start_position: CONFIG
                .resume_playback
                .then(|| video.resume_position())
                .flatten(),
        })
    }

    pub fn play_audio(&mut self) {
        if let Some(metadata) = self.get_current_video_spec() {
            self.dispatch(IoEvent::FetchFormats(metadata, FormatAction::PlayAudio));
        }
    }

    pub fn play_audio_using_ytdlp(&mut self) {
        if let Some(metadata) = self.get_current_video_spec() {
            self.dispatch(IoEvent::PlayAudioUsingYtdlp(metadata));
        }
    }

    pub fn toggle_playback(&mut self) {
        self.dispatch(IoEvent::TogglePlayback);
    }

    pub fn seek_playback(&mut self, seconds: i32) {
        self.dispatch(IoEvent::SeekPlayback(seconds));
    }

    pub fn adjust_volume(&mut self, value: i8) {
        self.dispatch(IoEvent::AdjustVolume(value));
    }

    pub fn toggle_mute(&mut self) {
        self.dispatch(IoEvent::ToggleMute);
    }

    pub fn stop_playback(&mut self) {
        self.dispatch(IoEvent::StopPlayback);
    }

    pub fn release_video(&mut self) {
        self.dispatch(IoEvent::ReleaseVideo);
    }

    pub fn enter_format_selection(&mut self) {
        if let Some(metadata) = self.get_current_video_spec() {
            self.dispatch(IoEvent::FetchFormats(metadata, FormatAction::Select));
        }
    }

    pub fn play_from_formats(&mut self) {
        if let Some(metadata) = self.get_current_video_spec() {
            self.dispatch(IoEvent::FetchFormats(metadata, FormatAction::PlayVideo));
        }
    }

    pub fn confirm_selected_streams(&mut self) {
        self.input_mode = InputMode::Normal;
        let formats = mem::take(&mut self.stream_formats);
        self.dispatch(IoEvent::PlayFromFormats(Box::new(formats)));
    }

    pub fn copy_url_to_clipboard(&mut self, api: ApiBackend) {
        let url_component = match self.selected {
            Selected::Channels => match self.get_current_channel() {
                Some(current_channel) => {
                    format!("channel/{}", current_channel.channel_id)
                }
                None => return,
            },
            Selected::Videos => match self.get_current_video() {
                Some(current_video) => {
                    format!("watch?v={}", current_video.video_id)
                }
                None => return,
            },
        };

        self.dispatch(IoEvent::CopyLink(url_component, api));
    }

    pub fn copy_url_at_time(&mut self, api: ApiBackend) {
        if self.is_player_active()
            && let Some(metadata) = &self.playback_state.metadata
            && let Some(elapsed) = self.playback_state.elapsed
        {
            let url_component = format!("watch?v={}&t={elapsed}s", metadata.video_id);
            self.dispatch(IoEvent::CopyLink(url_component, api));
        } else {
            self.set_warning_message("No active playback to copy a timestamp from");
        }
    }

    pub fn open_in_browser(&mut self, api: ApiBackend) {
        let url_component = match self.selected {
            Selected::Channels => match self.get_current_channel() {
                Some(current_channel) => {
                    format!("channel/{}", current_channel.channel_id)
                }
                None => return,
            },
            Selected::Videos => match self.get_current_video() {
                Some(current_video) => {
                    format!("watch?v={}", current_video.video_id)
                }
                None => return,
            },
        };

        self.dispatch(IoEvent::OpenInBrowser(url_component, api));
    }

    fn get_videos_of_current_channel(&self) -> Result<TabList> {
        let mut tabs = Vec::with_capacity(3);

        if let Some(channel) = self.get_current_channel() {
            for tab in tabs_to_be_loaded() {
                tabs.push((
                    database::get_videos(&self.conn, &channel.channel_id, tab)?,
                    tab,
                ));
            }
        }

        Ok(tabs)
    }

    fn get_latest_videos(&self) -> Result<Vec<(Vec<VideoListItem>, ChannelTab)>> {
        let selected_tags: Vec<&str> = self
            .tags
            .get_selected_items()
            .iter()
            .map(|tag| tag.as_str())
            .collect();

        let mut tabs = Vec::with_capacity(3);

        for tab in tabs_to_be_loaded() {
            tabs.push((
                database::get_latest_videos(&self.conn, &selected_tags, tab)?,
                tab,
            ));
        }

        Ok(tabs)
    }

    pub fn load_videos(&mut self, preserve_tabs_state: bool) {
        let tabs = match self.mode {
            Mode::Subscriptions => self.get_videos_of_current_channel(),
            Mode::LatestVideos => self.get_latest_videos(),
        };

        match tabs {
            Ok(tabs) => {
                if preserve_tabs_state {
                    self.tabs.update_videos(tabs);
                } else {
                    self.tabs = Tabs::new(tabs);
                }

                for tab in &mut self.tabs.items {
                    if !self.hide_videos.is_empty() {
                        let f = if self
                            .hide_videos
                            .contains(HideVideos::WATCHED | HideVideos::MEMBERS_ONLY)
                        {
                            |video: &VideoListItem| !(video.watched || video.members_only)
                        } else if self.hide_videos.contains(HideVideos::WATCHED) {
                            |video: &VideoListItem| !video.watched
                        } else if self.hide_videos.contains(HideVideos::MEMBERS_ONLY) {
                            |video: &VideoListItem| !video.members_only
                        } else {
                            unreachable!()
                        };

                        tab.videos.items = tab.videos.items.drain(..).filter(f).collect();
                    }

                    let mut count = 0;
                    for video in &mut tab.videos.items {
                        if self.new_video_ids.contains(&video.video_id) {
                            video.is_new = true;
                            tab.has_new_video = true;
                            count += 1;
                        }
                        if count == self.new_video_ids.len() {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                self.tabs.items.clear();
                self.set_error_message(&e.to_string());
            }
        }
    }

    pub fn reload_videos(&mut self) {
        let Some(tab) = self.tabs.get_selected() else {
            self.load_videos(false);
            return;
        };
        let current_tab = tab.variant;

        let id_of_current_video = match tab.videos.get_selected() {
            Some(current_video)
                if self.hide_videos.contains(HideVideos::WATCHED) && current_video.watched =>
            {
                // if the currently selected video is watched, jump to the first unwatched video above
                let mut index = tab.videos.state.selected().unwrap();
                loop {
                    if let Some(i) = index.checked_sub(1) {
                        index = i;
                    } else {
                        break None;
                    }

                    let video = &tab.videos.items[index];
                    if !video.watched {
                        break Some(video.video_id.clone());
                    }
                }
            }
            Some(current_video) => Some(current_video.video_id.clone()),
            None => None,
        };

        self.load_videos(false);
        self.tabs.select_tab(current_tab);

        let Some(tab) = self.tabs.get_mut_selected() else {
            return;
        };

        match id_of_current_video {
            Some(id) => {
                tab.videos.state.select(tab.videos.find_by_id(&id));
            }
            None => tab.videos.reset_state(),
        }

        self.on_change_video();
    }

    pub fn on_change_channel(&mut self) {
        self.load_videos(false);
        self.on_change_video();
    }

    pub fn on_change_video(&mut self) {
        if let Some(emulator) = &self.emulator
            && let Some(video) = self.get_current_video()
        {
            self.dispatch(IoEvent::GetThumbnail(
                emulator.graphics_protocol,
                video.video_id.clone(),
            ));
        }
    }

    pub fn set_channel_refresh_state(&mut self, channel_id: &str, refresh_state: RefreshState) {
        let mut channel = self.channels.get_mut_by_id(channel_id);

        if let Some(channel) = channel.as_deref_mut() {
            channel.refresh_state = refresh_state;
        }

        if matches!(refresh_state, RefreshState::Completed) {
            let now = crate::utils::now().ok();

            if let Some(channel) = channel {
                channel.last_refreshed = now;
            }

            if let Err(e) = database::set_last_refreshed_field(&self.conn, channel_id, now) {
                self.set_error_message(&e.to_string());
            }
        }
    }

    pub fn apply_to_focused_list(&mut self, f: impl FnOnce(&mut dyn Selectable)) {
        match self.selected {
            Selected::Channels => {
                let prev = self.channels.state.selected();
                f(&mut self.channels);

                if prev != self.channels.state.selected() {
                    self.on_change_channel();
                }
            }
            Selected::Videos => {
                if let Some(videos) = self.tabs.get_videos_mut() {
                    let prev = videos.state.selected();
                    f(videos);

                    if prev != videos.state.selected() {
                        self.on_change_video();
                    }
                }
            }
        }
    }

    pub fn select_first(&mut self) {
        self.apply_to_focused_list(|list| list.select_first());
    }

    pub const fn on_left(&mut self) {
        if matches!(self.mode, Mode::Subscriptions) {
            self.selected = Selected::Channels;
        }
    }

    pub const fn on_right(&mut self) {
        if matches!(self.mode, Mode::Subscriptions) {
            self.selected = Selected::Videos;
        }
    }

    pub fn jump_to_channel(&mut self) {
        if self.mode == Mode::LatestVideos
            && let Some(tab) = self.tabs.get_selected()
            && let Some(video) = tab.videos.get_selected()
        {
            let tab = tab.variant;
            let video_id = video.video_id.clone();
            let channel_id = video.channel_id.clone();
            self.mode = Mode::Subscriptions;
            self.selected = Selected::Videos;

            if let Some(index) = self.channels.find_by_id(&channel_id) {
                self.channels.select_with_index(index);
                self.on_change_channel();
                self.tabs.select_tab(tab);

                if let Some(videos) = self.tabs.get_videos_mut()
                    && let Some(index) = videos.find_by_id(&video_id)
                {
                    videos.select_with_index(index);
                    self.on_change_video();
                }
            }
        }
    }

    pub fn is_footer_active(&self) -> bool {
        matches!(
            self.input_mode,
            InputMode::Search
                | InputMode::Subscribe
                | InputMode::TagCreation
                | InputMode::TagRenaming
        ) || !self.message.is_empty()
    }

    pub fn is_player_active(&self) -> bool {
        matches!(
            self.playback_state.phase,
            PlaybackPhase::Loading | PlaybackPhase::Playing | PlaybackPhase::Paused
        )
    }

    pub const fn toggle_help(&mut self) {
        self.help_window_state.toggle();
    }

    pub fn prompt_for_subscription(&mut self) {
        self.prev_input_mode = self.input_mode.clone();
        self.input_mode = InputMode::Subscribe;
        self.message.clear_message();
        self.input = Input::new("Enter channel id or url: ");
    }

    pub fn subscribe(&mut self) {
        let input = self.input.take_text();
        self.input_mode = InputMode::Normal;
        self.subscribe_to_channel(input);
    }

    pub const fn prompt_for_unsubscribing(&mut self) {
        if matches!(self.mode, Mode::Subscriptions) && self.channels.state.selected().is_some() {
            self.input_mode = InputMode::Confirmation;
        }
    }

    pub fn unsubscribe(&mut self) {
        if let Some(idx) = self.channels.state.selected() {
            database::delete_channel(&self.conn, &self.channels.items[idx].channel_id).unwrap();
            self.input_mode = InputMode::Normal;
            self.channels.items.remove(idx);
            self.channels.check_bounds();
            self.on_change_channel();
        }
    }

    fn start_searching(&mut self, direction: SearchDirection) {
        self.prev_input_mode = self.input_mode.clone();
        self.input_mode = InputMode::Search;
        self.message.clear_message();
        self.input = Input::new(match direction {
            SearchDirection::Forward => "/",
            SearchDirection::Backward => "?",
        });
        self.search.direction = direction;
    }

    pub fn search_forward(&mut self) {
        self.start_searching(SearchDirection::Forward);
    }

    pub fn search_backward(&mut self) {
        self.start_searching(SearchDirection::Backward);
    }

    pub fn search_in_selected(&mut self, update: SearchUpdate) {
        match self.prev_input_mode {
            InputMode::Normal => match self.selected {
                Selected::Channels => {
                    self.search
                        .search(&mut self.channels, self.input.text(), update);
                    self.on_change_channel();
                }
                Selected::Videos => {
                    if let Some(videos) = self.tabs.get_videos_mut() {
                        self.search.search(videos, self.input.text(), update);
                        self.on_change_video();
                    }
                }
            },
            InputMode::Import => {
                self.search
                    .search(&mut self.import_state, self.input.text(), update)
            }
            InputMode::Tag => self
                .search
                .search(&mut self.tags, self.input.text(), update),
            InputMode::ChannelSelection => {
                self.search
                    .search(&mut self.channel_selection, self.input.text(), update);
            }
            InputMode::FormatSelection => self.search.search(
                self.stream_formats.get_mut_selected_tab(),
                self.input.text(),
                update,
            ),
            _ => panic!(),
        }
    }

    fn repeat_last_search_helper(&mut self, opposite: bool) {
        match self.input_mode {
            InputMode::Normal => match self.selected {
                Selected::Channels => {
                    self.search.repeat_last(&mut self.channels, opposite);
                    self.on_change_channel();
                }
                Selected::Videos => {
                    if let Some(videos) = self.tabs.get_videos_mut() {
                        self.search.repeat_last(videos, opposite);
                        self.on_change_video();
                    }
                }
            },
            InputMode::Import => self.search.repeat_last(&mut self.import_state, opposite),
            InputMode::Tag => self.search.repeat_last(&mut self.tags, opposite),
            InputMode::ChannelSelection => self
                .search
                .repeat_last(&mut self.channel_selection, opposite),
            InputMode::FormatSelection => self
                .search
                .repeat_last(self.stream_formats.get_mut_selected_tab(), opposite),
            _ => panic!(),
        }
        if self.no_search_pattern_match() {
            self.set_error_message(&format!("Pattern not found: {}", self.search.pattern));
        }
        self.search.complete_search(true);
        self.search.pattern.clear();
    }

    pub fn repeat_last_search(&mut self) {
        self.repeat_last_search_helper(false);
    }

    pub fn repeat_last_search_opposite(&mut self) {
        self.repeat_last_search_helper(true);
    }

    pub fn update_search_after_input(&mut self, change: InputChange) {
        let update = match change {
            InputChange::Append => SearchUpdate::Filter,
            InputChange::Insert | InputChange::Delete => SearchUpdate::Build,
        };

        self.search_in_selected(update);
    }

    pub const fn no_search_pattern_match(&self) -> bool {
        !self.search.pattern.is_empty() && !self.search.any_matches()
    }

    pub fn complete_search(&mut self) {
        if self.no_search_pattern_match() {
            self.set_error_message(&format!("Pattern not found: {}", self.search.pattern));
        }
        self.finalize_search(false);
    }

    pub fn finalize_search(&mut self, abort: bool) {
        self.input_mode = self.prev_input_mode.clone();
        self.input.clear();
        self.search.complete_search(abort);
    }

    fn recover_item(&mut self) {
        if self.search.recovery_index.is_some() {
            match self.prev_input_mode {
                InputMode::Normal => match self.selected {
                    Selected::Channels => {
                        self.search.recover_item(&mut self.channels);
                        self.on_change_channel();
                    }
                    Selected::Videos => {
                        if let Some(videos) = self.tabs.get_videos_mut() {
                            self.search.recover_item(videos);
                            self.on_change_video();
                        }
                    }
                },
                InputMode::Import => self.search.recover_item(&mut self.import_state),
                InputMode::Tag => self.search.recover_item(&mut self.tags),
                InputMode::ChannelSelection => {
                    self.search.recover_item(&mut self.channel_selection);
                }
                InputMode::FormatSelection => self
                    .search
                    .recover_item(self.stream_formats.get_mut_selected_tab()),
                _ => panic!(),
            }
        }
    }

    pub fn abort_search(&mut self) {
        self.recover_item();
        self.finalize_search(true);
    }

    pub fn select_channels_to_import(&mut self, path: &Path, format: import::Format) -> Result<()> {
        let mut import_state = match format {
            import::Format::YoutubeCsv => import::YoutubeCsv::import(path),
            import::Format::NewPipe => import::NewPipe::import(path),
        }
        .with_context(|| "Failed to import")?;

        import_state = import_state
            .into_iter()
            .filter(|entry| self.channels.find_by_id(&entry.channel_id).is_none())
            .collect::<Vec<ImportItem>>();

        if import_state.is_empty() {
            self.set_warning_message("Already subscribed to all the channels in the file");
            return Ok(());
        }

        self.import_state = SelectionList::new(import_state);
        self.import_state.select_all();

        self.input_mode = InputMode::Import;

        Ok(())
    }

    pub fn confirm_import(&mut self) {
        self.import_state.items = self
            .import_state
            .items
            .drain(..)
            .filter(|entry| entry.selected)
            .collect();

        if self.import_state.items.is_empty() {
            self.input_mode = InputMode::Normal;
            return;
        }

        self.import_channels();
    }

    pub fn export_subscriptions(&self, path: &Path, format: import::Format) -> Result<()> {
        match format {
            import::Format::YoutubeCsv => import::YoutubeCsv::export(&self.channels.items, path),
            import::Format::NewPipe => import::NewPipe::export(&self.channels.items, path),
        }
    }

    fn dispatch(&mut self, action: IoEvent) {
        if let Err(e) = self.io_tx.send(action) {
            self.set_error_message(&format!("Error from dispatch: {e}"));
        }
    }

    pub fn subscribe_to_channel(&mut self, input: String) {
        self.set_message("Resolving channel id");
        self.dispatch(IoEvent::SubscribeToChannel(input));
    }

    pub fn import_channels(&mut self) {
        let ids = self
            .import_state
            .items
            .iter_mut()
            .map(|channel| {
                channel.sub_state = RefreshState::ToBeRefreshed;
                channel.channel_id.clone()
            })
            .collect();

        self.dispatch(IoEvent::ImportChannels(ids));
    }

    fn get_channels_for_refreshing(&mut self, filter_failed: bool) -> Vec<String> {
        self.channels
            .items
            .iter_mut()
            .filter(|channel| {
                filter_failed && matches!(channel.refresh_state, RefreshState::Failed)
                    || !filter_failed
                        && !matches!(
                            channel.last_refreshed,
                            Some(time) if utils::time_passed(time).is_ok_and(|t| t < CONFIG.refresh_threshold)
                        )
            })
            .map(|channel| {
                channel.set_to_be_refreshed();
                channel.channel_id.clone()
            })
            .collect::<Vec<String>>()
    }

    pub fn refresh_channel(&mut self) {
        if let Some(current_channel) = self.get_current_channel() {
            let channel_id = current_channel.channel_id.clone();
            self.dispatch(IoEvent::RefreshChannels(vec![channel_id]));
        }
    }

    pub fn refresh_channels(&mut self) {
        if self.channels.items.is_empty() {
            return;
        }

        let ids = self.get_channels_for_refreshing(false);

        if ids.is_empty() {
            self.set_warning_message("All the channels have been recently refreshed");
        } else {
            self.dispatch(IoEvent::RefreshChannels(ids));
        }
    }

    pub fn refresh_failed_channels(&mut self) {
        if self.channels.items.is_empty() {
            return;
        }

        let ids = self.get_channels_for_refreshing(true);

        if ids.is_empty() {
            self.set_warning_message("There are no channels to retry refreshing");
        } else {
            self.dispatch(IoEvent::RefreshChannels(ids));
        }
    }

    pub fn set_message(&mut self, message: &str) {
        self.message.set_message(message);
    }

    pub fn _set_message_with_default_duration(&mut self, message: &str) {
        const DEFAULT_DURATION: u64 = 5;
        self.set_message(message);
        self.clear_message_after_duration(DEFAULT_DURATION);
    }

    pub fn set_error_message(&mut self, message: &str) {
        const DEFAULT_DURATION: u64 = 5;
        self.message.set_error_message(message);
        self.clear_message_after_duration(DEFAULT_DURATION);
    }

    pub fn set_warning_message(&mut self, message: &str) {
        const DEFAULT_DURATION: u64 = 5;
        self.message.set_warning_message(message);
        self.clear_message_after_duration(DEFAULT_DURATION);
    }

    pub fn clear_message_after_duration(&mut self, duration_seconds: u64) {
        self.dispatch(IoEvent::ClearMessage(
            self.message.clone_token(),
            duration_seconds,
        ));
    }

    pub const fn toggle_tag_selection(&mut self) {
        if matches!(self.input_mode, InputMode::Tag) {
            self.input_mode = InputMode::Normal;
        } else {
            self.input_mode = InputMode::Tag;
        }
    }

    pub fn enter_tag_creation(&mut self) {
        self.prev_input_mode = self.input_mode.clone();
        self.input_mode = InputMode::TagCreation;
        self.message.clear_message();
        self.input = Input::new("Tag name: ");
    }

    pub fn enter_tag_renaming(&mut self) {
        if let Some(tag) = self.tags.get_selected() {
            let mut input = Input::new("Tag name: ");
            input.set_text(&tag.item);
            self.prev_input_mode = self.input_mode.clone();
            self.input_mode = InputMode::TagRenaming;
            self.message.clear_message();
            self.input = input;
        }
    }

    pub fn enter_channel_selection(&mut self) {
        if let Some(selected_tag) = &self.tags.get_selected() {
            self.input_mode = InputMode::ChannelSelection;

            let mut all_channels =
                SelectionList::new(database::get_channels(&self.conn, &[]).unwrap());

            let selected_channels = database::get_channels(&self.conn, &[selected_tag]).unwrap();

            for channel in selected_channels {
                if let Some(c) = all_channels.get_mut_by_id(&channel.channel_id) {
                    c.selected = true;
                }
            }

            self.channel_selection = all_channels;
        }
    }

    pub fn update_tag(&mut self) {
        let selected_channels: Vec<String> = self
            .channel_selection
            .get_selected_items()
            .into_iter()
            .map(|channel| channel.channel_id.clone())
            .collect();

        database::update_channels_of_tag(
            &self.conn,
            self.tags.get_selected().unwrap(),
            &selected_channels,
        )
        .unwrap();

        self.reload_channels();

        self.input_mode = InputMode::Tag;
    }

    pub fn create_tag(&mut self) {
        if let Err(e) = database::create_tag(&self.conn, self.input.text()) {
            self.set_error_message(&e.to_string());
        } else {
            self.tags
                .items
                .push(SelectionItem::new(self.input.text().to_owned()));
        }

        self.input_mode = InputMode::Tag;
        self.input.clear();
    }

    pub fn rename_selected_tag(&mut self) {
        let input = self.input.text().to_owned();
        if let Some(tag) = self.tags.get_mut_selected() {
            if let Err(e) = database::rename_tag(&self.conn, &tag.item, &input) {
                self.set_error_message(&e.to_string());
            } else {
                input.clone_into(&mut tag.item);
            }
        }

        self.input_mode = InputMode::Tag;
        self.input.clear();
    }

    pub fn delete_selected_tag(&mut self) {
        if let Some(idx) = self.tags.state.selected() {
            if let Err(e) = database::delete_tag(&self.conn, &self.tags.items[idx].item) {
                self.set_error_message(&e.to_string());
                return;
            }

            if self.tags.items.remove(idx).selected {
                self.reload_channels();
            }

            self.tags.check_bounds();
        }
    }

    pub fn switch_api(&mut self) {
        self.dispatch(IoEvent::SwitchApi);
    }
}

#[derive(Default)]
pub enum Selected {
    #[default]
    Channels,
    Videos,
}

#[derive(PartialEq, Eq, Clone, Debug, Default, Deserialize)]
#[serde(rename_all(deserialize = "snake_case"))]
pub enum Mode {
    #[default]
    #[serde(alias = "subs")]
    Subscriptions,
    LatestVideos,
}

#[derive(Deserialize)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum VideoPlayer {
    Mpv,
    Vlc,
}

type TabList = Vec<(Vec<VideoListItem>, ChannelTab)>;

pub struct Tab {
    pub variant: ChannelTab,
    pub videos: StatefulList<VideoListItem, TableState>,
    pub has_new_video: bool,
}

impl Tab {
    pub fn new(variant: ChannelTab, videos: Vec<VideoListItem>) -> Self {
        Self {
            variant,
            videos: StatefulList::with_items(videos),
            has_new_video: false,
        }
    }
}

#[derive(Default)]
pub struct Tabs(StatefulList<Tab, ListState>);

impl Tabs {
    pub fn new(tabs: TabList) -> Self {
        Self(StatefulList::with_items(
            tabs.into_iter()
                .filter(|(videos, _)| !videos.is_empty())
                .map(|(videos, variant)| Tab::new(variant, videos))
                .collect(),
        ))
    }

    pub fn update_videos(&mut self, tabs: TabList) {
        for (mut idx, (videos, variant)) in
            tabs.into_iter().filter(|(v, _)| !v.is_empty()).enumerate()
        {
            if let Some(tab) = self.items.get_mut(idx)
                && tab.variant == variant
            {
                tab.videos.items = videos;
            } else {
                while self
                    .items
                    .get(idx)
                    .is_some_and(|tab| (tab.variant as u8) < variant as u8)
                {
                    idx += 1;
                }

                self.items.insert(idx, Tab::new(variant, videos));
            }
        }

        if self.state.selected().is_none() {
            self.select_first();
        }
    }

    fn select_tab(&mut self, tab: ChannelTab) {
        let idx = self.items.iter().position(|item| item.variant == tab);

        if idx.is_some() {
            self.state.select(idx);
        }
    }

    fn get_videos_mut(&mut self) -> Option<&mut StatefulList<VideoListItem, TableState>> {
        self.get_mut_selected().map(|tab| &mut tab.videos)
    }

    fn get_video_by_id(&self, video_id: &str) -> Option<&VideoListItem> {
        self.items.iter().find_map(|tab| {
            tab.videos
                .items
                .iter()
                .find(|video| video.video_id == video_id)
        })
    }

    fn get_video_mut_by_id(&mut self, video_id: &str) -> Option<&mut VideoListItem> {
        self.items.iter_mut().find_map(|tab| {
            tab.videos
                .items
                .iter_mut()
                .find(|video| video.video_id == video_id)
        })
    }

    pub fn get_selected_video(&self) -> Option<&VideoListItem> {
        self.get_selected()
            .and_then(|tab| tab.videos.get_selected())
    }
}

impl Deref for Tabs {
    type Target = StatefulList<Tab, ListState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Tabs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
