use crate::{
    channel::{Channel, ChannelTab},
    utils,
    video::{Video, VideoListItem},
};
use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params};
use std::{
    fs::{self, OpenOptions},
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

const LATEST_USER_VERSION: u8 = 4;
const MIN_DOWNGRADE_USER_VERSION: u8 = 1;

fn user_version(conn: &Connection) -> Result<u8> {
    Ok(conn.pragma_query_value(None, "user_version", |value| value.get(0))?)
}

pub fn open_db(path: &Path) -> Result<Connection> {
    let database_exists = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len() > 0,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect database at {}", path.display()));
        }
    };

    let mut conn = Connection::open(path)?;

    conn.pragma_update(None, "foreign_keys", "on")?;

    let current_user_version = user_version(&conn)?;

    if database_exists && current_user_version < LATEST_USER_VERSION {
        backup_db(&conn, path, current_user_version)?;
    }

    for i in current_user_version..LATEST_USER_VERSION {
        apply_up_migration(&mut conn, i)?;
    }

    Ok(conn)
}

#[derive(Debug, Eq, PartialEq)]
pub enum DowngradeOutcome {
    Downgraded {
        from: u8,
        to: u8,
        backup_path: PathBuf,
    },
    AlreadyAtTarget {
        version: u8,
    },
}

pub fn downgrade_database(path: &Path, target_version: Option<u8>) -> Result<DowngradeOutcome> {
    if let Some(target_version) = target_version {
        ensure!(
            (MIN_DOWNGRADE_USER_VERSION..LATEST_USER_VERSION).contains(&target_version),
            "unsupported target database schema version {target_version}; supported downgrade targets are {} through {}",
            MIN_DOWNGRADE_USER_VERSION,
            LATEST_USER_VERSION - 1
        );
    }

    let flags = OpenFlags::default().difference(OpenFlags::SQLITE_OPEN_CREATE);
    let mut conn = Connection::open_with_flags(path, flags).with_context(|| {
        format!(
            "failed to open database for downgrade at {}",
            path.display()
        )
    })?;

    conn.pragma_update(None, "foreign_keys", "on")?;

    let current_version = user_version(&conn)?;

    ensure!(
        current_version <= LATEST_USER_VERSION,
        "database schema version {current_version} is newer than the latest version supported by this ytsub build ({LATEST_USER_VERSION})"
    );

    let target_version = match target_version {
        Some(target_version) => target_version,
        None => {
            ensure!(
                current_version > MIN_DOWNGRADE_USER_VERSION,
                "database schema version {current_version} has no supported previous version"
            );
            current_version - 1
        }
    };

    ensure!(
        target_version <= current_version,
        "cannot downgrade database schema version {current_version} to newer version {target_version}"
    );

    if target_version == current_version {
        return Ok(DowngradeOutcome::AlreadyAtTarget {
            version: current_version,
        });
    }

    let backup_path = backup_db(&conn, path, current_version)?;

    apply_down_migrations(&mut conn, current_version, target_version).with_context(|| {
        format!(
            "failed to downgrade database schema from {current_version} to {target_version}; the original database backup is at {}",
            backup_path.display()
        )
    })?;

    Ok(DowngradeOutcome::Downgraded {
        from: current_version,
        to: target_version,
        backup_path,
    })
}

fn apply_down_migrations(
    conn: &mut Connection,
    current_version: u8,
    target_version: u8,
) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    for version in ((target_version + 1)..=current_version).rev() {
        apply_down_migration(&tx, version).with_context(|| {
            format!(
                "failed to migrate database schema from {version} to {}",
                version - 1
            )
        })?;
    }

    tx.commit()?;
    Ok(())
}

