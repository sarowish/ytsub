use crate::channel::{Channel, RefreshState, Video, VideoType};
use crate::input::InputMode;
use crate::search::{Search, SearchDirection, SearchState};
use crate::{database, Options};
use crate::{utils, IoEvent};
use anyhow::{Context, Result};
use nix::sys::wait::wait;
use nix::unistd::ForkResult::{Child, Parent};
use nix::unistd::{dup2, fork, setsid};
use rand::prelude::*;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use std::os::unix::io::IntoRawFd;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tui::widgets::{ListState, TableState};
use ureq::{Agent, AgentBuilder};

pub struct App {
    pub channels: StatefulList<Channel, ListState>,
    pub videos: StatefulList<Video, TableState>,
    pub selected: Selected,
    pub mode: Mode,
    pub conn: Connection,
    pub message: String,
    pub input: String,
    pub input_mode: InputMode,
    pub cursor_position: u16,
    pub options: Options,
    new_video_ids: HashSet<String>,
    search: Search,
    instance: Instance,
    hide_watched: bool,
    io_tx: Sender<IoEvent>,
}

impl App {
    pub fn new(mut options: Options, io_tx: Sender<IoEvent>) -> Result<Self> {
        if options.database_path.is_none() {
            options.database_path = Some(utils::get_database_file()?);
        }
        let mut app = Self {
            channels: StatefulList::with_items(Default::default()),
            videos: StatefulList::with_items(Default::default()),
            selected: Selected::Channels,
            mode: Mode::Subscriptions,
            conn: Connection::open(options.database_path.as_ref().unwrap())?,
            message: Default::default(),
            input: Default::default(),
            input_mode: InputMode::Normal,
            cursor_position: 0,
            search: Default::default(),
            instance: Instance::new(options.request_timeout)?,
            options,
            new_video_ids: Default::default(),
            hide_watched: false,
            io_tx,
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
            self.set_message(&e.to_string());
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
                self.set_message(&e.to_string());
                return;
            }
        };
        if new_video_count > 0 {
            self.move_channel_to_top(channel_id);
            let ids =
                database::get_newly_inserted_video_ids(&self.conn, channel_id, new_video_count)
                    .unwrap_or_default();
            self.new_video_ids.extend(ids);
        }
    }

    fn move_channel_to_top(&mut self, channel_id: &str) {
        let id_of_current_channel = self
            .get_current_channel()
            .map(|channel| channel.channel_id.clone());
        let index = self.find_channel_by_id(channel_id).unwrap();
        let mut channel = self.channels.items.remove(index);
        channel.new_video |= true;
        self.channels.items.insert(0, channel);
        if let Some(id) = id_of_current_channel {
            let index = self.find_channel_by_id(&id).unwrap();
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
        self.find_channel_by_id(channel_id)
            .map(|index| &mut self.channels.items[index])
    }

    fn find_channel_by_id(&mut self, channel_id: &str) -> Option<usize> {
        self.channels
            .items
            .iter()
            .position(|channel| channel.channel_id == channel_id)
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
                self.set_message(&e.to_string())
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
        self.on_change_channel();
    }

    fn run_detached<F: FnOnce()>(&mut self, func: F) {
        let pid = unsafe { fork().unwrap() };
        match pid {
            Parent { .. } => {
                wait().unwrap();
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
                func();
                std::process::exit(0);
            }
        }
    }

    pub fn play_video(&mut self) {
        if let Some(current_video) = self.get_current_video() {
            let url = format!(
                "{}/watch?v={}",
                self.instance.domain.clone(),
                current_video.video_id
            );
            let mpv_process = || {
                std::process::Command::new("mpv").arg(url).spawn().unwrap();
            };
            self.run_detached(mpv_process);
            self.mark_as_watched();
        }
    }

    pub fn open_video_in_browser(&mut self) {
        if let Some(current_video) = self.get_current_video() {
            let url = format!(
                "{}/watch?v={}",
                self.instance.domain.clone(),
                current_video.video_id
            );
            let browser_process = || webbrowser::open(&url).unwrap();
            self.run_detached(browser_process);
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
                self.set_message(&e.to_string())
            }
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

    pub fn prompt_for_subscription(&mut self) {
        self.input_mode = InputMode::Subscribe;
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
        if let Mode::Subscriptions = self.mode {
            self.input_mode = InputMode::Confirmation;
        }
    }

    pub fn unsubscribe(&mut self) {
        if let Some(index) = self.channels.state.selected() {
            let channel_id = &self.get_current_channel().unwrap().channel_id;
            database::delete_channel(&self.conn, channel_id).unwrap();
            self.input_mode = InputMode::Normal;
            self.channels.items.remove(index);
            if index == self.channels.items.len() {
                self.channels.previous();
            }
            self.on_change_channel();
        }
    }

    fn start_searching(&mut self) {
        self.input_mode = InputMode::Search;
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
    }

    pub fn repeat_last_search(&mut self) {
        self.repeat_last_search_helper(false);
    }

    pub fn repeat_last_search_opposite(&mut self) {
        self.repeat_last_search_helper(true);
    }

    pub fn push_key(&mut self, c: char) {
        if self.cursor_position as usize == self.input.len() {
            self.input.push(c);
        } else {
            self.input.insert(self.cursor_position.into(), c);
            if let InputMode::Search = self.input_mode {
                self.search.state = SearchState::PoppedKey;
            }
        }
        if let InputMode::Search = self.input_mode {
            self.search_in_block();
            self.search.state = SearchState::PushedKey;
        }
        self.cursor_position += 1;
    }

    pub fn pop_key(&mut self) {
        if self.cursor_position != 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position.into());
            if let InputMode::Search = self.input_mode {
                self.search.state = SearchState::PoppedKey;
                self.search_in_block();
            }
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position != 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position as usize != self.input.len() {
            self.cursor_position += 1;
        }
    }

    pub fn move_cursor_one_word_left(&mut self) {
        self.cursor_position = self.input[..self.cursor_position as usize]
            .trim()
            .rfind(|c| c == ' ')
            .map(|pos| pos + 1)
            .unwrap_or(0) as u16;
    }

    pub fn move_cursor_one_word_right(&mut self) {
        self.cursor_position = self
            .input
            .chars()
            .skip(self.cursor_position as usize)
            .position(|c| c == ' ')
            .map(|pos| pos as u16 + self.cursor_position + 1)
            .unwrap_or(self.input.len() as u16) as u16;
    }

    pub fn move_cursor_to_beginning_of_line(&mut self) {
        self.cursor_position = 0;
    }

    pub fn move_cursor_to_end_of_line(&mut self) {
        self.cursor_position = self.input.len() as u16;
    }

    pub fn delete_word_before_cursor(&mut self) {
        let old_cursor_position = self.cursor_position;
        self.move_cursor_one_word_left();
        self.input
            .drain(self.cursor_position as usize..old_cursor_position as usize);
        self.search.state = SearchState::PoppedKey;
    }

    pub fn clear_line(&mut self) {
        self.input.clear();
        self.cursor_position = 0;
        self.search.state = SearchState::PoppedKey;
    }

    pub fn clear_to_right(&mut self) {
        self.input.drain(self.cursor_position as usize..);
        self.search.state = SearchState::PoppedKey;
    }

    pub fn any_matches(&self) -> bool {
        self.search.any_matches()
    }

    pub fn complete_search(&mut self) {
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
            self.set_message(&format!("Error from dispatch: {}", e));
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
        self.dispatch(IoEvent::RefreshChannels);
    }

    pub fn set_message(&mut self, message: &str) {
        self.message = message.to_string();
    }

    pub fn clear_message(&mut self) {
        self.message.clear();
    }
}

