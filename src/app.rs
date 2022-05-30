use crate::channel::{Channel, ListItem, RefreshState, Video, VideoType};
use crate::help::HelpWindowState;
use crate::input::InputMode;
use crate::message::Message;
use crate::search::{Search, SearchDirection, SearchState};
use crate::{database, OPTIONS};
use crate::{utils, IoEvent};
use anyhow::{Context, Result};
use rand::prelude::*;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tui::widgets::{ListState, TableState};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use ureq::{Agent, AgentBuilder};

pub struct App {
    pub channels: StatefulList<Channel, ListState>,
    pub videos: StatefulList<Video, TableState>,
    pub selected: Selected,
    pub mode: Mode,
    pub conn: Connection,
    pub message: Message,
    pub input: String,
    pub input_mode: InputMode,
    pub input_idx: usize,
    pub cursor_position: u16,
    pub help_window_state: HelpWindowState,
    new_video_ids: HashSet<String>,
    search: Search,
    instance: Instance,
    pub hide_watched: bool,
    io_tx: Sender<IoEvent>,
}

impl App {
    pub fn new(io_tx: Sender<IoEvent>) -> Result<Self> {
        let mut app = Self {
            channels: StatefulList::with_items(Default::default()),
            videos: StatefulList::with_items(Default::default()),
            selected: Selected::Channels,
            mode: Mode::Subscriptions,
            conn: Connection::open(OPTIONS.database.clone())?,
            message: Message::new(),
            input: Default::default(),
            input_mode: InputMode::Normal,
            input_idx: 0,
            cursor_position: 0,
            search: Default::default(),
            instance: Instance::new()?,
            new_video_ids: Default::default(),
            hide_watched: OPTIONS.hide_watched,
            io_tx,
            help_window_state: HelpWindowState::new(),
        };

        database::initialize_db(&app.conn)?;
        app.set_mode_subs();
        app.load_channels()?;
        app.select_first();

        Ok(app)
    }

    pub fn add_channel(&mut self, mut videos_json: Value) {
        let channel_id: String = videos_json
            .get("authorId")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let channel_name: String = videos_json
            .get("author")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let channel = Channel::new(channel_id.clone(), channel_name);
        if let Err(e) = database::create_channel(&self.conn, &channel) {
            self.set_error_message(&e.to_string());
            return;
        };
        self.channels.items.push(channel);
        let latest_videos = videos_json["latestVideos"].take();
        self.add_videos(latest_videos, &channel_id);
    }

    pub fn add_videos(&mut self, videos_json: Value, channel_id: &str) {
        let videos: Vec<Video> = Video::vec_from_json(videos_json);
        let new_video_count = match database::add_videos(&self.conn, channel_id, &videos) {
            Ok(new_video_count) => new_video_count,
            Err(e) => {
                self.set_error_message(&e.to_string());
                return;
            }
        };
        if new_video_count > 0 {
            self.move_channel_to_top(channel_id);
            let ids =
                database::get_newly_inserted_video_ids(&self.conn, channel_id, new_video_count)
                    .unwrap_or_default();
            self.new_video_ids.extend(ids);
            self.reload_videos();
        }
    }

    fn move_channel_to_top(&mut self, channel_id: &str) {
        let id_of_current_channel = self
            .get_current_channel()
            .map(|channel| channel.channel_id.clone());
        let index = self.channels.find_by_id(channel_id).unwrap();
        let mut channel = self.channels.items.remove(index);
        channel.new_video |= true;
        self.channels.items.insert(0, channel);
        if let Some(id) = id_of_current_channel {
            let index = self.channels.find_by_id(&id).unwrap();
            self.channels.select_with_index(index);
        }
    }

