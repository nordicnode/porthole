//! Porthole's own config: small things worth remembering between runs,
//! like where the fleet was installed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Where the stack lives (docker-compose.yml, .env, configs).
    /// Saved by the Setup wizard on success.
    pub install_dir: Option<String>,
}

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".config/porthole/config.json")
}

pub fn load() -> Config {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &Config) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg).expect("config serializes");
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips() {
        let cfg = Config {
            install_dir: Some("/opt/fleet".to_string()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
        assert_eq!(Config::default().install_dir, None);
    }
}
