use anyhow::{anyhow, Context, Result};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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

/// Resolved colors ready for use in the UI.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Subdued text: relative line numbers, metadata, subtitles, file paths.
    pub dim: Color,
    /// Primary accent: titles, active scope tab, progress gauge (remote), playing item.
    pub accent: Color,
    /// Secondary highlight: artist names, local gauge, queue count title.
    pub highlight: Color,
    /// Primary body text: unselected item labels in the library list.
    pub text: Color,
    /// Foreground of the selected row (text on top of selected_bg).
    pub selected_fg: Color,
    /// Background of the selected row.
    pub selected_bg: Color,
    /// Background of the ART type badge.
    pub tag_artist: Color,
    /// Background of the ALB type badge.
    pub tag_album: Color,
    /// Background of the SNG type badge.
    pub tag_song: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            dim: Color::Gray,
            accent: Color::Cyan,
            highlight: Color::Yellow,
            text: Color::White,
            selected_fg: Color::Black,
            selected_bg: Color::White,
            tag_artist: Color::Magenta,
            tag_album: Color::Blue,
            tag_song: Color::Green,
        }
    }
}

/// TOML-serialisable theme config. All fields are optional strings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeConfig {
    /// Subdued/dim text color (default: "gray"). Accepts named colors or "#rrggbb".
    pub dim: Option<String>,
    /// Primary accent color (default: "cyan").
    pub accent: Option<String>,
    /// Secondary highlight color (default: "yellow").
    pub highlight: Option<String>,
    /// Primary body text color (default: "white").
    pub text: Option<String>,
    /// Text color of the selected row (default: "black").
    pub selected_fg: Option<String>,
    /// Background color of the selected row (default: "white").
    pub selected_bg: Option<String>,
    /// Background color of the ART type badge (default: "magenta").
    pub tag_artist: Option<String>,
    /// Background color of the ALB type badge (default: "blue").
    pub tag_album: Option<String>,
    /// Background color of the SNG type badge (default: "green").
    pub tag_song: Option<String>,
}

impl ThemeConfig {
    pub fn resolve(&self) -> Theme {
        Theme {
            dim: self.dim.as_deref().map(parse_color).unwrap_or(Color::Gray),
            accent: self.accent.as_deref().map(parse_color).unwrap_or(Color::Cyan),
            highlight: self.highlight.as_deref().map(parse_color).unwrap_or(Color::Yellow),
            text: self.text.as_deref().map(parse_color).unwrap_or(Color::White),
            selected_fg: self.selected_fg.as_deref().map(parse_color).unwrap_or(Color::Black),
            selected_bg: self.selected_bg.as_deref().map(parse_color).unwrap_or(Color::White),
            tag_artist: self.tag_artist.as_deref().map(parse_color).unwrap_or(Color::Magenta),
            tag_album: self.tag_album.as_deref().map(parse_color).unwrap_or(Color::Blue),
            tag_song: self.tag_song.as_deref().map(parse_color).unwrap_or(Color::Green),
        }
    }
}

fn parse_color(s: &str) -> Color {
    match s.to_lowercase().trim() {
        "black"                       => Color::Black,
        "red"                         => Color::Red,
        "green"                       => Color::Green,
        "yellow"                      => Color::Yellow,
        "blue"                        => Color::Blue,
        "magenta"                     => Color::Magenta,
        "cyan"                        => Color::Cyan,
        "gray" | "grey"               => Color::Gray,
        "darkgray" | "darkgrey"
        | "dark_gray" | "dark_grey"   => Color::DarkGray,
        "white"                       => Color::White,
        "lightred"   | "light_red"    => Color::LightRed,
        "lightgreen" | "light_green"  => Color::LightGreen,
        "lightyellow"| "light_yellow" => Color::LightYellow,
        "lightblue"  | "light_blue"   => Color::LightBlue,
        "lightmagenta"|"light_magenta"=> Color::LightMagenta,
        "lightcyan"  | "light_cyan"   => Color::LightCyan,
        hex => {
            let hex = hex.strip_prefix('#').unwrap_or(hex);
            if hex.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return Color::Rgb(r, g, b);
                }
            }
            Color::Reset
        }
    }
}

/// Full on-disk config: an optional [theme] table plus one table per Kodi system.
#[derive(Debug, Serialize, Deserialize)]
pub struct FullConfig {
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(flatten)]
    pub systems: HashMap<String, KodiSystem>,
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("koditerm")
        .join("config.toml")
}

pub fn load() -> Result<FullConfig> {
    let path = config_path();
    if !path.exists() {
        let cfg = default_config();
        save(&cfg)?;
        return Ok(cfg);
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading config from {}", path.display()))?;
    let cfg: FullConfig = toml::from_str(&content).context("parsing config")?;
    if cfg.systems.is_empty() {
        return Err(anyhow!("config contains no systems"));
    }
    Ok(cfg)
}

pub fn save(cfg: &FullConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(cfg)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Returns the named system, the one marked `default = true`, or an error.
pub fn resolve<'a>(cfg: &'a FullConfig, name: Option<&str>) -> Result<(&'a str, &'a KodiSystem)> {
    if let Some(n) = name {
        return cfg
            .systems
            .get_key_value(n)
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| {
                let mut names: Vec<_> = cfg.systems.keys().map(String::as_str).collect();
                names.sort();
                anyhow!("no system named '{}' (available: {})", n, names.join(", "))
            });
    }

    if let Some((k, v)) = cfg.systems.iter().find(|(_, v)| v.default) {
        return Ok((k.as_str(), v));
    }

    let mut names: Vec<_> = cfg.systems.keys().map(String::as_str).collect();
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

fn default_config() -> FullConfig {
    let mut systems = HashMap::new();
    systems.insert(
        "my-kodi".to_string(),
        KodiSystem {
            host: "192.168.1.1".to_string(),
            port: 80,
            username: "kodi".to_string(),
            password: "kodi".to_string(),
            default: true,
        },
    );
    // Default theme: Tokyo Night Storm
    let theme = ThemeConfig {
        dim:        Some("#565f89".to_string()), // muted blue-grey
        accent:     Some("#7aa2f7".to_string()), // blue
        highlight:  Some("#e0af68".to_string()), // gold
        text:       Some("#c0caf5".to_string()), // foreground
        selected_fg: Some("#1d202f".to_string()), // dark background
        selected_bg: Some("#7aa2f7".to_string()), // blue
        tag_artist: Some("#bb9af7".to_string()), // purple
        tag_album:  Some("#7aa2f7".to_string()), // blue
        tag_song:   Some("#9ece6a".to_string()), // green
    };
    FullConfig { theme, systems }
}
