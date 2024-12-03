use anyhow::{bail, Result};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::CONFIG;

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

pub fn fetch_invidious_instances() -> Result<Vec<String>> {
    const REQUEST_URL: &str = "https://api.invidious.io/instances.json";
    const ONION: &str = "onion";
    let agent = ureq::agent();
    let instances: Value = agent.get(REQUEST_URL).call()?.into_json()?;
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

pub fn generate_instances_file() -> Result<()> {
    let instances = fetch_invidious_instances()?;
    let instances_file_path = &CONFIG.options.instances;
    let instances_dir = instances_file_path.parent().unwrap();

    if !instances_dir.exists() {
        std::fs::create_dir_all(instances_dir)?;
    }

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
    let file = File::open(&CONFIG.options.instances)?;
    let mut instances = Vec::new();
    for instance in BufReader::new(file).lines() {
        instances.push(instance?);
    }
    Ok(instances)
}

pub fn get_default_database_file() -> Result<PathBuf> {
    Ok(get_data_dir()?.join(DATABASE_FILE))
}

pub fn length_as_seconds(length: &str) -> u32 {
    let mut total = 0;

    for t in length.split(':') {
        total *= 60;
        total += t.parse::<u32>().unwrap();
    }

    total
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

pub fn published(published_text: &str) -> Result<u64> {
    let (num, time_frame) = {
        let v: Vec<&str> = published_text.splitn(2, ' ').collect();

        match v[0].parse::<u64>() {
            Ok(num) => (num, v[1]),
            _ => (
                v[0].trim_end_matches(char::is_alphabetic).parse().unwrap(),
                v[0].trim_start_matches(char::is_numeric),
            ),
        }
    };

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
        panic!()
    };

    Ok(now()?.saturating_sub(from_now))
}

pub fn published_text(published: u64) -> Result<String> {
    let now = now()?;
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
    Ok(if published > now {
        format!("Premieres in {num} {time_frame}")
    } else {
        format!("Shared {num} {time_frame} ago")
    })
}

pub fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

pub fn time_passed(time: u64) -> Result<u64> {
    Ok(now()?.saturating_sub(time))
}

#[cfg(test)]
mod tests {
    use super::{length_as_hhmmss, length_as_seconds, now, published, published_text};

    #[test]
    fn length_conversion() {
        const SECONDS: u32 = 5409;
        const TEXT: &str = "1:30:09";

        assert_eq!(length_as_hhmmss(SECONDS), TEXT);
        assert_eq!(length_as_seconds(TEXT), SECONDS);
    }

    #[test]
    fn published_conversion() {
        const TEXT: &str = "5 days ago";
        let time = now().unwrap().saturating_sub(432000);

        assert_eq!(published(TEXT).unwrap(), time);
        assert_eq!(published_text(time).unwrap(), "Shared ".to_owned() + TEXT);
    }
}
