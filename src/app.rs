use crate::api::{ApiBackend, ChannelFeed};
use crate::channel::{
    Channel, ChannelTab, HideVideos, ListItem, RefreshState, Video, tabs_to_be_loaded,
};
use crate::help::HelpWindowState;
use crate::import::{self, ImportItem};
use crate::input::InputMode;
use crate::message::Message;
use crate::search::{Search, SearchDirection, SearchState};
use crate::stream_formats::Formats;
use crate::{CLAP_ARGS, IoEvent, OPTIONS, database, utils};
use anyhow::{Context, Result};
use ratatui::widgets::{ListState, TableState};
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use tokio::sync::mpsc::UnboundedSender;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl ListItem for String {
    fn id(&self) -> &str {
        self
    }
}

pub struct App {
    pub channels: StatefulList<Channel, ListState>,
    pub tabs: Tabs,
    pub tags: SelectionList<String>,
    pub selected: Selected,
    pub mode: Mode,
    pub conn: Connection,
    pub message: Message,
    pub input: String,
    pub input_mode: InputMode,
    pub input_idx: usize,
    pub prev_input_mode: InputMode,
    pub cursor_position: u16,
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
        let hide_videos = match (OPTIONS.hide_watched, OPTIONS.hide_members_only) {
            (true, true) => HideVideos::all(),
            (true, false) => HideVideos::WATCHED,
            (false, true) => HideVideos::MEMBERS_ONLY,
            (false, false) => HideVideos::empty(),
        };