    pub fn load_channels(&mut self) -> Result<()> {
        self.channels = database::get_channels(&self.conn)?.into();
        Ok(())
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

    pub fn instance(&self) -> Instance {
        self.instance.clone()
    }

    fn get_channel_by_id(&mut self, channel_id: &str) -> Option<&mut Channel> {
        self.channels
            .find_by_id(channel_id)
            .map(|index| &mut self.channels.items[index])
    }

    fn find_channel_by_name(&mut self, channel_name: &str) -> Option<usize> {
        self.channels
            .items
            .iter()
            .position(|channel| channel.channel_name == channel_name)
    }

    pub fn start_refreshing_channel(&mut self, channel_id: &str) {
        self.get_channel_by_id(channel_id).unwrap().refresh_state = RefreshState::Refreshing;
    }

    pub fn complete_refreshing_channel(&mut self, channel_id: &str) {
        self.get_channel_by_id(channel_id).unwrap().refresh_state = RefreshState::Completed;
    }

    pub fn refresh_failed(&mut self, channel_id: &str) {
        self.get_channel_by_id(channel_id).unwrap().refresh_state = RefreshState::Failed;
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
                    println!("{}", e);
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
                self.set_error_message(&format!("couldn't run \"{}\": {}", video_player, e));
            } else {
                self.mark_as_watched();
            }
        }
    }

    pub fn open_in_browser(&mut self) {
        let url = match self.selected {
            Selected::Channels => match self.get_current_channel() {
                Some(current_channel) => format!(
                    "{}/channel/{}",
                    self.instance.domain, current_channel.channel_id
                ),
                None => return,
            },
            Selected::Videos => match self.get_current_video() {
                Some(current_video) => format!(
                    "{}/watch?v={}",
                    self.instance.domain, current_video.video_id
                ),
                None => return,
            },
        };

        let browser_process = || webbrowser::open(&url);

        #[cfg(unix)]
        let res = self.run_detached(browser_process);
        #[cfg(not(unix))]
        let res = browser_process();

        if let Err(e) = res {
            self.set_error_message(&format!("{}", e));
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
        database::get_latest_videos(&self.conn)
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
                if let VideoType::LatestVideos(channel_name) = video.video_type.as_ref().unwrap() {
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
        !matches!(self.input_mode, InputMode::Normal | InputMode::Confirmation)
            || !self.message.is_empty()
    }

    pub fn toggle_help(&mut self) {
        self.help_window_state.toggle();
    }

    pub fn prompt_for_subscription(&mut self) {
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
        if let Some(index) = self.channels.state.selected() {
            let channel_id = &self.get_current_channel().unwrap().channel_id;
            database::delete_channel(&self.conn, channel_id).unwrap();
            self.input_mode = InputMode::Normal;
            self.channels.items.remove(index);
            let length = self.channels.items.len();
            if length == 0 {
                self.channels.state.select(None);
            } else if index == length {
                self.channels.previous();
            }
            self.on_change_channel();
        }
    }

    fn start_searching(&mut self) {
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

    pub fn search_in_block(&mut self) {
        match self.selected {
            Selected::Channels => {
                self.search.search(&mut self.channels, &self.input);
                self.on_change_channel()
            }
            Selected::Videos => self.search.search(&mut self.videos, &self.input),
        }
    }

    fn repeat_last_search_helper(&mut self, opposite: bool) {
        match self.selected {
            Selected::Channels => {
                self.search.repeat_last(&mut self.channels, opposite);
                self.on_change_channel()
            }
            Selected::Videos => self.search.repeat_last(&mut self.videos, opposite),
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
        self.search_in_block();
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
            self.search_in_block();
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
        self.input_mode = InputMode::Normal;
        self.input.clear();
        self.search.complete_search(abort);
    }

    fn recover_item(&mut self) {
        if self.search.recovery_index.is_some() {
            match self.selected {
                Selected::Channels => {
                    self.search.recover_item(&mut self.channels);
                    self.on_change_channel()
                }
                Selected::Videos => self.search.recover_item(&mut self.videos),
            }
        }
    }

    pub fn abort_search(&mut self) {
        self.recover_item();
        self.finalize_search(true);
    }

    fn dispatch(&mut self, action: IoEvent) {
        if let Err(e) = self.io_tx.send(action) {
            self.set_error_message(&format!("Error from dispatch: {}", e));
        }
    }

    pub fn subscribe_to_channel(&mut self, channel_id: String) {
        self.dispatch(IoEvent::SubscribeToChannel(channel_id));
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

    pub fn refresh_failed_channels(&mut self) {
        match Instance::new() {
            Ok(instance) => self.instance = instance,
            Err(e) => {
                self.set_error_message(&format!("Couldn't change instance: {}", e));
                return;
            }
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

    pub fn clear_message_after_duration(&mut self, duration_seconds: u64) {
        self.dispatch(IoEvent::ClearMessage(duration_seconds));
    }
}

#[derive(Clone)]
pub struct Instance {
    pub domain: String,
    agent: Agent,
}

impl Instance {
    pub fn new() -> Result<Self> {
        let invidious_instances = match utils::read_instances() {
            Ok(instances) => instances,
            Err(_) => {
                utils::fetch_invidious_instances().with_context(|| "No instances available")?
            }
        };
        let mut rng = thread_rng();
        let domain = invidious_instances[rng.gen_range(0..invidious_instances.len())].to_string();
        let agent = AgentBuilder::new()
            .timeout(Duration::from_secs(OPTIONS.request_timeout))
            .build();
        Ok(Self { domain, agent })
    }

    pub fn get_videos_of_channel(&self, channel_id: &str) -> Result<Value> {
        let url = format!("{}/api/v1/channels/{}", self.domain, channel_id);
        Ok(self
            .agent
            .get(&url)
            .query(
                "fields",
                "author,authorId,latestVideos(title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp)",
            )
            .call()?
            .into_json()?)
    }

    pub fn get_latest_videos_of_channel(&self, channel_id: &str) -> Result<Value> {
        let url = format!("{}/api/v1/channels/latest/{}", self.domain, channel_id);
        Ok(self
            .agent
            .get(&url)
            .query(
                "fields",
                "title,videoId,published,lengthSeconds,isUpcoming,premiereTimestamp",
            )
            .call()?
            .into_json()?)
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
    fn with_items(items: Vec<T>) -> StatefulList<T, S> {
        StatefulList {
            state: Default::default(),
            items,
        }
    }

    fn select_with_index(&mut self, index: usize) {
        self.state.select(if self.items.is_empty() {
            None
        } else {
            Some(index)
        })
    }

    fn next(&mut self) {
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

    fn previous(&mut self) {
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

    fn select_first(&mut self) {
        self.select_with_index(0);
    }

    fn select_last(&mut self) {
        self.select_with_index(self.items.len().checked_sub(1).unwrap_or_default());
    }

    fn reset_state(&mut self) {
        self.state
            .select(if self.items.is_empty() { None } else { Some(0) });
    }

    fn get_selected(&self) -> Option<&T> {
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
}

impl<T: ListItem, S: State> StatefulList<T, S> {
    fn find_by_id(&mut self, id: &str) -> Option<usize> {
        self.items.iter().position(|item| item.id() == id)
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

#[derive(PartialEq, Clone, Debug)]
pub enum Mode {
    Subscriptions,
    LatestVideos,
}