fn apply_down_migration(tx: &Transaction<'_>, current_version: u8) -> Result<()> {
    match current_version {
        4 => {
            tx.execute("CREATE TABLE watched (video_id TEXT PRIMARY KEY)", [])?;
            tx.execute(
                "
                INSERT INTO watched (video_id)
                SELECT video_id
                FROM video_state
                WHERE watched = 1
                ",
                [],
            )?;
            tx.execute("DROP TABLE video_state", [])?;
        }
        3 => {
            tx.execute("ALTER TABLE videos ADD COLUMN watched BOOL", [])?;
            tx.execute(
                "
                UPDATE videos
                SET watched = EXISTS (
                    SELECT 1
                    FROM watched
                    WHERE watched.video_id = videos.video_id
                )
                ",
                [],
            )?;
            tx.execute("DROP TABLE watched", [])?;
            tx.execute("ALTER TABLE videos DROP COLUMN tab", [])?;
            tx.execute("ALTER TABLE videos DROP COLUMN members_only", [])?;
        }
        2 => {
            tx.execute("ALTER TABLE channels DROP COLUMN last_refreshed", [])?;
        }
        _ => bail!("no down migration from database schema version {current_version}"),
    }

    tx.pragma_update(None, "user_version", current_version - 1)?;
    Ok(())
}

fn backup_db(conn: &Connection, database_path: &Path, schema_version: u8) -> Result<PathBuf> {
    let original_name = database_path
        .file_name()
        .context("database path does not contain a filename")?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");

    let mut backup_name = original_name.to_os_string();
    backup_name.push(format!(".schema-{schema_version}.{timestamp}.bak"));

    let mut base_path = database_path.to_owned();
    base_path.set_file_name(backup_name);

    let (backup_path, partial_file) = reserve_backup_paths(&base_path)?;

    let permissions = fs::metadata(database_path)
        .with_context(|| {
            format!(
                "failed to read permissions from database at {}",
                database_path.display()
            )
        })?
        .permissions();

    fs::set_permissions(partial_file.path(), permissions).with_context(|| {
        format!(
            "failed to set permissions on backup at {}",
            partial_file.path().display()
        )
    })?;

    conn.backup(rusqlite::MAIN_DB, partial_file.path(), None)
        .with_context(|| {
            format!(
                "failed to create backup at {}",
                partial_file.path().display()
            )
        })?;

    fs::rename(partial_file.path(), &backup_path).with_context(|| {
        format!(
            "failed to move completed backup from {} to {}",
            partial_file.path().display(),
            backup_path.display()
        )
    })?;
    partial_file.keep();

    Ok(backup_path)
}

struct PartialFile {
    path: PathBuf,
    persisted: bool,
}

impl PartialFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            persisted: false,
        }
    }

    fn keep(mut self) {
        self.persisted = true;
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PartialFile {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn reserve_backup_paths(base: &Path) -> Result<(PathBuf, PartialFile)> {
    for number in 0.. {
        let mut backup_path = base.to_owned();

        if number != 0 {
            backup_path.set_extension(format!("{number}.bak"));
        }

        match backup_path.try_exists() {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", backup_path.display()));
            }
        }

        let mut partial_path = backup_path.clone();
        partial_path.as_mut_os_string().push(".partial");

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial_path)
        {
            Ok(_) => {
                let partial_file = PartialFile::new(partial_path);

                if backup_path.try_exists()? {
                    continue;
                }

                return Ok((backup_path, partial_file));
            }

            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to reserve {}", partial_path.display()));
            }
        }
    }

    unreachable!()
}

