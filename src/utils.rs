use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const SUBS_FILE: &str = "subs";
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
