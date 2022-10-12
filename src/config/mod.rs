pub mod keys;
pub mod options;
pub mod theme;

use self::{
    keys::{KeyBindings, UserKeyBindings},
    options::{Options, UserOptions},
    theme::{Theme, UserTheme},
};
use crate::{utils, CLAP_ARGS};
use anyhow::Result;
use serde::Deserialize;
use std::{fs, path::PathBuf};

const CONFIG_FILE: &str = "config.toml";

#[derive(Deserialize)]
struct UserConfig {
    #[serde(flatten)]
    options: Option<UserOptions>,
    #[serde(flatten)]
    theme: Option<UserTheme>,
    key_bindings: Option<UserKeyBindings>,
}

#[derive(Default)]
pub struct Config {
    pub options: Options,
    pub theme: Theme,
    pub key_bindings: KeyBindings,
}

impl Config {
    pub fn new() -> Result<Self> {
        let config_file = match CLAP_ARGS.value_of("config") {
            Some(path) => PathBuf::from(path),
            None => utils::get_config_dir()?.join(CONFIG_FILE),
        };

        let mut config = match fs::read_to_string(&config_file) {
            Ok(config_str) if !CLAP_ARGS.is_present("no_config") => {
                Self::try_from(toml::from_str::<UserConfig>(&config_str)?)?
            }
            _ => Self::default(),
        };

        config.options.override_with_clap_args();

        if config.options.database.as_os_str().is_empty() {
            config.options.database = utils::get_default_database_file()?;
        }

        if config.options.instances.as_os_str().is_empty() {
            config.options.instances = utils::get_default_instances_file()?;
        }

        Ok(config)
    }
}

impl TryFrom<UserConfig> for Config {
    type Error = anyhow::Error;

    fn try_from(user_config: UserConfig) -> Result<Self, Self::Error> {
        let mut config = Self::default();

        if let Some(options) = user_config.options {
            config.options = options.into();
        }

        if let Some(theme) = user_config.theme {
            config.theme = theme.try_into()?;
        }

        if let Some(key_bindings) = user_config.key_bindings {
            config.key_bindings = key_bindings.try_into()?;
        }

        Ok(config)
    }
}
