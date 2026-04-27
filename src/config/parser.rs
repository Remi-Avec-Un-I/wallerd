use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

fn default_mode() -> String {
    "wallpaper".to_string()
}
fn default_displays() -> Vec<String> {
    vec!["*".to_string()]
}
fn default_scaling() -> String {
    "fill".to_string()
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Config {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_displays")]
    pub displays: Vec<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub path: PathBuf,
    pub view: String,
    #[serde(default = "default_scaling")]
    pub scaling: String,
    pub transition_shader: Option<String>,
    pub transition_duration: Option<i32>,
    pub constant_shader: Option<String>,
    pub interval: Option<u64>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ConfigFile {
    pub default: Config,
    #[serde(flatten)]
    pub additional: HashMap<String, Config>,
}

pub fn parse(path: &Path) -> Result<ConfigFile, ()> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        log::error!("Failed to read config file '{:?}': {}", path, e);
    })?;
    toml::from_str(&content).map_err(|e| {
        log::error!("Invalid config file '{:?}': {}", path, e);
    })
}

pub fn load_config_file(file: Option<&Path>) -> Result<ConfigFile, ()> {
    let path = get_path(file).map_err(|_| {
        log::error!("No config file found. Create one at ~/.config/wallerd/config.toml");
    })?;
    parse(&path)
}

pub fn get_path(file: Option<&Path>) -> Result<PathBuf, ()> {
    match file {
        Some(p) => {
            if !p.exists() {
                log::warn!(
                    "The specified config file '{:?}' doesn't exist, fallback to the default one.",
                    p
                );
                default_config_path()
            } else {
                Ok(p.to_path_buf())
            }
        }
        None => default_config_path(),
    }
}

fn default_config_path() -> Result<PathBuf, ()> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".config"))
                .unwrap_or_else(|| PathBuf::from("~/.config"))
        });
    let path = base.join("wallerd").join("config.toml");
    if path.exists() {
        Ok(path)
    } else {
        log::warn!("No config file found at '{:?}'", path);
        Err(())
    }
}
