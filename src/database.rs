use crate::channel::{Channel, Video};
use rusqlite::{params, Connection};

pub fn initialize_db(conn: &Connection) {
    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS channels (
            channel_id TEXT PRIMARY KEY,
            channel_name TEXT
            )
        ",
        [],
    )
    .unwrap();
    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS videos (
            video_id TEXT PRIMARY KEY,
            channel_id TEXT,
            title TEXT,
            published INTEGER,
            length INTEGER,
            watched BOOL,
            FOREIGN KEY(channel_id) REFERENCES channels(channel_id)
            )
        ",
        [],
    )
    .unwrap();
}

pub fn create_channel(conn: &Connection, channel: &Channel) {
    conn.execute(
        "INSERT INTO channels (channel_id, channel_name)
        VALUES (?1, ?2)",
        params![channel.channel_id, channel.channel_name],
    )
    .unwrap();
}

pub fn add_videos(conn: &Connection, channel_id: &str, videos: &[Video]) {
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
    conn.execute(&query, videos_values.as_slice()).unwrap();
}

pub fn get_channel_ids(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT channel_id
            FROM channels
        ",
        )
        .unwrap();

    stmt.query_map([], |row| Ok(row.get::<_, String>(0).unwrap()))
        .unwrap()
        .map(|res| res.unwrap())
        .collect()
}

pub fn get_channels(conn: &Connection) -> Vec<Channel> {
    let mut stmt = conn
        .prepare(
            "SELECT channel_id, channel_name
            FROM channels
            ORDER BY channel_name ASC
        ",
        )
        .unwrap();

    stmt.query_map([], |row| {
        let channel_id: String = row.get(0).unwrap();
        let channel_name: String = row.get(1).unwrap();
        Ok(Channel::new(channel_id, channel_name))
    })
    .unwrap()
    .map(|res| res.unwrap())
    .collect()
}

pub fn get_videos(conn: &Connection, channel_id: &str) -> Vec<Video> {
    let mut stmt = conn
        .prepare(
            "SELECT video_id, title, published, length, watched
            FROM videos
            WHERE channel_id=?1
            ORDER BY published DESC
            ",
        )
        .unwrap();
    stmt.query_map(params![channel_id], |row| {
        Ok(Video {
            video_id: row.get(0).unwrap(),
            title: row.get(1).unwrap(),
            published: row.get(2).unwrap(),
            length: row.get(3).unwrap(),
            watched: row.get(4).unwrap(),
            new: false,
        })
    })
    .unwrap()
    .map(|res| res.unwrap())
    .collect()
}

pub fn set_watched_field(conn: &Connection, video_id: &str, watched: bool) {
    let mut stmt = conn
        .prepare("UPDATE videos SET watched=?1 WHERE video_id=?2")
        .unwrap();
    stmt.execute(params![watched, video_id]).unwrap();
}
