use crate::api::invidious::Instance;
use crate::api::local::Local;
use crate::api::{Api, ApiBackend, ChannelFeed};
use crate::channel::{Channel, ListItem, RefreshState, Video};
use crate::help::HelpWindowState;
use crate::import::{self, ImportItem};
use crate::input::InputMode;
use crate::message::Message;
use crate::search::{Search, SearchDirection, SearchState};
use crate::{database, IoEvent, CLAP_ARGS, OPTIONS};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashSet;
use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use tui::widgets::{ListState, TableState};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl ListItem for String {
    fn id(&self) -> &str {
        self
    }
}

pub struct App {
    pub channels: StatefulList<Channel, ListState>,
    pub videos: StatefulList<Video, TableState>,
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
    pub invidious_instances: Option<Vec<String>>,
    invidious_instance: Option<Instance>,
    local_api: Local,
    pub selected_api: ApiBackend,
    pub hide_watched: bool,
    io_tx: Sender<IoEvent>,
    pub channel_selection: SelectionList<Channel>,
}

impl App {
    pub fn new(io_tx: Sender<IoEvent>) -> Result<Self> {
        let mut app = Self {
            channels: StatefulList::with_items(Default::default()),
            videos: StatefulList::with_items(Default::default()),
            tags: Default::default(),
            selected: Selected::Channels,
            mode: Mode::Subscriptions,
            conn: Connection::open(OPTIONS.database.clone())?,
            message: Message::new(),
            input: Default::default(),
            input_mode: InputMode::Normal,
            input_idx: 0,
            prev_input_mode: InputMode::Normal,
            cursor_position: 0,
            search: Default::default(),
            invidious_instances: crate::utils::read_instances().ok(),
            invidious_instance: None,
            local_api: Local::new(),
            selected_api: OPTIONS.api.clone(),
            new_video_ids: Default::default(),
            channels_with_new_videos: Default::default(),
            hide_watched: OPTIONS.hide_watched,
            io_tx,
            help_window_state: HelpWindowState::new(),
            import_state: SelectionList::default(),
            channel_selection: Default::default(),
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

        database::initialize_db(&app.conn)?;
        app.set_mode_subs();
        app.load_channels();
        app.on_change_channel();

        if let ApiBackend::Invidious = app.selected_api {
            app.set_instance();
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
        };
        self.channels.items.push(channel);
        self.add_videos(channel_feed);
    }

    pub fn add_videos(&mut self, mut channel_feed: ChannelFeed) {
        let channel_id = channel_feed.channel_id.as_ref().unwrap();

        let present_videos: Vec<Video> = match database::get_videos(&self.conn, channel_id) {
            Ok(videos) => videos,
            Err(e) => {
                self.set_error_message(&e.to_string());
                return;
            }
        };

        let mut videos = Vec::new();
        let mut added_new_video = false;

        for video in channel_feed.videos.drain(..) {
            if let Some(p_video) = present_videos
                .iter()
                .find(|p_video| p_video.video_id == video.video_id)
            {
                if p_video.length.is_none() && video.length.is_some()
                    || matches!(p_video.length, Some(length) if length == 0)
                        && matches!(video.length, Some(length) if length != 0)
                {
                    videos.push(video);
                }
            } else {
                self.new_video_ids.insert(video.video_id.clone());
                videos.push(video);
                added_new_video = true;
            }
        }

        if let Err(e) = database::add_videos(&self.conn, channel_id, &videos) {
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
            self.load_videos();
        }
    }

    pub fn delete_selected_video(&mut self) {
        if let Some(idx) = self.videos.state.selected() {
            if let Err(e) = database::delete_video(&self.conn, &self.videos.items[idx].video_id) {
                self.set_error_message(&e.to_string());
            }
            self.videos.items.remove(idx);
            self.videos.check_bounds();
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
            };
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
            self.load_videos();
            self.select_first();
        }
    }

    pub fn instance(&self) -> Box<dyn Api> {
        match self.selected_api {
            ApiBackend::Invidious => Box::new(self.invidious_instance.as_ref().unwrap().clone()),
            ApiBackend::Local => Box::new(self.local_api.clone()),
        }
    }

    pub fn switch_api(&mut self) {
        self.selected_api = match self.selected_api {
            ApiBackend::Local => {
                if self.invidious_instance.is_none() {
                    self.set_instance();
                }
                ApiBackend::Invidious
            }
            _ => ApiBackend::Local,
        };

        self.set_message_with_default_duration(&format!("Selected API: {}", self.selected_api));
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
        self.videos.get_selected()
    }

    fn get_mut_current_video(&mut self) -> Option<&mut Video> {
        self.videos.get_mut_selected()
    }