        let mut app = Self {
            channels: StatefulList::with_items(Vec::default()),
            tabs: Tabs::default(),
            tags: SelectionList::default(),
            selected: Selected::Channels,
            mode: Mode::Subscriptions,
            conn: Connection::open(OPTIONS.database.clone())?,
            message: Message::new(),
            input: String::default(),
            input_mode: InputMode::Normal,
            input_idx: 0,
            prev_input_mode: InputMode::Normal,
            cursor_position: 0,
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

        database::initialize_db(&mut app.conn)?;
        app.set_mode_subs();
        app.load_channels();
        app.on_change_channel();

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

        let present_videos: Vec<Video> = match database::get_videos(&self.conn, channel_id, tab) {
            Ok(videos) => videos,
            Err(e) => {
                self.set_error_message(&e.to_string());
                return;
            }
        };

        // Videos sharing the same published text has the same unix time. Because of this, to
        // preserve a new video's order relative to the other videos sharing the same published
        // text, they need to be replaced in the database.
        let mut timestamps: HashMap<u64, Vec<Video>> = HashMap::new();
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
                self.channels_with_new_videos.insert(channel_id.to_string());
            }
        } else if !videos.is_empty() {
            self.load_videos(true);
        }
    }

    pub fn get_more_videos(&mut self) {
        if let Some(current_channel) = self.channels.get_selected()
            && let Some(tab) = self.tabs.get_selected()
        {
            self.message.set_message("Fetching videos");

            let channel_id = current_channel.channel_id.clone();
            let present_videos = if self.hide_videos.is_empty() {
                tab.videos
                    .items
                    .iter()
                    .map(|video| video.video_id.clone())
                    .collect()
            } else {
                match database::get_videos(&self.conn, &current_channel.channel_id, tab.variant) {
                    Ok(videos) => videos.into_iter().map(|video| video.video_id).collect(),
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
            self.select_first();
        }
    }

    fn find_channel_by_name(&mut self, channel_name: &str) -> Option<usize> {
        self.channels
            .items
            .iter()
            .position(|channel| channel.channel_name == channel_name)
    }

    pub fn get_current_channel(&self) -> Option<&Channel> {
        self.channels.get_selected()
    }

    pub fn get_current_video(&self) -> Option<&Video> {
        self.tabs.get_selected_video()
    }

    pub fn set_watched(&mut self, video_id: &str, is_watched: bool) {
        if let Some(videos) = self.tabs.get_videos_mut()
            && let Some(video) = videos.get_mut_by_id(video_id)
        {
            video.watched = is_watched;
        }

        if let Err(e) = database::add_watched(&self.conn, video_id, is_watched) {
            self.set_error_message(&e.to_string());
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
        if let Some(current_video) = self.get_current_video() {
            self.dispatch(IoEvent::PlayUsingYtdlp(current_video.video_id.clone()));
        }
    }

    pub fn enter_format_selection(&mut self) {
        let Some(current_video) = self.get_current_video() else {
            return;
        };

        self.dispatch(IoEvent::FetchFormats(
            current_video.title.clone(),
            current_video.video_id.clone(),
            false,
        ));
    }

    pub fn play_from_formats(&mut self) {
        let Some(current_video) = self.get_current_video() else {
            return;
        };

        self.dispatch(IoEvent::FetchFormats(
            current_video.title.clone(),
            current_video.video_id.clone(),
            true,
        ));
    }

    pub fn confirm_selected_streams(&mut self) {
        self.input_mode = InputMode::Normal;
        let formats = mem::take(&mut self.stream_formats);
        self.dispatch(IoEvent::PlayFromFormats(Box::new(formats)));
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

    fn get_latest_videos(&self) -> Result<Vec<(Vec<Video>, ChannelTab)>> {
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
                            |video: &Video| !(video.watched || video.members_only)
                        } else if self.hide_videos.contains(HideVideos::WATCHED) {
                            |video: &Video| !video.watched
                        } else if self.hide_videos.contains(HideVideos::MEMBERS_ONLY) {
                            |video: &Video| !video.members_only
                        } else {
                            unreachable!()
                        };

                        tab.videos.items = tab.videos.items.drain(..).filter(f).collect();
                    }

                    let mut count = 0;
                    for video in &mut tab.videos.items {
                        if self.new_video_ids.contains(&video.video_id) {
                            video.new = true;
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
    }

    pub fn on_change_channel(&mut self) {
        self.load_videos(false);
    }

    pub fn set_channel_refresh_state(&mut self, channel_id: &str, refresh_state: RefreshState) {
        let mut channel = self.channels.get_mut_by_id(channel_id);

        if let Some(channel) = channel.as_deref_mut() {
            channel.refresh_state = refresh_state;
        }

        if let RefreshState::Completed = refresh_state {
            let now = crate::utils::now().ok();

            if let Some(channel) = channel {
                channel.last_refreshed = now;
            }

            if let Err(e) = database::set_last_refreshed_field(&self.conn, channel_id, now) {
                self.set_error_message(&e.to_string());
            }
        }
    }

    pub fn on_down(&mut self) {
        match self.selected {
            Selected::Channels => {
                self.channels.next();
                self.on_change_channel();
            }
            Selected::Videos => {
                if let Some(videos) = self.tabs.get_videos_mut() {
                    videos.next();
                }
            }
        }
    }

    pub fn on_up(&mut self) {
        match self.selected {
            Selected::Channels => {
                self.channels.previous();
                self.on_change_channel();
            }
            Selected::Videos => {
                if let Some(videos) = self.tabs.get_videos_mut() {
                    videos.previous();
                }
            }
        }
    }

    pub fn on_left(&mut self) {
        if matches!(self.mode, Mode::Subscriptions) {
            self.selected = Selected::Channels;
        }
    }

    pub fn on_right(&mut self) {
        if matches!(self.mode, Mode::Subscriptions) {
            self.selected = Selected::Videos;
        }
    }

    pub fn select_first(&mut self) {
        match self.selected {
            Selected::Channels => {
                if let Some(0) = self.channels.state.selected() {
                    return;
                }
                self.channels.select_first();
                self.on_change_channel();
            }
            Selected::Videos => {
                if let Some(videos) = self.tabs.get_videos_mut() {
                    videos.select_first();
                }
            }
        }
    }

    pub fn select_last(&mut self) {
        match self.selected {
            Selected::Channels => {
                let length = self.channels.items.len();
                if matches!(self.channels.state.selected(), Some(index) if index + 1 == length) {
                    return;
                }
                self.channels.select_last();
                self.on_change_channel();
            }
            Selected::Videos => {
                if let Some(videos) = self.tabs.get_videos_mut() {
                    videos.select_last();
                }
            }
        }
    }

    pub fn jump_to_channel(&mut self) {
        if let Mode::LatestVideos = self.mode
            && let Some(tab) = self.tabs.get_selected()
            && let Some(video) = tab.videos.get_selected()
            && let Some(channel_name) = &video.channel_name
        {
            let tab = tab.variant;
            let video_id = video.video_id.clone();
            let channel_name = channel_name.clone();
            self.mode = Mode::Subscriptions;
            self.selected = Selected::Videos;

            if let Some(index) = self.find_channel_by_name(&channel_name) {
                self.channels.select_with_index(index);
                self.on_change_channel();
                self.tabs.select_tab(tab);

                if let Some(videos) = self.tabs.get_videos_mut()
                    && let Some(index) = videos.find_by_id(&video_id)
                {
                    videos.select_with_index(index);
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

    pub fn toggle_help(&mut self) {
        self.help_window_state.toggle();
    }

    pub fn prompt_for_subscription(&mut self) {
        self.prev_input_mode = self.input_mode.clone();
        self.input_mode = InputMode::Subscribe;
        self.message.clear_message();
        self.input_idx = 0;
        self.cursor_position = 0;
    }

    pub fn subscribe(&mut self) {
        let input = self.input.drain(..).collect::<String>();
        self.input_mode = InputMode::Normal;
        self.subscribe_to_channel(input);
    }

    pub fn prompt_for_unsubscribing(&mut self) {
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

    fn start_searching(&mut self) {
        self.prev_input_mode = self.input_mode.clone();
        self.input_mode = InputMode::Search;
        self.message.clear_message();
        self.input_idx = 0;
        self.cursor_position = 0;
    }

    pub fn search_forward(&mut self) {
        self.start_searching();
        self.search.direction = SearchDirection::Forward;
    }

    pub fn search_backward(&mut self) {
        self.start_searching();
        self.search.direction = SearchDirection::Backward;
    }

    pub fn search_direction(&self) -> &SearchDirection {
        &self.search.direction
    }

    pub fn search_in_selected(&mut self) {
        match self.prev_input_mode {
            InputMode::Normal => match self.selected {
                Selected::Channels => {
                    self.search.search(&mut self.channels, &self.input);
                    self.on_change_channel();
                }
                Selected::Videos => {
                    if let Some(videos) = self.tabs.get_videos_mut() {
                        self.search.search(videos, &self.input)
                    }
                }
            },
            InputMode::Import => self.search.search(&mut self.import_state, &self.input),
            InputMode::Tag => self.search.search(&mut self.tags, &self.input),
            InputMode::ChannelSelection => {
                self.search.search(&mut self.channel_selection, &self.input);
            }
            InputMode::FormatSelection => self
                .search
                .search(self.stream_formats.get_mut_selected_tab(), &self.input),
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
                        self.search.repeat_last(videos, opposite)
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

    fn update_search_on_delete(&mut self) {
        self.search.state = SearchState::PoppedKey;
        self.search_in_selected();
    }

    pub fn push_key(&mut self, c: char) {
        if self.input_idx == self.input.len() {
            self.input.push(c);
        } else {
            self.input.insert(self.input_idx, c);
            if let InputMode::Search = self.input_mode {
                self.search.state = SearchState::PoppedKey;
            }
        }
        if let InputMode::Search = self.input_mode {
            self.search_in_selected();
            self.search.state = SearchState::PushedKey;
        }
        self.input_idx += c.len_utf8();
        self.cursor_position += c.width().unwrap() as u16;
    }

    pub fn pop_key(&mut self) {
        if self.input_idx != 0 {
            let (idx, ch) = self.input[..self.input_idx]
                .grapheme_indices(true)
                .next_back()
                .unwrap();
            self.cursor_position -= ch.width() as u16;
            self.input.drain(idx..self.input_idx);
            self.input_idx = idx;
            if let InputMode::Search = self.input_mode {
                self.update_search_on_delete();
            }
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.input_idx != 0 {
            let (idx, ch) = self.input[..self.input_idx]
                .grapheme_indices(true)
                .next_back()
                .unwrap();
            self.input_idx = idx;
            self.cursor_position -= ch.width() as u16;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.input_idx != self.input.len() {
            let (idx, ch) = self.input[self.input_idx..]
                .grapheme_indices(true)
                .next()
                .map(|(idx, ch)| (self.input_idx + idx + ch.len(), ch))
                .unwrap();
            self.input_idx = idx;
            self.cursor_position += ch.width() as u16;
        }
    }

    pub fn move_cursor_one_word_left(&mut self) {
        let idx = self.input[..self.input_idx]
            .unicode_word_indices()
            .next_back()
            .map_or(0, |(idx, _)| idx);
        self.cursor_position -= self.input[idx..self.input_idx].width() as u16;
        self.input_idx = idx;
    }

    pub fn move_cursor_one_word_right(&mut self) {
        let old_idx = self.input_idx;
        self.input_idx = self.input[self.input_idx..]
            .unicode_word_indices()
            .nth(1)
            .map_or(self.input.len(), |(idx, _)| self.input_idx + idx);
        self.cursor_position += self.input[old_idx..self.input_idx].width() as u16;
    }

    pub fn move_cursor_to_beginning_of_line(&mut self) {
        self.input_idx = 0;
        self.cursor_position = 0;
    }

    pub fn move_cursor_to_end_of_line(&mut self) {
        self.input_idx = self.input.len();
        self.cursor_position = self.input.width() as u16;
    }

    pub fn delete_word_before_cursor(&mut self) {
        let old_idx = self.input_idx;
        self.move_cursor_one_word_left();
        self.input.drain(self.input_idx..old_idx);
        if let InputMode::Search = self.input_mode {
            self.update_search_on_delete();
        }
    }

    pub fn clear_line(&mut self) {
        self.input.clear();
        self.input_idx = 0;
        self.cursor_position = 0;
        if let InputMode::Search = self.input_mode {
            self.update_search_on_delete();
        }
    }

    pub fn clear_to_right(&mut self) {
        self.input.drain(self.input_idx..);
        if let InputMode::Search = self.input_mode {
            self.update_search_on_delete();
        }
    }

    pub fn no_search_pattern_match(&self) -> bool {
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
                            self.search.recover_item(videos)
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
            import::Format::YoutubeCsv => import::YoutubeCsv::read_subscriptions(path),
            import::Format::NewPipe => import::NewPipe::read_subscriptions(path),
        }
        .with_context(|| "Failed to import")?;

        import_state = import_state
            .drain(..)
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

    pub fn export_subscriptions(&mut self, path: &Path, format: import::Format) -> Result<()> {
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
                            Some(time) if utils::time_passed(time).is_ok_and(|t| t < OPTIONS.refresh_threshold)
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
        }

        self.dispatch(IoEvent::RefreshChannels(ids));
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

    pub fn toggle_tag_selection(&mut self) {
        if let InputMode::Tag = self.input_mode {
            self.input_mode = InputMode::Normal;
        } else {
            self.input_mode = InputMode::Tag;
        }
    }

    pub fn enter_tag_creation(&mut self) {
        self.prev_input_mode = self.input_mode.clone();
        self.input_mode = InputMode::TagCreation;
        self.message.clear_message();
        self.input_idx = 0;
        self.cursor_position = 0;
    }

    pub fn enter_tag_renaming(&mut self) {
        if let Some(tag) = self.tags.get_selected() {
            self.prev_input_mode = self.input_mode.clone();
            self.input_mode = InputMode::TagRenaming;
            self.message.clear_message();
            self.input = tag.item.clone();
            self.input_idx = self.input.len();
            self.cursor_position = self.input.width() as u16;
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
        if let Err(e) = database::create_tag(&self.conn, &self.input) {
            self.set_error_message(&e.to_string());
        } else {
            self.tags.items.push(SelectionItem::new(self.input.clone()));
        }

        self.input_mode = InputMode::Tag;
        self.input.clear();
    }

    pub fn rename_selected_tag(&mut self) {
        if let Some(tag) = self.tags.get_mut_selected() {
            if let Err(e) = database::rename_tag(&self.conn, &tag.item, &self.input) {
                self.set_error_message(&e.to_string());
            } else {
                self.input.clone_into(&mut tag.item);
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

pub trait State {
    fn select(&mut self, index: Option<usize>);
    fn selected(&self) -> Option<usize>;
}

impl State for ListState {
    fn select(&mut self, index: Option<usize>) {
        self.select(index);
    }

    fn selected(&self) -> Option<usize> {
        self.selected()
    }
}

impl State for TableState {
    fn select(&mut self, index: Option<usize>) {
        self.select(index);
    }

    fn selected(&self) -> Option<usize> {
        self.selected()
    }
}

pub struct StatefulList<T, S: State> {
    pub state: S,
    pub items: Vec<T>,
}

impl<T, S: State + Default> Default for StatefulList<T, S> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            items: Vec::default(),
        }
    }
}

impl<T, S: State + Default> StatefulList<T, S> {
    pub fn with_items(items: Vec<T>) -> StatefulList<T, S> {
        let mut stateful_list = StatefulList {
            state: Default::default(),
            items,
        };

        stateful_list.select_first();

        stateful_list
    }
}

impl<T, S: State> StatefulList<T, S> {
    fn select_with_index(&mut self, index: usize) {
        self.state.select(if self.items.is_empty() {
            None
        } else {
            Some(index)
        });
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.select_with_index(i);
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.select_with_index(i);
    }

    pub fn select_first(&mut self) {
        self.select_with_index(0);
    }

    pub fn select_last(&mut self) {
        self.select_with_index(self.items.len().checked_sub(1).unwrap_or_default());
    }

    fn reset_state(&mut self) {
        self.state
            .select(if self.items.is_empty() { None } else { Some(0) });
    }

    pub fn get_selected(&self) -> Option<&T> {
        self.state.selected().and_then(|idx| self.items.get(idx))
    }

    pub fn get_mut_selected(&mut self) -> Option<&mut T> {
        self.state
            .selected()
            .and_then(|idx| self.items.get_mut(idx))
    }

    fn check_bounds(&mut self) {
        if let Some(idx) = self.state.selected() {
            if self.items.is_empty() {
                self.state.select(None);
            } else if idx >= self.items.len() {
                self.select_last();
            }
        }
    }
}

impl<T: ListItem, S: State> StatefulList<T, S> {
    pub fn find_by_id(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|item| item.id() == id)
    }

    pub fn get_mut_by_id(&mut self, id: &str) -> Option<&mut T> {
        self.find_by_id(id).map(|index| &mut self.items[index])
    }
}

impl<T, S: State + Default> From<Vec<T>> for StatefulList<T, S> {
    fn from(v: Vec<T>) -> Self {
        StatefulList::with_items(v)
    }
}

pub enum Selected {
    Channels,
    Videos,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Mode {
    Subscriptions,
    LatestVideos,
}

#[derive(Deserialize)]
#[serde(rename_all(deserialize = "lowercase"))]
pub enum VideoPlayer {
    Mpv,
    Vlc,
}

type TabList = Vec<(Vec<Video>, ChannelTab)>;

pub struct Tab {
    pub variant: ChannelTab,
    pub videos: StatefulList<Video, TableState>,
    pub has_new_video: bool,
}

impl Tab {
    pub fn new(variant: ChannelTab, videos: Vec<Video>) -> Self {
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

    fn get_videos_mut(&mut self) -> Option<&mut StatefulList<Video, TableState>> {
        self.get_mut_selected().map(|tab| &mut tab.videos)
    }

    pub fn get_selected_video(&self) -> Option<&Video> {
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

pub struct SelectionItem<T> {
    pub selected: bool,
    pub item: T,
}

impl<T> SelectionItem<T> {
    pub fn new(item: T) -> Self {
        Self {
            selected: false,
            item,
        }
    }

    pub fn toggle(&mut self) {
        self.selected = !self.selected;
    }
}

impl<T: Display> Display for SelectionItem<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {}",
            if self.selected { "*" } else { " " },
            self.item
        )
    }
}

impl<T: ListItem> ListItem for SelectionItem<T> {
    fn id(&self) -> &str {
        self.item.id()
    }
}

impl<T> Deref for SelectionItem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<T> DerefMut for SelectionItem<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

pub struct SelectionList<T: ListItem>(StatefulList<SelectionItem<T>, ListState>);

impl<T: ListItem> SelectionList<T> {
    pub fn new(items: Vec<T>) -> Self {
        let items = items.into_iter().map(SelectionItem::new).collect();

        Self(StatefulList::with_items(items))
    }

    pub fn toggle_selected(&mut self) {
        if let Some(item) = self.get_mut_selected() {
            item.toggle();
        }
    }

    pub fn select(&mut self) {
        if let Some(item) = self.items.iter_mut().find(|item| item.selected) {
            item.selected = false;
        }

        self.toggle_selected();
    }

    pub fn select_all(&mut self) {
        self.items.iter_mut().for_each(|item| item.selected = true);
    }

    pub fn deselect_all(&mut self) {
        self.items.iter_mut().for_each(|item| item.selected = false);
    }

    pub fn get_selected_items(&self) -> Vec<&T> {
        self.items
            .iter()
            .filter(|item| item.selected)
            .map(|item| &item.item)
            .collect()
    }

    pub fn get_selected_item(&self) -> &T {
        self.items.iter().find(|item| item.selected).unwrap()
    }
}

impl<T: ListItem> Deref for SelectionList<T> {
    type Target = StatefulList<SelectionItem<T>, ListState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ListItem> DerefMut for SelectionList<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: ListItem> Default for SelectionList<T> {
    fn default() -> Self {
        Self(StatefulList::with_items(Vec::default()))
    }
}
