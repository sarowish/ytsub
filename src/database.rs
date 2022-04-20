use crate::{
    channel::{Channel, Video, VideoType},
    utils,
};
use anyhow::Result;
use rusqlite::{params, Connection};

pub fn initialize_db(conn: &Connection) -> Result<()> {
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

pub fn add_videos(conn: &Connection, channel_id: &str, videos: &[Video]) -> Result<usize> {
    let columns = [
        "video_id",
        "channel_id",
        "title",
        "published",
        "length",
        "watched",
    ];
    let columns_str = columns.join(", ");

    let idxs = (1..=(videos.len() * columns.len())).collect::<Vec<_>>();
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

    let query = format!(
        "INSERT OR IGNORE INTO videos ({})
        VALUES {}",
        columns_str, values_string
    );
    let new_video_count = conn.execute(&query, videos_values.as_slice())?;

    Ok(new_video_count)
}

pub fn get_newly_inserted_video_ids(
    conn: &Connection,
    channel_id: &str,
    new_video_count: usize,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT video_id
        FROM videos
        WHERE channel_id=?1
        ORDER BY published DESC
        LIMIT ?2
        ",
    )?;
    let mut video_ids = Vec::new();
    for video in stmt.query_map(params![channel_id, new_video_count], |row| row.get(0))? {
        video_ids.push(video?);
    }

    Ok(video_ids)
}

pub fn get_channel_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT channel_id
        FROM channels
        ",
    )?;

    let mut ids = Vec::new();
    for row in stmt.query_map([], |row| row.get(0))? {
        ids.push(row?);
    }

    Ok(ids)
}

pub fn get_channels(conn: &Connection) -> Result<Vec<Channel>> {
    let mut stmt = conn.prepare(
        "SELECT channel_id, channel_name
        FROM channels
        ORDER BY channel_name ASC
        ",
    )?;

    let mut channels = Vec::new();
    for channel in stmt.query_map([], |row| {
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

pub fn get_latest_videos(conn: &Connection) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, published, length, watched, channel_name
            FROM videos, channels
            WHERE videos.channel_id = channels.channel_id
            ORDER BY published DESC
            LIMIT 100
            ",
    )?;
    let mut videos = Vec::new();

    for video in stmt.query_map([], |row| {
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