    fn set_watched(&mut self, is_watched: bool) {
        if let Some(current_video) = self.get_mut_current_video() {
            current_video.watched = is_watched;
            if let Err(e) = database::set_watched_field(
                &self.conn,
                &self.get_current_video().unwrap().video_id,
                is_watched,
            ) {
                self.set_error_message(&e.to_string())
            }
        }
    }

    pub fn mark_as_watched(&mut self) {
        self.set_watched(true);
    }

    fn mark_as_unwatched(&mut self) {
        self.set_watched(false);
    }

    pub fn toggle_watched(&mut self) {
        if let Some(video) = self.get_current_video() {
            if video.watched {
                self.mark_as_unwatched();
            } else {
                self.mark_as_watched();
            }
        }
    }

    pub fn toggle_hide(&mut self) {
        self.hide_watched = !self.hide_watched;
        self.reload_videos();
    }

    #[cfg(unix)]
    fn run_detached<F: FnOnce() -> Result<(), std::io::Error>>(&mut self, func: F) -> Result<()> {
        use nix::sys::wait::{wait, WaitStatus};
        use nix::unistd::ForkResult::{Child, Parent};
        use nix::unistd::{close, dup2, fork, pipe, setsid};
        use std::fs::File;
        use std::io::prelude::*;
        use std::os::unix::io::{FromRawFd, IntoRawFd};

        let (pipe_r, pipe_w) = pipe().unwrap();
        let pid = unsafe { fork().unwrap() };
        match pid {
            Parent { .. } => {
                // check if child process failed to run
                if let Ok(WaitStatus::Exited(_, 101)) = wait() {
                    close(pipe_w)?;
                    let mut file = unsafe { File::from_raw_fd(pipe_r) };
                    let mut error_message = String::new();
                    file.read_to_string(&mut error_message)?;
                    close(pipe_r)?;
                    Err(anyhow::anyhow!(error_message))
                } else {
                    Ok(())
                }
            }
            Child => {
                setsid().unwrap();
                let fd = std::fs::OpenOptions::new()
                    .write(true)
                    .read(true)
                    .open("/dev/null")
                    .unwrap()
                    .into_raw_fd();
                dup2(fd, 0).unwrap();
                dup2(fd, 1).unwrap();
                dup2(fd, 2).unwrap();
                if let Err(e) = func() {
                    close(pipe_r).unwrap();
                    dup2(pipe_w, 1).unwrap();
                    println!("{e}");
                    close(pipe_w).unwrap();
                    std::process::exit(101);
                }
                std::process::exit(0);
            }
        }
    }

    pub fn play_video(&mut self) {
        if let Some(current_video) = self.get_current_video() {
            let url = format!(
                "{}/watch?v={}",
                "https://www.youtube.com", current_video.video_id
            );
            let video_player = &OPTIONS.video_player;
            let video_player_process = || {
                std::process::Command::new(video_player)
                    .arg(url)
                    .spawn()
                    .map(|_| ())
            };

            #[cfg(unix)]
            let res = self.run_detached(video_player_process);
            #[cfg(not(unix))]
            let res = video_player_process();

            if let Err(e) = res {
                self.set_error_message(&format!("couldn't run \"{video_player}\": {e}"));
            } else {
                self.mark_as_watched();
            }
        }
    }

    pub fn open_in_invidious(&mut self) {
        let Some(instance) = &self.invidious_instance else {
            self.set_error_message("No Invidious instances available.");
            return;
        };

        let url = match self.selected {
            Selected::Channels => match self.get_current_channel() {
                Some(current_channel) => {
                    format!("{}/channel/{}", instance.domain, current_channel.channel_id)
                }
                None => return,
            },
            Selected::Videos => match self.get_current_video() {
                Some(current_video) => {
                    format!("{}/watch?v={}", instance.domain, current_video.video_id)
                }
                None => return,
            },
        };

        self.open_in_browser(&url);
    }

    pub fn open_in_youtube(&mut self) {
        const YOUTUBE_URL: &str = "https://www.youtube.com";

        let url = match self.selected {
            Selected::Channels => match self.get_current_channel() {
                Some(current_channel) => {
                    format!("{}/channel/{}", YOUTUBE_URL, current_channel.channel_id)
                }
                None => return,
            },
            Selected::Videos => match self.get_current_video() {
                Some(current_video) => {
                    format!("{}/watch?v={}", YOUTUBE_URL, current_video.video_id)
                }
                None => return,
            },
        };

        self.open_in_browser(&url);
    }

