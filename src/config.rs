use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub type Config = HashMap<String, KodiSystem>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KodiSystem {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub default: bool,
}

impl KodiSystem {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("koditerm")
        .join("config.toml")
}

pub fn load() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        let cfg = default_config();
        save(&cfg)?;
        return Ok(cfg);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading config from {}", path.display()))?;
    let cfg: Config = toml::from_str(&content).context("parsing config")?;
    if cfg.is_empty() {
        return Err(anyhow!("config contains no systems"));
    }
    Ok(cfg)
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(cfg)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Returns the named system, the one marked `default = true`, or an error
/// with instructions if neither applies.
pub fn resolve<'a>(cfg: &'a Config, name: Option<&str>) -> Result<(&'a str, &'a KodiSystem)> {
    if let Some(n) = name {
        return cfg
            .get_key_value(n)
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| {
                let mut names: Vec<_> = cfg.keys().map(String::as_str).collect();
                names.sort();
                anyhow!("no system named '{}' (available: {})", n, names.join(", "))
            });
    }

    if let Some((k, v)) = cfg.iter().find(|(_, v)| v.default) {
        return Ok((k.as_str(), v));
    }

    let mut names: Vec<_> = cfg.keys().map(String::as_str).collect();
    names.sort();
    Err(anyhow!(
        "No default system configured.\n\n\
         Usage:\n\
         \x20  koditerm --system <name>    connect to a named system\n\n\
         Available systems: {}\n\n\
         Tip: add 'default = true' to one of your systems in\n\
         \x20  {}",
        names.join(", "),
        config_path().display()
    ))
}

fn default_config() -> Config {
    let mut cfg = Config::new();
    cfg.insert(
        "my-kodi".to_string(),
        KodiSystem {
            host: "192.168.1.1".to_string(),
            port: 80,
            username: "kodi".to_string(),
            password: "kodi".to_string(),
            default: true,
        },
    );
    cfg
}
