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
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }
    Ok(path)
}

fn get_data_dir() -> Result<PathBuf> {
    let path = match dirs::data_local_dir() {
        Some(path) => path.join(PACKAGE_NAME),
        None => bail!("Couldn't find local data directory"),
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
    let mut file = File::create(instances_file_path.as_path())?;
    println!(
        "Generated \"{}\" with the following instances:",
        instances_file_path.display()
    );
    for instance in instances {
        writeln!(file, "{}", instance)?;
        println!("{}", instance);
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

pub fn as_hhmmss(length: u32) -> String {
    let seconds = length % 60;
    let minutes = (length / 60) % 60;
    let hours = (length / 60) / 60;
    match (hours, minutes, seconds) {
        (0, 0, _) => format!("0:{:02}", seconds),
        (0, _, _) => format!("{}:{:02}", minutes, seconds),
        _ => format!("{}:{:02}:{:02}", hours, minutes, seconds),
    }
}

const MINUTE: u64 = 60;
const HOUR: u64 = 3600;
const DAY: u64 = 86400;
const WEEK: u64 = 604800;
const MONTH: u64 = 2592000;
const YEAR: u64 = 31536000;

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
        format!("Premieres in {} {}", num, time_frame)
    } else {
        format!("Shared {} {} ago", num, time_frame)
    })
}

pub fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

pub fn time_passed(time: u64) -> Result<u64> {
    Ok(now()?.saturating_sub(time))
}
