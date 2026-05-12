use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::Result;

const CONFIG_FILE: &str = "sandbox.toml";

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub default_headers: Vec<(String, String)>,
    pub response_dir: String,
    pub timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            default_headers: vec![],
            response_dir: "responses".to_string(),
            timeout_secs: 30,
        }
    }
}

impl Config {
    pub fn load(sandbox_root: &Path) -> Result<Self> {
        let path = sandbox_root.join(CONFIG_FILE);
        if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&raw)?)
        } else {
            let cfg = Config::default();
            cfg.save(sandbox_root)?;
            Ok(cfg)
        }
    }

    pub fn save(&self, sandbox_root: &Path) -> Result<()> {
        let path = sandbox_root.join(CONFIG_FILE);
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    pub fn response_dir_path(&self, sandbox_root: &Path) -> PathBuf {
        sandbox_root.join(&self.response_dir)
    }
}
