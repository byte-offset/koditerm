use crate::config::Theme;
use crate::kodi::{Album, Artist, KodiClient, PlayerStatus, Song};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackBackend {
    Remote,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    Track,
    Queue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchScope {
    All,
    Artists,
    Albums,
    Songs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchMode {
    Exact,
    Fuzzy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
    Command,
}

#[derive(Debug, Clone)]
pub enum LibraryItem {
    Artist(Artist),
    Album(Album),
    Song(Song),
}

impl LibraryItem {
    pub fn display_label(&self) -> &str {
        match self {
            LibraryItem::Artist(a) => &a.label,
            LibraryItem::Album(a) => &a.label,
            LibraryItem::Song(s) => &s.label,
        }
    }

    pub fn type_label(&self) -> &str {
        match self {
            LibraryItem::Artist(_) => "Artist",
            LibraryItem::Album(_) => "Album",
            LibraryItem::Song(_) => "Song",
        }
    }

    pub fn subtitle(&self) -> String {
        match self {
            LibraryItem::Artist(_) => String::new(),
            LibraryItem::Album(a) => {
                let artist = a.artist.join(", ");
                if let Some(y) = a.year {
                    if y > 0 {
                        format!("{artist} ({y})")
                    } else {
                        artist
                    }
                } else {
                    artist
                }
            }
            LibraryItem::Song(s) => {
                let artist = s.artist.join(", ");
                format!("{artist} — {}", s.album)
            }
        }
    }
}

pub struct App {
    pub kodi: Arc<KodiClient>,
    pub status: PlayerStatus,
    pub input_mode: InputMode,
    pub search_scope: SearchScope,
    pub search_mode: SearchMode,
    pub search_query: String,
    pub all_artists: Vec<Artist>,
    pub all_albums: Vec<Album>,
    pub all_songs: Vec<Song>,
    pub filtered_items: Vec<LibraryItem>,
    pub selected: usize,
    pub list_offset: usize,
    pub status_message: Option<String>,
    pub loading: bool,
    pub library_loaded: bool,
    pub artists_loaded: bool,
    pub albums_loaded: bool,
    pub songs_loaded: bool,
    pub visible_rows: usize,
    pub pending_g: bool,
    pub show_help: bool,
    pub backend: PlaybackBackend,
    pub remote_queue: Vec<Song>,
    pub local_queue: Vec<Song>,
    pub local_queue_pos: usize,
    pub local_current_song: Option<Song>,
    pub local_paused: bool,
    pub local_fetching: bool,
    pub local_volume: u32,
    pub local_position: u32,
    pub pending_count: String,
    pub repeat_mode: RepeatMode,
    pub show_track_info: bool,
    pub theme: Theme,
}

impl App {
    pub fn new(kodi: KodiClient, theme: Theme) -> Self {
        App {
            kodi: Arc::new(kodi),
            status: PlayerStatus::default(),
            input_mode: InputMode::Normal,
            search_scope: SearchScope::All,
            search_mode: SearchMode::Exact,
            search_query: String::new(),
            all_artists: Vec::new(),
            all_albums: Vec::new(),
            all_songs: Vec::new(),
            filtered_items: Vec::new(),
            selected: 0,
            list_offset: 0,
            status_message: Some("Loading library…".to_string()),
            loading: true,
            library_loaded: false,
            artists_loaded: false,
            albums_loaded: false,
            songs_loaded: false,
            visible_rows: 20,
            pending_g: false,
            show_help: false,
            backend: PlaybackBackend::Remote,
            remote_queue: Vec::new(),
            local_queue: Vec::new(),
            local_queue_pos: 0,
            local_current_song: None,
            local_paused: false,
            local_fetching: false,
            local_volume: 100,
            local_position: 0,
            pending_count: String::new(),
            repeat_mode: RepeatMode::Off,
            show_track_info: false,
            theme,
        }
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat_mode = match self.repeat_mode {
            RepeatMode::Off => RepeatMode::Track,
            RepeatMode::Track => RepeatMode::Queue,
            RepeatMode::Queue => RepeatMode::Off,
        };
    }

    pub fn toggle_backend(&mut self) {
        self.backend = match self.backend {
            PlaybackBackend::Remote => PlaybackBackend::Local,
            PlaybackBackend::Local => PlaybackBackend::Remote,
        };
        self.selected = 0;
        self.list_offset = 0;
        self.apply_filter();
    }

    pub fn clear_local_queue(&mut self) {
        self.local_queue.clear();
        self.local_queue_pos = 0;
        self.local_current_song = None;
        self.local_fetching = false;
        self.local_position = 0;
        self.apply_filter();
    }

    pub fn push_local_queue(&mut self, song: Song) {
        self.local_queue.push(song);
        self.apply_filter();
    }

    pub fn set_artists(&mut self, artists: Vec<Artist>) {
        self.all_artists = artists;
        self.artists_loaded = true;
        self.update_loading_state();
        self.apply_filter();
    }

    pub fn set_albums(&mut self, albums: Vec<Album>) {
        self.all_albums = albums;
        self.albums_loaded = true;
        self.update_loading_state();
        self.apply_filter();
    }

    pub fn set_songs(&mut self, songs: Vec<Song>) {
        self.all_songs = songs;
        self.songs_loaded = true;
        self.update_loading_state();
        self.apply_filter();
    }

    fn update_loading_state(&mut self) {
        let pending: Vec<&str> = [
            (!self.artists_loaded).then_some("artists"),
            (!self.albums_loaded).then_some("albums"),
            (!self.songs_loaded).then_some("songs"),
        ]
        .into_iter()
        .flatten()
        .collect();

        if pending.is_empty() {
            self.loading = false;
            self.library_loaded = true;
            self.status_message = None;
        } else {
            self.status_message = Some(format!("Loading {}…", pending.join(", ")));
        }
    }

    pub fn toggle_search_mode(&mut self) {
        self.search_mode = match self.search_mode {
            SearchMode::Exact => SearchMode::Fuzzy,
            SearchMode::Fuzzy => SearchMode::Exact,
        };
        self.selected = 0;
        self.list_offset = 0;
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        let mode = &self.search_mode;
        self.filtered_items = match self.search_scope {
            SearchScope::Artists => filter_artists(&self.all_artists, &q, mode),
            SearchScope::Albums => filter_albums(&self.all_albums, &q, mode),
            SearchScope::Songs => filter_songs(&self.all_songs, &q, mode),
            SearchScope::All => {
                let mut items = Vec::new();
                items.extend(filter_artists(&self.all_artists, &q, mode));
                items.extend(filter_albums(&self.all_albums, &q, mode));
                items.extend(filter_songs(&self.all_songs, &q, mode));
                items
            }
        };
        if self.selected >= self.filtered_items.len() {
            self.selected = self.filtered_items.len().saturating_sub(1);
        }
        self.clamp_offset(self.visible_rows);
    }

    pub fn move_down(&mut self, visible_rows: usize) {
        self.move_down_by(1, visible_rows);
    }

    pub fn move_up(&mut self, visible_rows: usize) {
        self.move_up_by(1, visible_rows);
    }

    pub fn move_down_by(&mut self, n: usize, visible_rows: usize) {
        if self.filtered_items.is_empty() {
            return;
        }
        self.selected = (self.selected + n).min(self.filtered_items.len() - 1);
        self.clamp_offset(visible_rows);
    }

    pub fn move_up_by(&mut self, n: usize, visible_rows: usize) {
        self.selected = self.selected.saturating_sub(n);
        self.clamp_offset(visible_rows);
    }

    pub fn half_page_down(&mut self) {
        if self.filtered_items.is_empty() {
            return;
        }
        let half = (self.visible_rows / 2).max(1);
        self.selected = (self.selected + half).min(self.filtered_items.len() - 1);
        self.clamp_offset(self.visible_rows);
    }

    pub fn half_page_up(&mut self) {
        let half = (self.visible_rows / 2).max(1);
        self.selected = self.selected.saturating_sub(half);
        self.clamp_offset(self.visible_rows);
    }

    pub fn page_down(&mut self) {
        if self.filtered_items.is_empty() {
            return;
        }
        self.selected = (self.selected + self.visible_rows).min(self.filtered_items.len() - 1);
        self.clamp_offset(self.visible_rows);
    }

    pub fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(self.visible_rows);
        self.clamp_offset(self.visible_rows);
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
        self.list_offset = 0;
    }

    pub fn go_bottom(&mut self) {
        self.selected = self.filtered_items.len().saturating_sub(1);
        self.clamp_offset(self.visible_rows);
    }

    fn clamp_offset(&mut self, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }
        if self.selected < self.list_offset {
            self.list_offset = self.selected;
        } else if self.selected >= self.list_offset + visible_rows {
            self.list_offset = self.selected + 1 - visible_rows;
        }
    }

    pub fn selected_item(&self) -> Option<&LibraryItem> {
        self.filtered_items.get(self.selected)
    }

    pub fn set_scope(&mut self, scope: SearchScope) {
        self.search_scope = scope;
        self.selected = 0;
        self.list_offset = 0;
        self.apply_filter();
    }

    pub fn next_scope(&mut self) {
        self.set_scope(match self.search_scope {
            SearchScope::All => SearchScope::Artists,
            SearchScope::Artists => SearchScope::Albums,
            SearchScope::Albums => SearchScope::Songs,
            SearchScope::Songs => SearchScope::All,
        });
    }

    pub fn prev_scope(&mut self) {
        self.set_scope(match self.search_scope {
            SearchScope::All => SearchScope::Songs,
            SearchScope::Artists => SearchScope::All,
            SearchScope::Albums => SearchScope::Artists,
            SearchScope::Songs => SearchScope::Albums,
        });
    }

    pub fn push_search_char(&mut self, c: char) {
        self.search_query.push(c);
        self.selected = 0;
        self.list_offset = 0;
        self.apply_filter();
    }

    pub fn pop_search_char(&mut self) {
        self.search_query.pop();
        self.selected = 0;
        self.list_offset = 0;
        self.apply_filter();
    }

    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.selected = 0;
        self.list_offset = 0;
        self.apply_filter();
    }
}