fn apply_up_migration(conn: &mut Connection, current_user_version: u8) -> Result<()> {
    match current_user_version {
        0 => {
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

            conn.pragma_update(None, "user_version", 1)?;
        }
        1 => {
            conn.execute("ALTER TABLE channels ADD COLUMN last_refreshed INTEGER", [])?;
            conn.pragma_update(None, "user_version", 2)?;
        }
        2 => {
            let tx = conn.transaction()?;

            tx.execute("ALTER TABLE videos ADD COLUMN tab INTEGER DEFAULT 0", [])?;
            tx.execute("ALTER TABLE videos ADD COLUMN members_only BOOL", [])?;

            {
                let mut stmt = tx.prepare("SELECT video_id FROM videos WHERE watched=true")?;

                let watched_videos = stmt
                    .query_map([], |row| row.get::<usize, String>(0))?
                    .filter_map(Result::ok)
                    .collect::<Vec<_>>();

                tx.execute(
                    "CREATE TABLE IF NOT EXISTS watched (video_id TEXT PRIMARY KEY)",
                    [],
                )?;

                if !watched_videos.is_empty() {
                    let query = build_bulk_stmt(
                        StatementType::AddWatched,
                        &["video_id"],
                        1..=watched_videos.len(),
                    );
                    tx.execute(&query, rusqlite::params_from_iter(watched_videos.iter()))?;
                }

                tx.execute("ALTER TABLE videos DROP COLUMN watched", [])?;
            }

            tx.pragma_update(None, "user_version", 3)?;

            tx.commit()?;
        }
        3 => {
            let tx = conn.transaction()?;

            tx.execute(
                "CREATE TABLE IF NOT EXISTS video_state (
                    video_id TEXT PRIMARY KEY,
                    watched INTEGER NOT NULL DEFAULT 0
                        CHECK (watched IN (0, 1)),
                    position INTEGER
                        CHECK (position IS NULL OR position >= 0)
                )",
                [],
            )?;

            tx.execute(
                "
                INSERT INTO video_state (video_id, watched, position)
                SELECT video_id, 1, NULL
                FROM watched
                ",
                [],
            )?;

            tx.execute("DROP TABLE watched", [])?;

            tx.pragma_update(None, "user_version", 4)?;

            tx.commit()?;
        }
        _ => unreachable!(),
    }

    Ok(())
}

