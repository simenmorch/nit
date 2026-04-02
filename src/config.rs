use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use serde::Deserialize;
use serde::de::{self, Visitor};
use syntect::highlighting::{Theme, ThemeSet};

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
    pub colors: ColorsConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Name of a bundled syntect theme, e.g. "base16-ocean.dark"
    pub syntax: String,
    /// Path to a .tmTheme or .sublime-color-scheme file (overrides `syntax` if set)
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

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    #[serde(default, deserialize_with = "deserialize_optional_hex_color")]
    pub bg: Option<Color>,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub border_focused: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub border_unfocused: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub fg: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub fg_muted: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub fg_selected: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub fg_added: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub fg_removed: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub fg_accent: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub fg_info: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub bg_added: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub bg_removed: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub bg_search_match: Color,
    #[serde(deserialize_with = "deserialize_hex_color")]
    pub bg_search_current: Color,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            bg: None,
            border_focused: Color::Cyan,
            border_unfocused: Color::DarkGray,
            fg: Color::White,
            fg_muted: Color::DarkGray,
            fg_selected: Color::Cyan,
            fg_added: Color::Green,
            fg_removed: Color::Red,
            fg_accent: Color::Yellow,
            fg_info: Color::Magenta,
            bg_added: Color::Rgb(30, 60, 30),
            bg_removed: Color::Rgb(60, 30, 30),
            bg_search_match: Color::Rgb(120, 100, 30),
            bg_search_current: Color::Rgb(180, 140, 20),
        }
    }
}

fn parse_hex_color(s: &str) -> std::result::Result<Color, String> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 {
        return Err(format!("expected 6 hex digits, got '{}'", s));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
    Ok(Color::Rgb(r, g, b))
}

fn deserialize_optional_hex_color<'de, D>(deserializer: D) -> std::result::Result<Option<Color>, D::Error>
where
    D: de::Deserializer<'de>,
{
    let color = deserialize_hex_color(deserializer)?;
    Ok(Some(color))
}

fn deserialize_hex_color<'de, D>(deserializer: D) -> std::result::Result<Color, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct HexColorVisitor;

    impl<'de> Visitor<'de> for HexColorVisitor {
        type Value = Color;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a hex color string like \"#88C0D0\"")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Color, E> {
            parse_hex_color(v).map_err(E::custom)
        }
    }

    deserializer.deserialize_str(HexColorVisitor)
}

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

fn config_dir() -> Option<PathBuf> {
    // Prefer XDG_CONFIG_HOME, fall back to ~/.config (even on macOS where
    // `directories` would use ~/Library/Application Support).
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("nit"));
    }
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".config").join("nit"))
}

fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("nit.toml"))
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = directories::BaseDirs::new() {
            return home.home_dir().join(rest);
        }
    }
    PathBuf::from(path)
}

fn load_theme_file(path: &Path) -> Result<Theme> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("tmTheme") => ThemeSet::get_theme(path)
            .with_context(|| format!("failed to load theme from {}", path.display())),
        Some("sublime-color-scheme") => {
            let cs = sublime_color_scheme::parse_color_scheme_file(path)
                .map_err(|e| anyhow::anyhow!("{}", e))
                .with_context(|| format!("failed to parse color scheme {}", path.display()))?;
            let theme: Theme = cs
                .try_into()
                .map_err(|e| anyhow::anyhow!("{:?}", e))
                .with_context(|| format!("failed to convert color scheme {}", path.display()))?;
            Ok(theme)
        }
        _ => bail!("unsupported theme format: {}", path.display()),
    }
}

const THEME_EXTENSIONS: &[&str] = &["tmTheme", "sublime-color-scheme"];

pub fn resolve_theme(config: &ThemeConfig) -> Result<Theme> {
    // 1. Explicit file path takes priority
    if let Some(ref path) = config.syntax_file {
        let expanded = expand_tilde(path);
        return load_theme_file(&expanded);
    }

    // 2. Check bundled syntect themes
    let ts = ThemeSet::load_defaults();
    if let Some(theme) = ts.themes.get(&config.syntax) {
        return Ok(theme.clone());
    }

    // 3. Search themes directory for a matching theme file
    if let Some(themes_dir) = config_dir().map(|d| d.join("themes")) {
        if themes_dir.is_dir() {
            for entry in fs::read_dir(&themes_dir)
                .with_context(|| format!("failed to read themes dir {}", themes_dir.display()))?
            {
                let entry = entry?;
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !THEME_EXTENSIONS.contains(&ext) {
                    continue;
                }
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if stem.eq_ignore_ascii_case(&config.syntax) {
                    return load_theme_file(&path);
                }
            }
        }
    }

    bail!(
        "unknown theme '{}'. Place a .tmTheme or .sublime-color-scheme file in ~/.config/nit/themes/ \
         or use a built-in theme: {}",
        config.syntax,
        ts.themes.keys().cloned().collect::<Vec<_>>().join(", ")
    );
}