    pub fn open_in_browser(&mut self, url: &str) {
        let browser_process = || webbrowser::open(url);

        #[cfg(unix)]
        let res = self.run_detached(browser_process);
        #[cfg(not(unix))]
        let res = browser_process();

        if let Err(e) = res {
            self.set_error_message(&format!("{e}"));
        } else if matches!(self.selected, Selected::Videos) {
            self.mark_as_watched();
        }
    }

    fn get_videos_of_current_channel(&self) -> Result<Vec<Video>> {
        if let Some(channel) = self.get_current_channel() {
            database::get_videos(&self.conn, &channel.channel_id)
        } else {
            Ok(Vec::new())
        }
    }

    fn get_latest_videos(&self) -> Result<Vec<Video>> {
        let selected_tags: Vec<&str> = self
            .tags
            .get_selected_items()
            .iter()
            .map(|tag| tag.as_str())
            .collect();

        database::get_latest_videos(&self.conn, &selected_tags)
    }

    pub fn load_videos(&mut self) {
        let videos = match self.mode {
            Mode::Subscriptions => self.get_videos_of_current_channel(),
            Mode::LatestVideos => self.get_latest_videos(),
        };
        match videos {
            Ok(videos) => {
                self.videos.items = if self.hide_watched {
                    videos.into_iter().filter(|video| !video.watched).collect()
                } else {
                    videos
                };

                let mut count = 0;
                for video in &mut self.videos.items {
                    if self.new_video_ids.contains(&video.video_id) {
                        video.new = true;
                        count += 1;
                    }
                    if count == self.new_video_ids.len() {
                        break;
                    }
                }
            }
            Err(e) => {
                self.videos.items.clear();
                self.set_error_message(&e.to_string());
            }
        }
    }

    pub fn reload_videos(&mut self) {
        let current_video = self.get_current_video();

        let id_of_current_video = match current_video {
            Some(current_video) if self.hide_watched && current_video.watched => {
                // if the currently selected video is watched, jump to the first unwatched video above
                let mut index = self.videos.state.selected().unwrap();
                loop {
                    if let Some(i) = index.checked_sub(1) {
                        index = i;
                    } else {
                        break None;
                    }

                    let video = &self.videos.items[index];
                    if !video.watched {
                        break Some(video.video_id.clone());
                    }
                }
            }
            Some(current_video) => Some(current_video.video_id.clone()),
            None => None,
        };

        self.load_videos();

        match id_of_current_video {
            Some(id) => {
                let index = self.videos.find_by_id(&id).unwrap();
                self.videos.select_with_index(index);
            }
            None => self.videos.reset_state(),
        }
    }

