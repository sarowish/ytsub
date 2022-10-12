use crate::{
    channel::{Channel, Video, VideoType},
    utils,
};
use anyhow::Result;
use rusqlite::{params, Connection};

pub fn initialize_db(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "on")?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS channels (
            channel_id TEXT PRIMARY KEY,
            channel_name TEXT
            )
        ",
        [],
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS videos (
            video_id TEXT PRIMARY KEY,
            channel_id TEXT,
            title TEXT,
            published INTEGER,
            length INTEGER,
            watched BOOL,
            FOREIGN KEY(channel_id) REFERENCES channels(channel_id) ON DELETE CASCADE
            )
        ",
        [],
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS tags (
            tag_name TEXT PRIMARY KEY
            )
        ",
        [],
    )?;

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS tag_relations (
            tag_name TEXT,
            channel_id TEXT,
            PRIMARY KEY(tag_name, channel_id),
            FOREIGN KEY(channel_id) REFERENCES channels(channel_id) ON DELETE CASCADE,
            FOREIGN KEY(tag_name) REFERENCES tags(tag_name) ON DELETE CASCADE ON UPDATE CASCADE
            )
        ",
        [],
    )?;

    Ok(())
}

pub fn create_channel(conn: &Connection, channel: &Channel) -> Result<()> {
    conn.execute(
        "INSERT INTO channels (channel_id, channel_name)
        VALUES (?1, ?2)",
        params![channel.channel_id, channel.channel_name],
    )?;

    Ok(())
}

pub fn delete_channel(conn: &Connection, channel_id: &str) -> Result<()> {
    if let Err(e) = conn.execute(
        "DELETE FROM channels WHERE channel_id=?1",
        params![channel_id],
    ) {
        match e {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::ConstraintViolation,
                    ..
                },
                _,
            ) => {
                // if the table was created without the "ON DELETE CASCADE" option, delete the
                // videos manually
                conn.execute(
                    "DELETE FROM videos WHERE channel_id=?1",
                    params![channel_id],
                )?;
                conn.execute(
                    "DELETE FROM channels WHERE channel_id=?1",
                    params![channel_id],
                )?;
            }
            _ => return Err(anyhow::anyhow!(e)),
        }
    }

    Ok(())
}

enum StatementType {
    AddVideo,
    AddToTag,
    RemoveFromTag,
    GetChannels,
    GetLatestVideos,
}

