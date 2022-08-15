use crate::{
    app::StatefulList,
    channel::{Channel, ListItem, RefreshState},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    fs::File,
    io::BufReader,
    ops::{Deref, DerefMut},
    path::PathBuf,
};
use tui::widgets::ListState;

pub enum Format {
    YoutubeCsv,
    NewPipe,
}

impl From<&str> for Format {
    fn from(format: &str) -> Self {
        match format {
            "youtube_csv" => Format::YoutubeCsv,
            "newpipe" => Format::NewPipe,
            _ => Format::YoutubeCsv,
        }
    }
}

pub trait Import {
    fn channel_id(&self) -> String;
    fn channel_title(&self) -> String;
}

#[derive(Deserialize, Serialize)]
pub struct YoutubeCsv {
    #[serde(rename = "Channel Id")]
    pub channel_id: String,
    #[serde(rename = "Channel Url")]
    channel_url: String,
    #[serde(rename = "Channel Title")]
    pub channel_title: String,
}

impl YoutubeCsv {
    pub fn read_subscriptions(path: PathBuf) -> Result<Subscriptions> {
        let file = File::open(path)?;
        let mut rdr = csv::Reader::from_reader(file);

        let mut subscriptions: Vec<YoutubeCsv> = Vec::new();

        for record in rdr.deserialize() {
            subscriptions.push(record?);
        }

        Ok(Subscriptions::new(&subscriptions))
    }

    pub fn export(channels: &[Channel], path: PathBuf) -> Result<()> {
        let file = File::create(path)?;
        let mut wtr = csv::Writer::from_writer(file);

        for channel in channels {
            wtr.serialize(YoutubeCsv {
                channel_id: channel.channel_id.to_owned(),
                channel_url: format!("http://www.youtube.com/channel/{}", channel.channel_id),
                channel_title: channel.channel_name.to_owned(),
            })?;
        }

        wtr.flush()?;

        Ok(())
    }
}

impl Import for YoutubeCsv {
    fn channel_id(&self) -> String {
        self.channel_id.to_owned()
    }

    fn channel_title(&self) -> String {
        self.channel_title.to_owned()
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct NewPipeInner {
    service_id: u64,
    pub url: String,
    pub name: String,
}

impl NewPipeInner {
    fn new(channel: &Channel) -> Self {
        Self {
            service_id: 0,
            url: format!("http://www.youtube.com/channel/{}", channel.channel_id),
            name: channel.channel_name.to_owned(),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct NewPipe {
    app_version: String,
    app_version_int: u64,
    pub subscriptions: Vec<NewPipeInner>,
}

impl NewPipe {
    fn new(subscriptions: Vec<NewPipeInner>) -> Self {
        Self {
            app_version: "0.23.0".to_string(),
            app_version_int: 986,
            subscriptions,
        }
    }

    pub fn read_subscriptions(path: PathBuf) -> Result<Subscriptions> {
        let file = File::open(path)?;
        let rdr = BufReader::new(file);

        let newpipe: NewPipe = serde_json::from_reader(rdr)?;

        Ok(Subscriptions::new(&newpipe.subscriptions))
    }

    pub fn export(channels: &[Channel], path: PathBuf) -> Result<()> {
        let file = File::create(path)?;

        let subs = channels.iter().map(NewPipeInner::new).collect();

        let newpipe = NewPipe::new(subs);

        Ok(serde_json::to_writer(file, &newpipe)?)
    }
}

impl Import for NewPipeInner {
    fn channel_id(&self) -> String {
        self.url
            .rsplit_once('/')
            .map(|(_, id)| id)
            .unwrap()
            .to_string()
    }

    fn channel_title(&self) -> String {
        self.name.to_owned()
    }
}

pub struct Subscriptions(StatefulList<ImportItemState, ListState>);

impl Subscriptions {
    pub fn new<T: Import>(subs: &[T]) -> Self {
        let subs = subs
            .iter()
            .map(|sub| ImportItemState::new(sub.channel_title(), sub.channel_id()))
            .collect();

        let mut list = StatefulList::with_items(subs);
        list.select_first();

        Self(list)
    }

    pub fn toggle(&mut self) {
        if let Some(idx) = self.state.selected() {
            self.items[idx].toggle();
        }
    }

    pub fn select_all(&mut self) {
        for entry in self.items.iter_mut() {
            entry.selected = true;
        }
    }

    pub fn deselect_all(&mut self) {
        for entry in self.items.iter_mut() {
            entry.selected = false;
        }
    }
}

impl Default for Subscriptions {
    fn default() -> Self {
        Self(StatefulList::with_items(Default::default()))
    }
}

impl Deref for Subscriptions {
    type Target = StatefulList<ImportItemState, ListState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Subscriptions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct ImportItemState {
    pub selected: bool,
    pub sub_state: RefreshState,
    pub channel_title: String,
    pub channel_id: String,
}

impl ImportItemState {
    pub fn new(channel_title: String, channel_id: String) -> Self {
        Self {
            selected: true,
            sub_state: RefreshState::Completed,
            channel_title,
            channel_id,
        }
    }

    pub fn toggle(&mut self) {
        self.selected = !self.selected;
    }
}

impl ListItem for ImportItemState {
    fn id(&self) -> &str {
        &self.channel_id
    }
}

impl Display for ImportItemState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}",
            {
                if let RefreshState::Completed = self.sub_state {
                    format!("[{}]", if self.selected { "*" } else { " " })
                } else {
                    match self.sub_state {
                        RefreshState::ToBeRefreshed => "□",
                        RefreshState::Refreshing => "■",
                        RefreshState::Completed => "",
                        RefreshState::Failed => "✗",
                    }
                    .to_string()
                }
            },
            self.channel_title
        )
    }
}
