use serde::Deserialize;
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub watch: WatchConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WatchConfig {
    pub interface: String,
    pub target_a: String,
    pub target_b: String,
}

pub fn load_config(path: impl AsRef<Path>) -> Result<Config> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}