fn build_bulk_stmt<T>(query_type: StatementType, columns: &[&str], values: &[T]) -> String {
    let columns_str = columns.join(", ");
    let idxs = (1..=values.len()).collect::<Vec<_>>();
    let values_string = idxs
        .chunks(columns.len())
        .map(|chunk| {
            format!(
                "({})",
                chunk
                    .iter()
                    .map(|i| format!("?{}", i))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    match query_type {
        StatementType::AddVideo => format!(
            "INSERT OR REPLACE INTO videos ({})
            VALUES {}
            ",
            columns_str, values_string
        ),
        StatementType::AddToTag => format!(
            "INSERT INTO tag_relations ({})
            VALUES {}",
            columns_str, values_string
        ),
        StatementType::RemoveFromTag => format!(
            "DELETE FROM tag_relations WHERE ({}) IN ({})",
            columns_str, values_string
        ),
        StatementType::GetChannels => format!(
            "SELECT DISTINCT channels.channel_id, channel_name
            FROM channels, tag_relations
            WHERE tag_relations.channel_id=channels.channel_id AND tag_relations.tag_name IN ({})
            ORDER BY channel_name COLLATE NOCASE ASC
            ",
            values_string
        ),
        StatementType::GetLatestVideos => format!(
            "SELECT DISTINCT video_id, title, published, length, watched, channel_name
            FROM videos, channels, tag_relations
            WHERE videos.channel_id = channels.channel_id AND tag_relations.tag_name IN ({})
            AND tag_relations.channel_id=channels.channel_id
            ORDER BY published DESC
            LIMIT 100
            ",
            values_string
        ),
    }
}

pub fn add_videos(conn: &Connection, channel_id: &str, videos: &[Video]) -> Result<()> {
    let columns = [
        "video_id",
        "channel_id",
        "title",
        "published",
        "length",
        "watched",
    ];

    let mut videos_values = Vec::with_capacity(videos.len() * columns.len());
    for video in videos {
        let values = params![
            video.video_id,
            channel_id,
            video.title,
            video.published,
            video.length,
            false,
        ];
        videos_values.extend_from_slice(values);
    }

    let query = build_bulk_stmt(StatementType::AddVideo, &columns, &videos_values);

    conn.execute(&query, videos_values.as_slice())?;

    Ok(())
}

pub fn delete_video(conn: &Connection, video_id: &str) -> Result<()> {
    conn.execute("DELETE FROM videos WHERE video_id=?1", params![video_id])?;
    Ok(())
}

pub fn get_channels(conn: &Connection, tags: &[&str]) -> Result<Vec<Channel>> {
    let mut stmt;
    let values;

    if tags.is_empty() {
        values = rusqlite::params_from_iter([].iter());

        stmt = conn.prepare(
            "SELECT channel_id, channel_name
            FROM channels
            ORDER BY channel_name COLLATE NOCASE ASC
            ",
        )?;
    } else {
        values = rusqlite::params_from_iter(tags.iter());

        stmt = conn.prepare(&build_bulk_stmt(
            StatementType::GetChannels,
            &["tag_name"],
            tags,
        ))?;
    }

    let mut channels = Vec::new();
    for channel in stmt.query_map(values, |row| {
        let channel_id: String = row.get(0)?;
        let channel_name: String = row.get(1)?;
        Ok(Channel::new(channel_id, channel_name))
    })? {
        channels.push(channel?);
    }

    Ok(channels)
}

pub fn get_videos(conn: &Connection, channel_id: &str) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, published, length, watched
        FROM videos
        WHERE channel_id=?1
        ORDER BY published DESC
        ",
    )?;
    let mut videos = Vec::new();
    for video in stmt.query_map(params![channel_id], |row| {
        Ok(Video {
            video_type: Some(VideoType::Subscriptions),
            video_id: row.get(0)?,
            title: row.get(1)?,
            published: row.get(2)?,
            published_text: utils::published_text(row.get(2)?),
            length: row.get(3)?,
            watched: row.get(4)?,
            new: false,
        })
    })? {
        videos.push(video?);
    }

    Ok(videos)
}

pub fn get_latest_videos(conn: &Connection, tags: &[&str]) -> Result<Vec<Video>> {
    let mut stmt;
    let values;

    if tags.is_empty() {
        values = rusqlite::params_from_iter([].iter());

        stmt = conn.prepare(
            "SELECT video_id, title, published, length, watched, channel_name
            FROM videos, channels
            WHERE videos.channel_id = channels.channel_id
            ORDER BY published DESC
            LIMIT 100
            ",
        )?;
    } else {
        values = rusqlite::params_from_iter(tags.iter());

        stmt = conn.prepare(&build_bulk_stmt(
            StatementType::GetLatestVideos,
            &["tag_name"],
            tags,
        ))?;
    }
    let mut videos = Vec::new();

    for video in stmt.query_map(values, |row| {
        Ok(Video {
            video_type: Some(VideoType::LatestVideos(row.get(5)?)),
            video_id: row.get(0)?,
            title: row.get(1)?,
            published: row.get(2)?,
            published_text: utils::published_text(row.get(2)?),
            length: row.get(3)?,
            watched: row.get(4)?,
            new: false,
        })
    })? {
        videos.push(video?);
    }

    Ok(videos)
}

pub fn set_watched_field(conn: &Connection, video_id: &str, watched: bool) -> Result<()> {
    let mut stmt = conn.prepare("UPDATE videos SET watched=?1 WHERE video_id=?2")?;
    stmt.execute(params![watched, video_id])?;
    Ok(())
}

pub fn create_tag(conn: &Connection, tag_name: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO tags (tag_name)
        VALUES (?1)",
        params![tag_name],
    )?;

    Ok(())
}

pub fn rename_tag(conn: &Connection, old_name: &str, new_name: &str) -> Result<()> {
    conn.execute(
        "UPDATE tags SET tag_name=?1 WHERE tag_name=?2",
        params![new_name, old_name],
    )?;

    Ok(())
}

pub fn get_tags(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT tag_name FROM tags")?;

    let mut tags = Vec::new();

    for tag in stmt.query_map([], |row| row.get(0))? {
        tags.push(tag?);
    }

    Ok(tags)
}

pub fn delete_tag(conn: &Connection, tag_name: &str) -> Result<()> {
    conn.execute("DELETE FROM tags WHERE tag_name=?1", params![tag_name])?;
    Ok(())
}

pub fn update_channels_of_tag(
    conn: &Connection,
    tag_name: &str,
    channel_ids: &[String],
) -> Result<()> {
    let present_channels = get_channels(conn, &[tag_name])?;

    let columns = ["tag_name", "channel_id"];

    let new: Vec<&String> = channel_ids
        .iter()
        .filter(|channel_id| {
            !present_channels
                .iter()
                .any(|other_channel| other_channel.channel_id == **channel_id)
        })
        .collect();

    let mut values = Vec::with_capacity(new.len() * columns.len());
    for channel_id in new {
        let v = params![tag_name, *channel_id];
        values.extend_from_slice(v);
    }

    if !values.is_empty() {
        let query = build_bulk_stmt(StatementType::AddToTag, &columns, &values);
        conn.execute(&query, values.as_slice())?;
    }

    let removed: Vec<&String> = present_channels
        .iter()
        .map(|channel| &channel.channel_id)
        .filter(|channel| {
            !channel_ids
                .iter()
                .any(|other_channel| other_channel == *channel)
        })
        .collect();

    let mut values = Vec::with_capacity(removed.len() * columns.len());
    for channel in removed {
        let v = params![tag_name, *channel];
        values.extend_from_slice(v);
    }

    let query = build_bulk_stmt(StatementType::RemoveFromTag, &columns, &values);
    conn.execute(&query, values.as_slice())?;

    Ok(())
}
