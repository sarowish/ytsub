use crate::channel::{Channel, ListItem, RefreshState};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, fs::File, io::BufReader, path::Path};

#[derive(Clone, Copy)]
pub enum Format {
    YoutubeCsv,
    NewPipe,
}

impl From<&str> for Format {
    fn from(format: &str) -> Self {
        match format {
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
    pub fn read_subscriptions(path: &Path) -> Result<Vec<ImportItem>> {
        let file = File::open(path)?;
        let mut rdr = csv::Reader::from_reader(file);

        let mut subscriptions: Vec<YoutubeCsv> = Vec::new();

        for record in rdr.deserialize() {
            subscriptions.push(record?);
        }

        Ok(subscriptions.into_iter().map(ImportItem::from).collect())
    }

    pub fn export(channels: &[Channel], path: &Path) -> Result<()> {
        let file = File::create(path)?;
        let mut wtr = csv::Writer::from_writer(file);

        for channel in channels {
            wtr.serialize(YoutubeCsv {
                channel_id: channel.channel_id.clone(),
                channel_url: format!("http://www.youtube.com/channel/{}", channel.channel_id),
                channel_title: channel.channel_name.clone(),
            })?;
        }

        wtr.flush()?;

        Ok(())
    }
}

impl Import for YoutubeCsv {
    fn channel_id(&self) -> String {
        self.channel_id.clone()
    }

    fn channel_title(&self) -> String {
        self.channel_title.clone()
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
            name: channel.channel_name.clone(),
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

    pub fn read_subscriptions(path: &Path) -> Result<Vec<ImportItem>> {
        let file = File::open(path)?;
        let rdr = BufReader::new(file);

        let newpipe: NewPipe = serde_json::from_reader(rdr)?;

        Ok(newpipe
            .subscriptions
            .into_iter()
            .map(ImportItem::from)
            .collect())
    }

    pub fn export(channels: &[Channel], path: &Path) -> Result<()> {
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
        self.name.clone()
    }
}

pub struct ImportItem {
    pub sub_state: RefreshState,
    pub channel_title: String,
    pub channel_id: String,
}

impl<T: Import> From<T> for ImportItem {
    fn from(item: T) -> Self {
        Self {
            sub_state: RefreshState::Completed,
            channel_title: item.channel_title(),
            channel_id: item.channel_id(),
        }
    }
}

impl ListItem for ImportItem {
    fn id(&self) -> &str {
        &self.channel_id
    }
}

impl Display for ImportItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}",
            match self.sub_state {
                RefreshState::ToBeRefreshed => "□",
                RefreshState::Refreshing => "■",
                _ => "",
            },
            self.channel_title
        )
    }
}
