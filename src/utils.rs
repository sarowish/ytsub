use crate::{CONFIG, http};
use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDateTime};
use regex_lite::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const INSTANCES_FILE: &str = "instances";
const DATABASE_FILE: &str = "videos.db";

pub fn get_config_dir() -> Result<PathBuf> {
    let path = match dirs::config_dir() {
        Some(path) => path.join(PACKAGE_NAME),
        None => bail!("Couldn't find config directory"),
    };

    Ok(path)
}

pub fn get_data_dir() -> Result<PathBuf> {
    let path = match dirs::data_local_dir() {
        Some(path) => path.join(PACKAGE_NAME),
        None => bail!("Couldn't find local data directory"),
    };

    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }

    Ok(path)
}

pub fn get_cache_dir() -> Result<PathBuf> {
    let path = match dirs::cache_dir() {
        Some(path) => path.join(PACKAGE_NAME),
        None => bail!("Couldn't find cache directory"),
    };

    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }

    Ok(path)
}

fn hyperlink(text: &str, link: &str) -> String {
    format!("\x1b]8;;{link}\x1b\\{text}\x1b]8;;\x1b\\")
}

pub async fn fetch_invidious_instances() -> Result<Vec<String>> {
    const REQUEST_URL: &str = "https://api.invidious.io/instances.json";
    const ONION: &str = "onion";
    let instances: Value = http::client()?
        .get(REQUEST_URL)
        .send()
        .await?
        .json()
        .await?;
    Ok(instances
        .as_array()
        .unwrap()
        .iter()
        .map(|arr| arr.as_array().unwrap())
        .filter(|instance| {
            let instance = &instance[1];
            instance["type"].as_str().unwrap() != ONION
                && instance["api"].as_bool().unwrap_or(false)
        })
        .map(|instance| instance[1]["uri"].as_str().unwrap().to_string())
        .collect())
}

pub fn get_default_instances_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join(INSTANCES_FILE))
}

pub async fn generate_instances_file() -> Result<()> {
    let instances = fetch_invidious_instances().await?;
    let instances_file_path = &CONFIG.instances;
    let instances_dir = instances_file_path.parent().unwrap();

    if !instances_dir.exists() {
        std::fs::create_dir_all(instances_dir)?;
    }

    anyhow::ensure!(
        !instances.is_empty(),
        format!(
            "No suitable instance available on {}",
            hyperlink("api.invidious.io", "https://api.invidious.io/")
        )
    );

    let mut file = File::create(instances_file_path.as_path())?;
    println!(
        "Generated \"{}\" with the following instances:",
        instances_file_path.display()
    );
    for instance in instances {
        writeln!(file, "{instance}")?;
        println!("{instance}");
    }
    Ok(())
}

pub fn read_instances() -> Result<Vec<String>> {
    let file = File::open(&CONFIG.instances)?;
    let mut instances = Vec::new();
    for instance in BufReader::new(file).lines() {
        instances.push(instance?);
    }
    Ok(instances)
}

pub fn get_default_database_file() -> Result<PathBuf> {
    Ok(get_data_dir()?.join(DATABASE_FILE))
}

pub fn length_as_seconds(length: &str) -> Option<u32> {
    let mut total = 0;

    for t in length.split(':') {
        total *= 60;
        total += t.parse::<u32>().ok()?;
    }

    Some(total)
}

pub fn length_from_accessibility_label(label: &str) -> Option<u32> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"((?<days>\d+) days?(, )?)?((?<hours>\d+) hours?(, )?)?((?<minutes>\d+) minutes?(, )?)?((?<seconds>\d+) seconds?)?$",
        )
        .unwrap()
    });

    let captures = RE.captures(label)?;

    let days = captures
        .name("days")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .unwrap_or(0);
    let hours = captures
        .name("hours")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .unwrap_or(0);
    let minutes = captures
        .name("minutes")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .unwrap_or(0);
    let seconds = captures
        .name("seconds")
        .and_then(|m| m.as_str().parse::<u32>().ok())
        .unwrap_or(0);

    let length = days * 86400 + hours * 3600 + minutes * 60 + seconds;

    Some(length).filter(|length| *length != 0)
}

pub fn params_from_url(url: &str) -> Result<HashMap<String, String>> {
    let parsed_url = Url::parse(url)?;

    Ok(parsed_url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect())
}

