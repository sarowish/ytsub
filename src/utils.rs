use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

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
