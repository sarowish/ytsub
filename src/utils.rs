use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const SUBS_FILE: &str = "subs";
const INSTANCES_FILE: &str = "instances";
const DATABASE_FILE: &str = "videos.db";

fn get_config_dir() -> PathBuf {
    let path = dirs::config_dir().unwrap().join(PACKAGE_NAME);
    if !path.exists() {
        std::fs::create_dir(&path).unwrap();
    }
    path
}

fn get_data_dir() -> PathBuf {
    let path = dirs::data_local_dir().unwrap().join(PACKAGE_NAME);
    if !path.exists() {
        std::fs::create_dir(&path).unwrap();
    }
    path
}

fn fetch_invidious_instances() -> Vec<String> {
    const REQUEST_URL: &str = "https://api.invidious.io/instances.json";
    const ONION: &str = "onion";
    let agent = ureq::agent();
    let instances: Value = agent.get(REQUEST_URL).call().unwrap().into_json().unwrap();
    instances
        .as_array()
        .unwrap()
        .iter()
        .map(|arr| arr.as_array().unwrap())
        .filter(|instance| {
            let instance = &instance[1];
            instance["type"].as_str().unwrap() != ONION && instance["api"].as_bool().unwrap()
        })
        .map(|instance| instance[1]["uri"].as_str().unwrap().to_string())
        .collect()
}

fn get_instances_file() -> PathBuf {
    get_config_dir().join(INSTANCES_FILE)
}

pub fn generate_instances_file() {
    let instances = fetch_invidious_instances();
    let instances_file_path = get_instances_file();
    let mut file = File::create(instances_file_path).unwrap();
    for instance in instances {
        writeln!(file, "{}", instance).unwrap();
    }
}

pub fn read_instances() -> Vec<String> {
    let file = File::open(get_instances_file()).expect(
        "Instances file doesn't exist. Create it
        by running the program with -g flag.",
    );
    BufReader::new(file).lines().map(|id| id.unwrap()).collect()
}

fn get_subscriptions_file() -> PathBuf {
    get_config_dir().join(SUBS_FILE)
}

pub fn read_subscriptions(path: Option<PathBuf>) -> Vec<String> {
    let path = path.unwrap_or_else(get_subscriptions_file);
    let file = File::open(path).unwrap();
    BufReader::new(file).lines().map(|id| id.unwrap()).collect()
}

pub fn get_database_file() -> PathBuf {
    get_data_dir().join(DATABASE_FILE)
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

pub fn published_text(published: u32) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let passed = now - published as u64;
    let (num, mut time_frame) = if passed < MINUTE {
        (passed, "second".to_string())
    } else if passed < HOUR {
        (passed / MINUTE, "minute".to_string())
    } else if passed < DAY {
        (passed / HOUR, "hour".to_string())
    } else if passed < WEEK * 2 {
        (passed / DAY, "day".to_string())
    } else if passed < MONTH {
        (passed / WEEK, "week".to_string())
    } else if passed < YEAR {
        (passed / MONTH, "month".to_string())
    } else {
        (passed / YEAR, "year".to_string())
    };
    if num > 1 {
        time_frame.push('s');
    }
    format!("{} {} ago", num, time_frame)
}