pub fn length_as_hhmmss(length: u32) -> String {
    let seconds = length % 60;
    let minutes = (length / 60) % 60;
    let hours = (length / 60) / 60;
    match (hours, minutes, seconds) {
        (0, 0, _) => format!("0:{seconds:02}"),
        (0, _, _) => format!("{minutes}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

const MINUTE: u64 = 60;
const HOUR: u64 = 3600;
const DAY: u64 = 86400;
const WEEK: u64 = 604800;
const MONTH: u64 = 2592000;
const YEAR: u64 = 31536000;

pub fn published_text_as_timestamp(published_text: &str) -> Result<u64> {
    let (num, time_frame) = {
        let v: Vec<&str> = published_text.splitn(2, ' ').collect();

        match (v[0].parse::<u64>(), v.get(1)) {
            (Ok(num), Some(rest)) => (num, *rest),
            _ => (
                v[0].trim_end_matches(char::is_alphabetic).parse()?,
                v[0].trim_start_matches(char::is_numeric),
            ),
        }
    };

    if time_frame == "waiting" {
        return Err(anyhow::anyhow!("Not a valid published text"));
    }

    let from_now = if time_frame.starts_with('s') {
        num
    } else if time_frame.starts_with("mi") {
        num * MINUTE
    } else if time_frame.starts_with('h') {
        num * HOUR
    } else if time_frame.starts_with('d') {
        num * DAY
    } else if time_frame.starts_with('w') {
        num * WEEK
    } else if time_frame.starts_with("mo") {
        num * MONTH
    } else if time_frame.starts_with('y') {
        num * YEAR
    } else {
        return Err(anyhow::anyhow!("Not a valid published text"));
    };

    Ok(now()?.saturating_sub(from_now))
}

pub fn premiere_text_as_timestamp(text: &str) -> Option<u64> {
    let text = text
        .strip_prefix("Premieres ")
        .or_else(|| text.strip_prefix("Scheduled for"))?;

    let date = NaiveDateTime::parse_from_str(text, "%m/%d/%y, %I:%M %p").ok()?;
    u64::try_from(date.and_utc().timestamp()).ok()
}

pub fn published_text(published: u64, stream: bool) -> Option<String> {
    let now = now().ok()?;

    let text = if published > now {
        let formatted_timestamp =
            DateTime::from_timestamp(published.cast_signed(), 0).map(|date| {
                date.with_timezone(&chrono::Local)
                    .format(&CONFIG.datetime_format)
                    .to_string()
            })?;

        format!(
            "{} {formatted_timestamp}",
            if stream { "Scheduled for" } else { "Premieres" }
        )
    } else {
        let time_diff = now.abs_diff(published);
        let (num, mut time_frame) = if time_diff < MINUTE {
            (time_diff, "second".to_string())
        } else if time_diff < HOUR {
            (time_diff / MINUTE, "minute".to_string())
        } else if time_diff < DAY {
            (time_diff / HOUR, "hour".to_string())
        } else if time_diff < WEEK * 2 {
            (time_diff / DAY, "day".to_string())
        } else if time_diff < MONTH {
            (time_diff / WEEK, "week".to_string())
        } else if time_diff < YEAR {
            (time_diff / MONTH, "month".to_string())
        } else {
            (time_diff / YEAR, "year".to_string())
        };

        if num > 1 {
            time_frame.push('s');
        }

        format!(
            "{} {num} {time_frame} ago",
            if stream { "Streamed" } else { "Shared" },
        )
    };

    Some(text)
}

pub fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

pub fn time_passed(time: u64) -> Result<u64> {
    Ok(now()?.saturating_sub(time))
}

pub fn binary_exists(program: &str) -> bool {
    which::which(program).is_ok()
}

pub fn env_var_is_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|var| !var.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        length_as_hhmmss, length_as_seconds, length_from_accessibility_label, now, params_from_url,
        premiere_text_as_timestamp, published_text, published_text_as_timestamp,
    };

    #[test]
    fn extract_params() {
        let url = "https://example.com/products?page=2&sort=desc";
        let params = params_from_url(url).ok();

        assert_eq!(
            params
                .as_ref()
                .and_then(|hm| hm.get("page").map(ToOwned::to_owned)),
            Some(String::from("2"))
        );
        assert_eq!(
            params.and_then(|hm| hm.get("sort").map(ToOwned::to_owned)),
            Some(String::from("desc"))
        );
    }

    #[test]
    fn length_conversion() {
        const SECONDS: u32 = 5409;
        const TEXT: &str = "1:30:09";

        assert_eq!(length_as_hhmmss(SECONDS), TEXT);
        assert_eq!(length_as_seconds(TEXT), Some(SECONDS));
    }

    #[test]
    fn published_conversion() {
        const TEXT: &str = "5 days ago";
        let time = now().unwrap().saturating_sub(432000);

        assert_eq!(published_text_as_timestamp(TEXT).unwrap(), time);
        assert_eq!(
            published_text(time, false).unwrap(),
            "Shared ".to_owned() + TEXT
        );
        assert_eq!(
            published_text(time, true).unwrap(),
            "Streamed ".to_owned() + TEXT
        );
    }

    #[test]
    fn premiere_conversion() {
        let mut text = "Premieres 5/27/26, 4:00 PM";
        assert_eq!(premiere_text_as_timestamp(text), Some(1779897600));

        text = "Scheduled for 5/27/26, 1:45 PM";
        assert_eq!(premiere_text_as_timestamp(text), Some(1779889500));

        text = "5 days ago";
        assert_eq!(premiere_text_as_timestamp(text), None);
    }

    #[test]
    fn published_handles_malformed_input_without_panic() {
        // Inputs lacking a space previously panicked with index-out-of-bounds
        // when `splitn(2, ' ')` returned a single element and the numeric
        // branch unconditionally indexed `v[1]`. They must now return Err.
        assert!(published_text_as_timestamp("").is_err());
        assert!(published_text_as_timestamp("5").is_err());
        assert!(published_text_as_timestamp("123").is_err());
    }

    #[test]
    fn published_text_rejects_waiting() {
        let text = "1 waiting";
        assert!(published_text_as_timestamp(text).is_err());
    }

    #[test]
    fn accessibility_label_length_conversion() {
        assert_eq!(
            length_from_accessibility_label(
                "Share Your #NASAMoonCrew and Get Excited for Artemis II 53 seconds",
            ),
            Some(53)
        );
        assert_eq!(
            length_from_accessibility_label(
                "Artemis II Moon Mission Complete! 3 minutes, 33 seconds",
            ),
            Some(213)
        );
        assert_eq!(
            length_from_accessibility_label(
                "Rhythmicity and Coordination - Russell Foster 1 hour, 10 minutes",
            ),
            Some(4200)
        );
        assert_eq!(
            length_from_accessibility_label(
                "240 Hour Countdown Timer - Longest Timer on YouTube 10 days",
            ),
            Some(864000)
        );
        assert_eq!(
            length_from_accessibility_label("No length in the accessibilty label"),
            None
        );
    }
}