#[derive(Clone)]
pub struct Instance {
    pub domain: String,
    agent: Agent,
}

impl Instance {
    pub fn new(timeout: u64) -> Result<Self> {
        let invidious_instances = match utils::read_instances() {
            Ok(instances) => instances,
            Err(_) => {
                utils::fetch_invidious_instances().with_context(|| "No instances available")?
            }
        };
        let mut rng = thread_rng();
        let domain = invidious_instances[rng.gen_range(0..invidious_instances.len())].to_string();
        let agent = AgentBuilder::new()
            .timeout(Duration::from_secs(timeout))
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
                "author,authorId,latestVideos(title,videoId,published,lengthSeconds)",
            )
            .call()?
            .into_json()?)
    }

    pub fn get_latest_videos_of_channel(&self, channel_id: &str) -> Result<Value> {
        let url = format!("{}/api/v1/channels/latest/{}", self.domain, channel_id);
        Ok(self
            .agent
            .get(&url)
            .query("fields", "title,videoId,published,lengthSeconds")
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

impl<T, S: State + Default> From<Vec<T>> for StatefulList<T, S> {
    fn from(v: Vec<T>) -> Self {
        StatefulList::with_items(v)
    }
}

pub enum Selected {
    Channels,
    Videos,
}

#[derive(Clone)]
pub enum Mode {
    Subscriptions,
    LatestVideos,
}
