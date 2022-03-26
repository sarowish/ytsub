use crate::channel::{Channel, RefreshState, Video};
use crate::input::InputMode;
use crate::search::{Search, SearchDirection, SearchState};
use crate::{database, Options};
use crate::{utils, IoEvent};
use anyhow::{Context, Result};
use nix::sys::wait::wait;
use nix::unistd::ForkResult::{Child, Parent};
use nix::unistd::{fork, setsid};
use rand::prelude::*;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tui::widgets::{ListState, TableState};
use ureq::{Agent, AgentBuilder};

pub struct App {
    pub channels: StatefulList<Channel, ListState>,
    pub videos: StatefulList<Video, TableState>,
    pub selected: Selected,
    pub mode: Mode,
    pub channel_ids: Vec<String>,
    pub conn: Connection,
    pub message: String,
    pub input: String,
    pub input_mode: InputMode,
    search: Search,
    instance: Instance,
    hide_watched: bool,
    io_tx: Sender<IoEvent>,
}

impl App {
    pub fn new(options: Options, io_tx: Sender<IoEvent>) -> Result<Self> {
        let mut app = Self {
            channels: StatefulList::with_items(Default::default()),
            videos: StatefulList::with_items(Default::default()),
            selected: Selected::Channels,
            mode: Mode::Subscriptions,
            channel_ids: utils::read_subscriptions(options.subs_path)?,
            conn: Connection::open(
                options
                    .database_path
                    .unwrap_or_else(|| utils::get_database_file().unwrap()),
            )?,
            message: Default::default(),
            input: Default::default(),
            input_mode: InputMode::Normal,
            search: Default::default(),
            instance: Instance::new(options.request_timeout)?,
            hide_watched: false,
            io_tx,
        };

        database::initialize_db(&app.conn)?;
        app.set_mode_subs();
        app.load_channels()?;
        app.select_first();
        app.add_new_channels();

        Ok(app)
    }

    pub fn get_new_channel_ids(&mut self) -> Result<Vec<String>> {
        let channels_in_database = database::get_channel_ids(&self.conn)?;
        let channels_in_database = channels_in_database.into_iter().collect::<HashSet<_>>();
        let current_channels = self.channel_ids.iter().cloned().collect::<HashSet<_>>();
        Ok(current_channels
            .difference(&channels_in_database)
            .cloned()
            .collect())
    }

    pub fn add_channel(&mut self, videos_json: Value) {
        let channel_id: String = videos_json
            .get(0)
            .unwrap()
            .get("authorId")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        let channel_name: String = videos_json
            .get(0)
            .unwrap()
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
        self.add_videos(videos_json, &channel_id);
    }

    pub fn add_videos(&mut self, videos_json: Value, channel_id: &str) {
        let videos: Vec<Video> = Video::vec_from_json(videos_json);
        let any_new_videos = match database::add_videos(&self.conn, channel_id, &videos) {
            Ok(new_video_count) => new_video_count,
            Err(e) => {
                self.set_message(&e.to_string());
                return;
            }
        };
        self.get_channel_by_id(channel_id).unwrap().new_video |= any_new_videos;
    }

    pub fn load_channels(&mut self) -> Result<()> {
        self.channels = database::get_channels(&self.conn)?.into();
        Ok(())
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.reset_search();
        match mode {
            Mode::Subscriptions => self.set_mode_subs(),
            Mode::LatestVideos => self.set_mode_latest_videos(),
        }
    }

    pub fn set_mode_subs(&mut self) {
        if !matches!(self.mode, Mode::Subscriptions) {
            self.mode = Mode::Subscriptions;
            self.selected = Selected::Channels;
            self.load_videos();
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
        for channel in &mut self.channels.items {
            if channel.channel_id == channel_id {
                return Some(channel);
            }
        }
        None
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

    pub fn play_video(&mut self) {
        let pid = unsafe { fork().unwrap() };
        if let Some(current_video) = self.get_current_video() {
            match pid {
                Parent { .. } => {
                    wait().unwrap();
                }
                Child => {
                    setsid().unwrap();
                    std::process::Command::new("mpv")
                        .arg("--no-terminal")
                        .arg(format!(
                            "{}/watch?v={}",
                            self.instance.domain.clone(),
                            current_video.video_id
                        ))
                        .spawn()
                        .unwrap();
                    std::process::exit(0);
                }
            }
            self.mark_as_watched();
        }
    }

    pub fn open_video_in_browser(&mut self) {
        let pid = unsafe { fork().unwrap() };
        if let Some(current_video) = self.get_current_video() {
            match pid {
                Parent { .. } => {
                    std::thread::spawn(|| {
                        wait().unwrap();
                    });
                }
                Child => {
                    setsid().unwrap();
                    open::that(&format!(
                        "{}/watch?v={}",
                        self.instance.domain.clone(),
                        current_video.video_id
                    ))
                    .unwrap();
                    std::process::exit(0);
                }
            }
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
        if matches!(self.selected, Selected::Videos) && matches!(self.mode, Mode::Subscriptions) {
            self.selected = Selected::Channels;
            self.reset_search();
        }
    }

    pub fn on_right(&mut self) {
        if matches!(self.selected, Selected::Channels) && matches!(self.mode, Mode::Subscriptions) {
            self.selected = Selected::Videos;
            self.reset_search();
        }
    }

    pub fn select_first(&mut self) {
        match self.selected {
            Selected::Channels => {
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
                self.channels.select_last();
                self.on_change_channel();
            }
            Selected::Videos => {
                self.videos.select_last();
            }
        }
    }

    pub fn is_footer_active(&self) -> bool {
        matches!(self.input_mode, InputMode::Editing) || !self.message.is_empty()
    }

    fn start_searching(&mut self) {
        self.input_mode = InputMode::Editing;
        self.search.previous_matches = self.search.matches.drain(..).collect();
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

    pub fn next_match(&mut self) {
        match self.selected {
            Selected::Channels => {
                self.search.next_match(&mut self.channels);
                self.on_change_channel()
            }
            Selected::Videos => self.search.next_match(&mut self.videos),
        }
    }

    pub fn prev_match(&mut self) {
        match self.selected {
            Selected::Channels => {
                self.search.prev_match(&mut self.channels);
                self.on_change_channel()
            }
            Selected::Videos => self.search.prev_match(&mut self.videos),
        }
    }

    pub fn push_key(&mut self, c: char) {
        self.input.push(c);
        self.search_in_block();
    }

    pub fn pop_key(&mut self) {
        self.input.pop();
        self.search.state = SearchState::PoppedKey;
        self.search_in_block();
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
        match self.selected {
            Selected::Channels => {
                self.search.recover_item(&mut self.channels);
                self.on_change_channel()
            }
            Selected::Videos => self.search.recover_item(&mut self.videos),
        }
    }

    pub fn abort_search(&mut self) {
        self.recover_item();
        self.finalize_search(true);
    }

    fn reset_search(&mut self) {
        self.search.matches.clear();
    }

    fn dispatch(&mut self, action: IoEvent) {
        if let Err(e) = self.io_tx.send(action) {
            self.set_message(&format!("Error from dispatch: {}", e));
        }
    }

    pub fn add_new_channels(&mut self) {
        self.dispatch(IoEvent::AddNewChannels);
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
        let url = format!("{}/api/v1/channels/{}/videos", self.domain, channel_id);
        Ok(self
            .agent
            .get(&url)
            .query(
                "fields",
                "title,videoId,author,authorId,published,lengthSeconds",
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
