use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub channels: Vec<ChannelConfig>,
    pub sync_interval_minutes: u64,
    pub database_path: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChannelConfig {
    pub name: String,
    pub ics_url: String,
    pub enabled: bool,
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }
}