fn exact_match(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

// Returns a score (lower = better) if all characters of needle appear as a
// subsequence in haystack, or None if they don't.
fn fuzzy_score(haystack: &str, needle: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(0);
    }
    let h = haystack.to_lowercase();
    let n = needle.to_lowercase();
    let mut hi = h.chars().peekable();
    let mut score: i64 = 0;
    let mut last_match = 0usize;
    for nc in n.chars() {
        let mut found = false;
        let mut pos = last_match;
        while let Some(hc) = hi.next() {
            pos += 1;
            if hc == nc {
                score -= pos as i64 - last_match as i64;
                last_match = pos;
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    Some(score)
}

fn filter_artists(artists: &[Artist], q: &str, mode: &SearchMode) -> Vec<LibraryItem> {
    if q.is_empty() {
        return artists.iter().cloned().map(LibraryItem::Artist).collect();
    }
    match mode {
        SearchMode::Exact => artists
            .iter()
            .filter(|a| exact_match(&a.label, q))
            .cloned()
            .map(LibraryItem::Artist)
            .collect(),
        SearchMode::Fuzzy => {
            let mut scored: Vec<_> = artists
                .iter()
                .filter_map(|a| fuzzy_score(&a.label, q).map(|s| (s, a)))
                .collect();
            scored.sort_by_key(|(s, _)| *s);
            scored.into_iter().map(|(_, a)| LibraryItem::Artist(a.clone())).collect()
        }
    }
}

fn filter_albums(albums: &[Album], q: &str, mode: &SearchMode) -> Vec<LibraryItem> {
    if q.is_empty() {
        return albums.iter().cloned().map(LibraryItem::Album).collect();
    }
    match mode {
        SearchMode::Exact => albums
            .iter()
            .filter(|a| exact_match(&format!("{} {}", a.label, a.artist.join(" ")), q))
            .cloned()
            .map(LibraryItem::Album)
            .collect(),
        SearchMode::Fuzzy => {
            let mut scored: Vec<_> = albums
                .iter()
                .filter_map(|a| {
                    let combined = format!("{} {}", a.label, a.artist.join(" "));
                    fuzzy_score(&combined, q).map(|s| (s, a))
                })
                .collect();
            scored.sort_by_key(|(s, _)| *s);
            scored.into_iter().map(|(_, a)| LibraryItem::Album(a.clone())).collect()
        }
    }
}

fn filter_songs(songs: &[Song], q: &str, mode: &SearchMode) -> Vec<LibraryItem> {
    if q.is_empty() {
        return songs.iter().cloned().map(LibraryItem::Song).collect();
    }
    match mode {
        SearchMode::Exact => songs
            .iter()
            .filter(|s| exact_match(&format!("{} {} {}", s.label, s.artist.join(" "), s.album), q))
            .cloned()
            .map(LibraryItem::Song)
            .collect(),
        SearchMode::Fuzzy => {
            let mut scored: Vec<_> = songs
                .iter()
                .filter_map(|s| {
                    let combined = format!("{} {} {}", s.label, s.artist.join(" "), s.album);
                    fuzzy_score(&combined, q).map(|sc| (sc, s))
                })
                .collect();
            scored.sort_by_key(|(s, _)| *s);
            scored.into_iter().map(|(_, s)| LibraryItem::Song(s.clone())).collect()
        }
    }
}
