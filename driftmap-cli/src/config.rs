use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub watch: WatchConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WatchConfig {
    #[serde(default = "default_mode")]
    #[allow(dead_code)]
    pub mode: String,
    pub interface: String,
    pub target_a: String,
    pub target_b: String,
    #[serde(default)]
    pub ignore_fields: Vec<String>,
}

fn default_mode() -> String {
    "capture".into()
}

pub fn load_config(path: impl AsRef<Path>) -> Result<Config> {
    let p = path.as_ref();
    let content = std::fs::read_to_string(p)
        .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", p.display(), e))?;
    let config: Config = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Invalid configuration in {}: {}", p.display(), e))?;
    Ok(config)
}
