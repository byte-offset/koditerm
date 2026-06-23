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
    /// Background of the ART type badge, or None for no styling.
    pub tag_artist: Option<Color>,
    /// Background of the ALB type badge, or None for no styling.
    pub tag_album: Option<Color>,
    /// Background of the SNG type badge, or None for no styling.
    pub tag_song: Option<Color>,
    /// Label text inside the artist badge.
    pub tag_artist_label: String,
    /// Label text inside the album badge.
    pub tag_album_label: String,
    /// Label text inside the song badge.
    pub tag_song_label: String,
}

impl Default for Theme {
    fn default() -> Self {
        // Tokyo Night Storm
        Theme {
            dim:         Color::Rgb(0x56, 0x5f, 0x89),
            accent:      Color::Rgb(0x7a, 0xa2, 0xf7),
            highlight:   Color::Rgb(0xe0, 0xaf, 0x68),
            text:        Color::Rgb(0xc0, 0xca, 0xf5),
            selected_fg: Color::Rgb(0x1d, 0x20, 0x2f),
            selected_bg: Color::Rgb(0x7a, 0xa2, 0xf7),
            tag_artist:  None,
            tag_album:   None,
            tag_song:    None,
            tag_artist_label: "🎤".to_string(),
            tag_album_label:  "💿".to_string(),
            tag_song_label:   "🎵".to_string(),
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
    /// Label text for the artist badge (default: "ART"). Unicode ok, e.g. "♪".
    pub tag_artist_label: Option<String>,
    /// Label text for the album badge (default: "ALB").
    pub tag_album_label: Option<String>,
    /// Label text for the song badge (default: "SNG").
    pub tag_song_label: Option<String>,
}

impl ThemeConfig {
    pub fn resolve(&self) -> Theme {
        let d = Theme::default();
        Theme {
            dim:         self.dim.as_deref().map(parse_color).unwrap_or(d.dim),
            accent:      self.accent.as_deref().map(parse_color).unwrap_or(d.accent),
            highlight:   self.highlight.as_deref().map(parse_color).unwrap_or(d.highlight),
            text:        self.text.as_deref().map(parse_color).unwrap_or(d.text),
            selected_fg: self.selected_fg.as_deref().map(parse_color).unwrap_or(d.selected_fg),
            selected_bg: self.selected_bg.as_deref().map(parse_color).unwrap_or(d.selected_bg),
            tag_artist:  self.tag_artist.as_deref().map(parse_color_opt).unwrap_or(d.tag_artist),
            tag_album:   self.tag_album.as_deref().map(parse_color_opt).unwrap_or(d.tag_album),
            tag_song:    self.tag_song.as_deref().map(parse_color_opt).unwrap_or(d.tag_song),
            tag_artist_label: self.tag_artist_label.clone().unwrap_or(d.tag_artist_label),
            tag_album_label:  self.tag_album_label.clone().unwrap_or(d.tag_album_label),
            tag_song_label:   self.tag_song_label.clone().unwrap_or(d.tag_song_label),
        }
    }
}

fn parse_color_opt(s: &str) -> Option<Color> {
    match s.to_lowercase().trim() {
        "none" | "transparent" => None,
        _ => Some(parse_color(s)),
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
        dim:             Some("#565f89".to_string()),
        accent:          Some("#7aa2f7".to_string()),
        highlight:       Some("#e0af68".to_string()),
        text:            Some("#c0caf5".to_string()),
        selected_fg:     Some("#1d202f".to_string()),
        selected_bg:     Some("#7aa2f7".to_string()),
        tag_artist:      Some("none".to_string()),
        tag_album:       Some("none".to_string()),
        tag_song:        Some("none".to_string()),
        tag_artist_label: Some("🎤".to_string()),
        tag_album_label:  Some("💿".to_string()),
        tag_song_label:   Some("🎵".to_string()),
    };
    FullConfig { theme, systems }
}