pub fn create_channel(conn: &Connection, channel: &Channel) -> Result<()> {
    conn.execute(
        "INSERT INTO channels (channel_id, channel_name, last_refreshed)
        VALUES (?1, ?2, ?3)",
        params![channel.channel_id, channel.channel_name, utils::now().ok()],
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

pub fn set_last_refreshed_field(
    conn: &Connection,
    channel_id: &str,
    time: Option<u64>,
) -> Result<()> {
    let mut stmt = conn.prepare("UPDATE channels SET last_refreshed=?1 WHERE channel_id=?2")?;
    stmt.execute(params![time, channel_id])?;
    Ok(())
}

#[derive(Copy, Clone)]
enum StatementType {
    AddVideo,
    AddToTag,
    RemoveFromTag,
    GetChannels,
    GetLatestVideos,
    AddWatched,
}

fn build_bulk_stmt(
    query_type: StatementType,
    columns: &[&str],
    indices: RangeInclusive<usize>,
) -> String {
    let columns_str = columns.join(", ");
    let idxs = indices.collect::<Vec<_>>();
    let values_string = idxs
        .chunks(columns.len())
        .map(|chunk| {
            format!(
                "({})",
                chunk
                    .iter()
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    match query_type {
        StatementType::AddVideo => format!(
            "INSERT OR REPLACE INTO videos ({columns_str})
            VALUES {values_string}
            "
        ),
        StatementType::AddToTag => format!(
            "INSERT INTO tag_relations ({columns_str})
            VALUES {values_string}"
        ),
        StatementType::RemoveFromTag => format!(
            "DELETE FROM tag_relations WHERE ({columns_str}) IN ({values_string})"
        ),
        StatementType::GetChannels => format!(
            "SELECT DISTINCT channels.channel_id, channel_name, last_refreshed
            FROM channels, tag_relations
            WHERE tag_relations.channel_id=channels.channel_id AND tag_relations.tag_name IN ({values_string})
            ORDER BY channel_name COLLATE NOCASE ASC
            "
        ),
        StatementType::GetLatestVideos => format!(
            "SELECT DISTINCT videos.video_id, title, published, length, members_only, videos.channel_id,
            channel_name, COALESCE(video_state.watched, 0), position
            FROM videos
            JOIN channels ON channels.channel_id = videos.channel_id
            JOIN tag_relations ON tag_relations.channel_id = channels.channel_id
            LEFT JOIN video_state ON video_state.video_id = videos.video_id
            WHERE tag_relations.tag_name IN ({values_string}) AND videos.tab=?1
            ORDER BY videos.published DESC
            LIMIT 100
            "
        ),
        StatementType::AddWatched => format!(
            "INSERT INTO watched ({columns_str})
            VALUES {values_string}
            "
        )
    }
}

pub fn add_videos(
    conn: &Connection,
    channel_id: &str,
    videos: &[Video],
    tab: ChannelTab,
) -> Result<()> {
    let columns = [
        "video_id",
        "channel_id",
        "title",
        "published",
        "length",
        "members_only",
        "tab",
    ];

    let mut videos_values = Vec::with_capacity(videos.len() * columns.len());
    let tab = tab as u8;

    for video in videos {
        let values = params![
            video.video_id,
            channel_id,
            video.title,
            video.published,
            video.length,
            video.members_only,
            tab
        ];
        videos_values.extend_from_slice(values);
    }

    let query = build_bulk_stmt(StatementType::AddVideo, &columns, 1..=videos_values.len());

    conn.execute(&query, videos_values.as_slice())?;

    Ok(())
}

pub fn delete_video(conn: &Connection, video_id: &str) -> Result<()> {
    conn.execute("DELETE FROM videos WHERE video_id=?1", params![video_id])?;
    Ok(())
}

pub fn update_title(conn: &Connection, video_id: &str, title: &str) -> Result<()> {
    conn.execute(
        "UPDATE videos SET title=?1 WHERE video_id=?2",
        params![title, video_id],
    )?;

    Ok(())
}

pub fn get_channels(conn: &Connection, tags: &[&str]) -> Result<Vec<Channel>> {
    let mut stmt;
    let values;

    if tags.is_empty() {
        values = rusqlite::params_from_iter([].iter());

        stmt = conn.prepare(
            "SELECT channel_id, channel_name, last_refreshed
            FROM channels
            ORDER BY channel_name COLLATE NOCASE ASC
            ",
        )?;
    } else {
        values = rusqlite::params_from_iter(tags.iter());

        stmt = conn.prepare(&build_bulk_stmt(
            StatementType::GetChannels,
            &["tag_name"],
            1..=tags.len(),
        ))?;
    }

    let mut channels = Vec::new();
    for channel in stmt.query_map(values, |row| {
        let channel_id: String = row.get(0)?;
        let channel_name: String = row.get(1)?;
        let last_refreshed: Option<u64> = row.get(2)?;
        Ok(Channel::new(channel_id, channel_name, last_refreshed))
    })? {
        channels.push(channel?);
    }

    Ok(channels)
}

pub fn get_videos(
    conn: &Connection,
    channel_id: &str,
    tab: ChannelTab,
) -> Result<Vec<VideoListItem>> {
    let mut stmt = conn.prepare(
        "SELECT videos.video_id, title, published, length, members_only,
        COALESCE(video_state.watched, 0), position
        FROM videos
        LEFT JOIN video_state ON video_state.video_id = videos.video_id
        WHERE videos.channel_id=?1 AND videos.tab=?2
        ORDER BY videos.published DESC
        ",
    )?;
    let mut videos = Vec::new();
    for video in stmt.query_map(params![channel_id, tab as u8], |row| {
        let published = row.get(2)?;

        Ok(VideoListItem {
            video: Video {
                video_id: row.get(0)?,
                title: row.get(1)?,
                published,
                length: row.get(3)?,
                members_only: row.get(4).unwrap_or_default(),
            },
            channel_id: channel_id.to_owned(),
            channel_name: None,
            published_text: utils::published_text(row.get(2)?, tab == ChannelTab::Streams)
                .unwrap_or_default(),
            watched: row.get(5)?,
            position: row.get(6)?,
            is_new: false,
        })
    })? {
        videos.push(video?);
    }

    Ok(videos)
}

pub fn get_latest_videos(
    conn: &Connection,
    tags: &[&str],
    tab: ChannelTab,
) -> Result<Vec<VideoListItem>> {
    let mut stmt;
    let mut values = Vec::with_capacity(tags.len() + 1);
    let tab_param = params![tab as u8];
    values.extend_from_slice(tab_param);

    if tags.is_empty() {
        stmt = conn.prepare(
            "SELECT videos.video_id, title, published, length, members_only, videos.channel_id,
            channel_name, COALESCE(video_state.watched, 0), position
            FROM videos
            JOIN channels ON channels.channel_id = videos.channel_id
            LEFT JOIN video_state ON video_state.video_id = videos.video_id
            WHERE videos.tab=?1
            ORDER BY videos.published DESC
            LIMIT 100
            ",
        )?;
    } else {
        for tag in tags {
            let tag = params![*tag];
            values.extend_from_slice(tag);
        }

        stmt = conn.prepare(&build_bulk_stmt(
            StatementType::GetLatestVideos,
            &["tag_name"],
            2..=values.len(),
        ))?;
    }
    let mut videos = Vec::new();

    for video in stmt.query_map(values.as_slice(), |row| {
        let published = row.get(2)?;

        Ok(VideoListItem {
            video: Video {
                video_id: row.get(0)?,
                title: row.get(1)?,
                published,
                length: row.get(3)?,
                members_only: row.get(4).unwrap_or_default(),
            },
            channel_id: row.get(5)?,
            channel_name: Some(row.get(6)?),
            published_text: utils::published_text(row.get(2)?, tab == ChannelTab::Streams)
                .unwrap_or_default(),
            watched: row.get(7)?,
            position: row.get(8)?,
            is_new: false,
        })
    })? {
        videos.push(video?);
    }

    Ok(videos)
}

pub fn set_watched(conn: &Connection, video_id: &str, watched: bool) -> Result<()> {
    let mut stmt = conn.prepare(
        "
        INSERT INTO video_state (video_id, watched)
        VALUES (?1, ?2)
        ON CONFLICT(video_id) DO UPDATE SET watched = excluded.watched
        ",
    )?;

    stmt.execute(params![video_id, watched])?;
    Ok(())
}

pub fn set_position(conn: &Connection, video_id: &str, position: u64) -> Result<()> {
    let mut stmt = conn.prepare(
        "
        INSERT INTO video_state (video_id, position)
        VALUES (?1, ?2)
        ON CONFLICT(video_id) DO UPDATE SET position = excluded.position
        ",
    )?;

    stmt.execute(params![video_id, position])?;
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
        let query = build_bulk_stmt(StatementType::AddToTag, &columns, 1..=values.len());
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

    let query = build_bulk_stmt(StatementType::RemoveFromTag, &columns, 1..=values.len());
    conn.execute(&query, values.as_slice())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DowngradeOutcome, LATEST_USER_VERSION, apply_up_migration, backup_db, downgrade_database,
        open_db, reserve_backup_paths, user_version,
    };
    use anyhow::Result;
    use rusqlite::Connection;
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    use tempfile::tempdir;

    const VIDEO_ID: &str = "test-video";
    const UNWATCHED_VIDEO_ID: &str = "unwatched-video";

    fn completed_backups(directory: &Path) -> Result<Vec<PathBuf>> {
        let mut backups = fs::read_dir(directory)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(".bak"))
            })
            .collect::<Vec<_>>();

        backups.sort();
        Ok(backups)
    }

    fn create_schema_three_database(path: &Path) -> Result<()> {
        let mut conn = Connection::open(path)?;

        apply_up_migration(&mut conn, 0)?;
        apply_up_migration(&mut conn, 1)?;

        conn.execute(
            "INSERT INTO channels (channel_id, channel_name) VALUES ('test-channel', 'Test')",
            [],
        )?;
        conn.execute(
            "
            INSERT INTO videos (video_id, channel_id, title, published, length, watched)
            VALUES (?1, 'test-channel', 'Test video', 0, 60, true)
            ",
            [VIDEO_ID],
        )?;

        apply_up_migration(&mut conn, 2)?;
        Ok(())
    }

    fn create_schema_two_database(path: &Path) -> Result<()> {
        let mut conn = Connection::open(path)?;

        apply_up_migration(&mut conn, 0)?;
        apply_up_migration(&mut conn, 1)?;
        conn.execute(
            "
            INSERT INTO channels (channel_id, channel_name, last_refreshed)
            VALUES ('test-channel', 'Test', 123)
            ",
            [],
        )?;
        conn.execute(
            "
            INSERT INTO videos (video_id, channel_id, title, published, length, watched)
            VALUES (?1, 'test-channel', 'Test video', 0, 60, true)
            ",
            [VIDEO_ID],
        )?;

        Ok(())
    }

    fn create_schema_four_database(path: &Path) -> Result<()> {
        let conn = open_db(path)?;

        conn.execute(
            "
            INSERT INTO channels (channel_id, channel_name, last_refreshed)
            VALUES ('test-channel', 'Test', 123)
            ",
            [],
        )?;
        conn.execute(
            "
            INSERT INTO videos (
                video_id, channel_id, title, published, length, tab, members_only
            ) VALUES
                (?1, 'test-channel', 'Watched video', 0, 60, 1, true),
                (?2, 'test-channel', 'Unwatched video', 0, 60, 2, false)
            ",
            [VIDEO_ID, UNWATCHED_VIDEO_ID],
        )?;
        conn.execute(
            "
            INSERT INTO video_state (video_id, watched, position) VALUES
                (?1, true, 42),
                (?2, false, 24)
            ",
            [VIDEO_ID, UNWATCHED_VIDEO_ID],
        )?;

        Ok(())
    }

    fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
        Ok(conn.query_row(
            "
            SELECT EXISTS (
                SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1
            )
            ",
            [table],
            |row| row.get(0),
        )?)
    }

    fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
        let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?;
        let columns = stmt
            .query_map([table], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(columns)
    }

    fn assert_downgraded(outcome: DowngradeOutcome, expected_from: u8, expected_to: u8) -> PathBuf {
        match outcome {
            DowngradeOutcome::Downgraded {
                from,
                to,
                backup_path,
            } => {
                assert_eq!(from, expected_from);
                assert_eq!(to, expected_to);
                backup_path
            }
            DowngradeOutcome::AlreadyAtTarget { version } => {
                panic!("expected downgrade, but database was already at schema {version}")
            }
        }
    }

    #[test]
    fn fresh_and_current_databases_are_not_backed_up() -> Result<()> {
        let directory = tempdir()?;
        let database_path = directory.path().join("videos.db");

        let conn = open_db(&database_path)?;
        assert_eq!(user_version(&conn)?, LATEST_USER_VERSION);
        assert!(completed_backups(directory.path())?.is_empty());

        let conn = open_db(&database_path)?;
        assert_eq!(user_version(&conn)?, LATEST_USER_VERSION);
        assert!(completed_backups(directory.path())?.is_empty());

        Ok(())
    }

    #[test]
    fn migration_creates_a_backup_of_the_previous_schema() -> Result<()> {
        let directory = tempdir()?;
        let database_path = directory.path().join("videos.db");
        create_schema_three_database(&database_path)?;

        let conn = open_db(&database_path)?;
        assert_eq!(user_version(&conn)?, LATEST_USER_VERSION);
        assert!(conn.query_row(
            "SELECT watched FROM video_state WHERE video_id = ?1",
            [VIDEO_ID],
            |row| row.get::<_, bool>(0),
        )?);

        let backups = completed_backups(directory.path())?;
        assert_eq!(backups.len(), 1);
        assert!(
            backups[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".schema-3.")
        );

        let backup = Connection::open(&backups[0])?;
        assert_eq!(user_version(&backup)?, 3);
        assert_eq!(
            backup.query_row(
                "SELECT COUNT(*) FROM watched WHERE video_id = ?1",
                [VIDEO_ID],
                |row| row.get::<_, u32>(0),
            )?,
            1
        );

        Ok(())
    }

    #[test]
    fn backup_name_collision_uses_the_next_number() -> Result<()> {
        let directory = tempdir()?;
        let base_path = directory
            .path()
            .join("videos.db.schema-3.20260812T143052Z.bak");
        fs::write(&base_path, b"existing backup")?;

        let (backup_path, partial_file) = reserve_backup_paths(&base_path)?;

        assert_eq!(
            backup_path,
            directory
                .path()
                .join("videos.db.schema-3.20260812T143052Z.1.bak")
        );
        assert_eq!(
            partial_file.path(),
            directory
                .path()
                .join("videos.db.schema-3.20260812T143052Z.1.bak.partial")
        );
        assert_eq!(fs::read(&base_path)?, b"existing backup");

        Ok(())
    }

    #[test]
    fn backup_error_removes_the_partial_file() -> Result<()> {
        let directory = tempdir()?;
        let nonexistent_database = directory.path().join("missing.db");
        let conn = Connection::open_in_memory()?;

        assert!(backup_db(&conn, &nonexistent_database, 0).is_err());
        assert!(fs::read_dir(directory.path())?.next().is_none());

        Ok(())
    }

    #[test]
    fn downgrade_four_to_three_preserves_watched_state_and_backup() -> Result<()> {
        let directory = tempdir()?;
        let database_path = directory.path().join("videos.db");
        create_schema_four_database(&database_path)?;

        let backup_path = assert_downgraded(downgrade_database(&database_path, None)?, 4, 3);

        let conn = Connection::open(&database_path)?;
        assert_eq!(user_version(&conn)?, 3);
        assert!(table_exists(&conn, "watched")?);
        assert!(!table_exists(&conn, "video_state")?);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM watched", [], |row| {
                row.get::<_, u32>(0)
            })?,
            1
        );
        assert_eq!(
            conn.query_row("SELECT video_id FROM watched", [], |row| {
                row.get::<_, String>(0)
            })?,
            VIDEO_ID
        );

        let backup = Connection::open(&backup_path)?;
        assert_eq!(user_version(&backup)?, 4);
        assert_eq!(
            backup.query_row(
                "SELECT position FROM video_state WHERE video_id = ?1",
                [VIDEO_ID],
                |row| row.get::<_, u64>(0),
            )?,
            42
        );
        assert_eq!(completed_backups(directory.path())?, vec![backup_path]);

        Ok(())
    }

    #[test]
    fn downgrade_three_to_two_restores_watched_column() -> Result<()> {
        let directory = tempdir()?;
        let database_path = directory.path().join("videos.db");
        create_schema_three_database(&database_path)?;

        let conn = Connection::open(&database_path)?;
        conn.execute(
            "UPDATE videos SET tab = 2, members_only = true WHERE video_id = ?1",
            [VIDEO_ID],
        )?;

        assert_downgraded(downgrade_database(&database_path, Some(2))?, 3, 2);

        let conn = Connection::open(&database_path)?;
        assert_eq!(user_version(&conn)?, 2);
        assert!(!table_exists(&conn, "watched")?);
        assert_eq!(
            table_columns(&conn, "videos")?,
            [
                "video_id",
                "channel_id",
                "title",
                "published",
                "length",
                "watched",
            ]
        );
        assert!(conn.query_row(
            "SELECT watched FROM videos WHERE video_id = ?1",
            [VIDEO_ID],
            |row| row.get::<_, bool>(0),
        )?);

        Ok(())
    }

    #[test]
    fn downgrade_two_to_one_removes_refresh_timestamp() -> Result<()> {
        let directory = tempdir()?;
        let database_path = directory.path().join("videos.db");
        create_schema_two_database(&database_path)?;

        assert_downgraded(downgrade_database(&database_path, Some(1))?, 2, 1);

        let conn = Connection::open(&database_path)?;
        assert_eq!(user_version(&conn)?, 1);
        assert_eq!(
            table_columns(&conn, "channels")?,
            ["channel_id", "channel_name"]
        );
        assert_eq!(
            conn.query_row(
                "SELECT channel_name FROM channels WHERE channel_id = 'test-channel'",
                [],
                |row| row.get::<_, String>(0),
            )?,
            "Test"
        );
        assert!(conn.query_row(
            "SELECT watched FROM videos WHERE video_id = ?1",
            [VIDEO_ID],
            |row| row.get::<_, bool>(0),
        )?);

        Ok(())
    }

    #[test]
    fn multi_step_downgrade_reaches_target_and_creates_one_backup() -> Result<()> {
        let directory = tempdir()?;
        let database_path = directory.path().join("videos.db");
        create_schema_four_database(&database_path)?;

        let backup_path = assert_downgraded(downgrade_database(&database_path, Some(1))?, 4, 1);

        let conn = Connection::open(&database_path)?;
        assert_eq!(user_version(&conn)?, 1);
        assert_eq!(
            table_columns(&conn, "channels")?,
            ["channel_id", "channel_name"]
        );
        assert_eq!(
            table_columns(&conn, "videos")?,
            [
                "video_id",
                "channel_id",
                "title",
                "published",
                "length",
                "watched",
            ]
        );
        assert!(conn.query_row(
            "SELECT watched FROM videos WHERE video_id = ?1",
            [VIDEO_ID],
            |row| row.get::<_, bool>(0),
        )?);
        assert!(!conn.query_row(
            "SELECT watched FROM videos WHERE video_id = ?1",
            [UNWATCHED_VIDEO_ID],
            |row| row.get::<_, bool>(0),
        )?);

        assert_eq!(completed_backups(directory.path())?, vec![backup_path]);

        Ok(())
    }

    #[test]
    fn downgraded_database_can_be_migrated_forward_again() -> Result<()> {
        let directory = tempdir()?;
        let database_path = directory.path().join("videos.db");
        create_schema_four_database(&database_path)?;

        assert_downgraded(downgrade_database(&database_path, Some(1))?, 4, 1);

        let conn = open_db(&database_path)?;
        assert_eq!(user_version(&conn)?, 4);
        assert!(conn.query_row(
            "SELECT watched FROM video_state WHERE video_id = ?1",
            [VIDEO_ID],
            |row| row.get::<_, bool>(0),
        )?);
        assert_eq!(
            conn.query_row(
                "SELECT position FROM video_state WHERE video_id = ?1",
                [VIDEO_ID],
                |row| row.get::<_, Option<u64>>(0),
            )?,
            None
        );

        Ok(())
    }

    #[test]
    fn downgrade_validates_paths_and_schema_versions_before_backup() -> Result<()> {
        let directory = tempdir()?;
        let missing_path = directory.path().join("missing.db");

        assert!(downgrade_database(&missing_path, Some(3)).is_err());
        assert!(!missing_path.exists());

        let database_path = directory.path().join("videos.db");
        create_schema_three_database(&database_path)?;

        assert_eq!(
            downgrade_database(&database_path, Some(3))?,
            DowngradeOutcome::AlreadyAtTarget { version: 3 }
        );
        assert!(downgrade_database(&database_path, Some(0)).is_err());

        let oldest_database_path = directory.path().join("oldest.db");
        let mut conn = Connection::open(&oldest_database_path)?;
        apply_up_migration(&mut conn, 0)?;
        assert!(downgrade_database(&oldest_database_path, None).is_err());

        let older_database_path = directory.path().join("older.db");
        create_schema_two_database(&older_database_path)?;
        assert!(downgrade_database(&older_database_path, Some(3)).is_err());

        let future_database_path = directory.path().join("future.db");
        create_schema_four_database(&future_database_path)?;
        let conn = Connection::open(&future_database_path)?;
        conn.pragma_update(None, "user_version", 5)?;
        assert!(downgrade_database(&future_database_path, Some(3)).is_err());

        assert!(completed_backups(directory.path())?.is_empty());

        Ok(())
    }
}
