use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Name of a bundled syntect theme, e.g. "base16-ocean.dark"
    pub syntax: String,
    /// Path to a custom .tmTheme file (overrides `syntax` if set)
    pub syntax_file: Option<String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            syntax: "base16-ocean.dark".to_string(),
            syntax_file: None,
        }
    }
}

/// Load config from ~/.config/nit/config.toml (or platform equivalent).
/// Returns default config if the file doesn't exist.
pub fn load() -> Result<Config> {
    let path = config_path();

    match path {
        Some(path) if path.exists() => {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("failed to read config at {}", path.display()))?;
            let config: Config = toml::from_str(&contents)
                .with_context(|| format!("failed to parse config at {}", path.display()))?;
            Ok(config)
        }
        _ => Ok(Config::default()),
    }
}

fn config_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "nit").map(|dirs| dirs.config_dir().join("nit.toml"))
}