    pub fn on_change_channel(&mut self) {
        self.load_videos();
        self.videos.reset_state();
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
            Selected::Videos => self.videos.next(),
        }
    }

    pub fn on_up(&mut self) {
        match self.selected {
            Selected::Channels => {
                self.channels.previous();
                self.on_change_channel();
            }
            Selected::Videos => self.videos.previous(),
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
                self.videos.select_first();
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
                self.videos.select_last();
            }
        }
    }

    pub fn jump_to_channel(&mut self) {
        if let Mode::LatestVideos = self.mode {
            if let Some(video) = self.get_current_video() {
                if let Some(channel_name) = &video.channel_name {
                    let channel_name = channel_name.clone();
                    let index = self.find_channel_by_name(&channel_name).unwrap();
                    self.mode = Mode::Subscriptions;
                    self.selected = Selected::Videos;
                    self.channels.select_with_index(index);
                    self.on_change_channel();
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
        let channel_id = if self.input.contains('/') {
            self.input
                .rsplit_once('/')
                .map(|(_, id)| id.to_owned())
                .unwrap()
        } else {
            self.input.drain(..).collect()
        };
        self.input_mode = InputMode::Normal;
        self.input.clear();
        self.subscribe_to_channel(channel_id);
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
                    self.on_change_channel()
                }
                Selected::Videos => self.search.search(&mut self.videos, &self.input),
            },
            InputMode::Import => self.search.search(&mut self.import_state, &self.input),
            InputMode::Tag => self.search.search(&mut self.tags, &self.input),
            InputMode::ChannelSelection => {
                self.search.search(&mut self.channel_selection, &self.input)
            }
            _ => panic!(),
        }
    }

    fn repeat_last_search_helper(&mut self, opposite: bool) {
        match self.input_mode {
            InputMode::Normal => match self.selected {
                Selected::Channels => {
                    self.search.repeat_last(&mut self.channels, opposite);
                    self.on_change_channel()
                }
                Selected::Videos => self.search.repeat_last(&mut self.videos, opposite),
            },
            InputMode::Import => self.search.repeat_last(&mut self.import_state, opposite),
            InputMode::Tag => self.search.repeat_last(&mut self.tags, opposite),
            InputMode::ChannelSelection => self
                .search
                .repeat_last(&mut self.channel_selection, opposite),
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
                .last()
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
                .last()
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
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        self.cursor_position -= self.input[idx..self.input_idx].width() as u16;
        self.input_idx = idx;
    }

    pub fn move_cursor_one_word_right(&mut self) {
        let old_idx = self.input_idx;
        self.input_idx = self.input[self.input_idx..]
            .unicode_word_indices()
            .nth(1)
            .map(|(idx, _)| self.input_idx + idx)
            .unwrap_or(self.input.len());
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
                        self.on_change_channel()
                    }
                    Selected::Videos => self.search.recover_item(&mut self.videos),
                },
                InputMode::Import => self.search.recover_item(&mut self.import_state),
                InputMode::Tag => self.search.recover_item(&mut self.tags),
                InputMode::ChannelSelection => {
                    self.search.recover_item(&mut self.channel_selection)
                }
                _ => panic!(),
            }
        }
    }

    pub fn abort_search(&mut self) {
        self.recover_item();
        self.finalize_search(true);
    }

    pub fn select_channels_to_import(
        &mut self,
        path: PathBuf,
        format: import::Format,
    ) -> Result<()> {
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

    pub fn import_subscriptions(&mut self) {
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

        self.subscribe_to_channels();
    }

    pub fn export_subscriptions(&mut self, path: PathBuf, format: import::Format) -> Result<()> {
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

    pub fn subscribe_to_channel(&mut self, channel_id: String) {
        self.dispatch(IoEvent::SubscribeToChannel(channel_id));
    }

    pub fn subscribe_to_channels(&mut self) {
        self.dispatch(IoEvent::SubscribeToChannels);
    }

    pub fn refresh_channel(&mut self) {
        if let Some(current_channel) = self.get_current_channel() {
            let channel_id = current_channel.channel_id.clone();
            self.dispatch(IoEvent::RefreshChannel(channel_id));
        }
    }

    pub fn refresh_channels(&mut self) {
        self.dispatch(IoEvent::RefreshChannels(false));
    }

    pub fn set_instance(&mut self) {
        if let Some(invidious_instances) = &self.invidious_instances {
            self.invidious_instance = Some(Instance::new(invidious_instances));
        } else {
            self.dispatch(IoEvent::FetchInstances);
        }
    }

    pub fn refresh_failed_channels(&mut self) {
        if let ApiBackend::Invidious = self.selected_api {
            self.set_instance();
        }

        self.dispatch(IoEvent::RefreshChannels(true));
    }

    pub fn set_message(&mut self, message: &str) {
        self.message.set_message(message);
    }

    pub fn set_message_with_default_duration(&mut self, message: &str) {
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
        self.dispatch(IoEvent::ClearMessage(duration_seconds));
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
                tag.item = self.input.clone();
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

impl<T, S: State + Default> StatefulList<T, S> {
    pub fn with_items(items: Vec<T>) -> StatefulList<T, S> {
        let mut stateful_list = StatefulList {
            state: Default::default(),
            items,
        };

        stateful_list.select_first();

        stateful_list
    }

    fn select_with_index(&mut self, index: usize) {
        self.state.select(if self.items.is_empty() {
            None
        } else {
            Some(index)
        })
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
        match self.state.selected() {
            Some(i) => Some(&self.items[i]),
            None => None,
        }
    }

    fn get_mut_selected(&mut self) -> Option<&mut T> {
        match self.state.selected() {
            Some(i) => Some(&mut self.items[i]),
            None => None,
        }
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
    pub fn find_by_id(&mut self, id: &str) -> Option<usize> {
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

pub struct SelectionItem<T: ListItem> {
    selected: bool,
    pub item: T,
}

impl<T: ListItem> SelectionItem<T> {
    fn new(item: T) -> Self {
        Self {
            selected: false,
            item,
        }
    }

    fn toggle(&mut self) {
        self.selected = !self.selected;
    }
}

impl<T: Display + ListItem> Display for SelectionItem<T> {
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

impl<T: ListItem> Deref for SelectionItem<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<T: ListItem> DerefMut for SelectionItem<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

pub struct SelectionList<T: ListItem>(StatefulList<SelectionItem<T>, ListState>);

impl<T: ListItem> SelectionList<T> {
    fn new(items: Vec<T>) -> Self {
        let items = items.into_iter().map(SelectionItem::new).collect();

        Self(StatefulList::with_items(items))
    }

    pub fn toggle_selected(&mut self) {
        if let Some(item) = self.get_mut_selected() {
            item.toggle();
        }
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
        Self(StatefulList::with_items(Default::default()))
    }
}
