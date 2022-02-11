use crate::channel::{Channel, Video};
use crate::database;
use rand::prelude::*;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead};
use std::time::Duration;
use tui::widgets::ListState;
use ureq::{Agent, AgentBuilder};

pub struct App {
    pub channels: StatefulList<Channel>,
    pub videos: StatefulList<Video>,
    pub selected: Selected,
    pub channel_ids: Vec<String>,
    pub conn: Connection,
    instance: Instance,
    hide_watched: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            channels: StatefulList::with_items(Default::default()),
            videos: StatefulList::with_items(Default::default()),
            selected: Selected::Channels,
            channel_ids: App::channel_ids_from_file("subs"),
            conn: Connection::open("./videos.db").unwrap(),
            instance: Instance::new(),
            hide_watched: false,
        }
    }

    fn channel_ids_from_file(file_name: &str) -> Vec<String> {
        let file = File::open(file_name).unwrap();
        io::BufReader::new(file)
            .lines()
            .map(|id| id.unwrap())
            .collect()
    }

    pub fn add_new_channels(&mut self) {
        let channels_in_database = database::get_channel_ids(&self.conn);
        let channels_in_database = channels_in_database.into_iter().collect::<HashSet<_>>();
        let current_channels = self.channel_ids.iter().cloned().collect::<HashSet<_>>();
        let difference = current_channels.difference(&channels_in_database);
        difference.for_each(|channel_id| self.add_channel(channel_id))
    }

    fn add_channel(&mut self, channel_id: &str) {
        let videos_json = self.instance.get_videos_of_channel(channel_id);
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
        database::create_channel(&self.conn, &channel);
        self.add_videos(videos_json, &channel_id);
        self.channels.items.push(channel);
    }

    pub fn add_videos(&self, videos_json: Value, channel_id: &str) {
        let videos: Vec<Video> = Video::vec_from_json(videos_json);
        database::add_videos(&self.conn, channel_id, &videos);
    }

    pub fn load_videos(&mut self) {
        self.channels = database::get_channels(&self.conn).into();
    }

    pub fn instance(&self) -> Instance {
        self.instance.clone()
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
        self.get_mut_current_video().unwrap().watched = is_watched;
        database::set_watched_field(
            &self.conn,
            &self.get_current_video().unwrap().video_id,
            is_watched,
        );
    }

    pub fn mark_as_watched(&mut self) {
        self.set_watched(true);
    }

    fn mark_as_unwatched(&mut self) {
        self.set_watched(false);
    }

    pub fn toggle_watched(&mut self) {
        if self.get_current_video().unwrap().watched {
            self.mark_as_unwatched();
        } else {
            self.mark_as_watched();
        }
    }

    pub fn toggle_hide(&mut self) {
        self.hide_watched = !self.hide_watched;
        self.on_change_channel();
    }

    pub fn play_video(&mut self) {
        std::process::Command::new("setsid")
            .arg("--fork")
            .arg("mpv")
            .arg("--no-terminal")
            .arg(format!(
                "{}/watch?v={}",
                self.instance.domain.clone(),
                self.get_current_video().unwrap().video_id
            ))
            .spawn()
            .unwrap();
    }

    pub fn open_video_in_browser(&mut self) {
        webbrowser::open_browser_with_options(
            webbrowser::BrowserOptions::create_with_suppressed_output(&format!(
                "{}/watch?v={}",
                self.instance.domain.clone(),
                self.get_current_video().unwrap().video_id
            )),
        )
        .unwrap();
    }

    pub fn on_change_channel(&mut self) {
        let channel_id = &self.get_current_channel().unwrap().channel_id.clone();
        let videos = database::get_videos(&self.conn, channel_id);
        self.videos.items = if self.hide_watched {
            videos.into_iter().filter(|video| !video.watched).collect()
        } else {
            videos
        };
        self.videos.reset_state();
    }

    pub fn on_refresh_channel(&mut self) {
        let channel_id = &self.get_current_channel().unwrap().channel_id.clone();
        let videos = database::get_videos(&self.conn, channel_id);
        self.videos.items = if self.hide_watched {
            videos.into_iter().filter(|video| !video.watched).collect()
        } else {
            videos
        };
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
        if let Selected::Videos = self.selected {
            self.selected = Selected::Channels;
        }
    }

    pub fn on_right(&mut self) {
        if let Selected::Channels = self.selected {
            self.selected = Selected::Videos;
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
}

#[derive(Clone)]
pub struct Instance {
    pub domain: String,
    agent: Agent,
}

impl Instance {
    pub fn new() -> Self {
        const INVIDIOUS_INSTANCES: [&str; 2] =
            ["https://vid.puffyan.us", "https://invidio.xamh.de"];
        let mut rng = thread_rng();
        let domain = INVIDIOUS_INSTANCES[rng.gen_range(0..INVIDIOUS_INSTANCES.len())].to_string();
        let agent = AgentBuilder::new().timeout(Duration::from_secs(5)).build();
        Self { domain, agent }
    }

    pub fn get_videos_of_channel(&self, channel_id: &str) -> Value {
        let query = String::from("?fields=title,videoId,author,authorId,published,lengthSeconds");
        let url = format!(
            "{}{}{}/videos{}",
            self.domain, "/api/v1/channels/", channel_id, query
        );
        self.agent.get(&url).call().unwrap().into_json().unwrap()
    }
}

pub struct StatefulList<T> {
    pub state: ListState,
    pub items: Vec<T>,
}

impl<T> StatefulList<T> {
    fn with_items(items: Vec<T>) -> StatefulList<T> {
        StatefulList {
            state: ListState::default(),
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
        self.select_with_index(self.items.len() - 1);
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

impl<T> From<Vec<T>> for StatefulList<T> {
    fn from(v: Vec<T>) -> Self {
        StatefulList::with_items(v)
    }
}

pub enum Selected {
    Channels,
    Videos,
}
