use crate::kodi::{Album, Artist, KodiClient, PlayerStatus, Song};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchScope {
    All,
    Artists,
    Albums,
    Songs,
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
    // For g/G double-key detection
    pub pending_g: bool,
}

impl App {
    pub fn new(kodi: KodiClient) -> Self {
        App {
            kodi: Arc::new(kodi),
            status: PlayerStatus::default(),
            input_mode: InputMode::Normal,
            search_scope: SearchScope::All,
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
            pending_g: false,
        }
    }

    pub fn set_library(&mut self, artists: Vec<Artist>, albums: Vec<Album>, songs: Vec<Song>) {
        self.all_artists = artists;
        self.all_albums = albums;
        self.all_songs = songs;
        self.library_loaded = true;
        self.loading = false;
        self.status_message = None;
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        let q = self.search_query.to_lowercase();
        self.filtered_items = match self.search_scope {
            SearchScope::Artists => fuzzy_filter_artists(&self.all_artists, &q),
            SearchScope::Albums => fuzzy_filter_albums(&self.all_albums, &q),
            SearchScope::Songs => fuzzy_filter_songs(&self.all_songs, &q),
            SearchScope::All => {
                let mut items = Vec::new();
                items.extend(fuzzy_filter_artists(&self.all_artists, &q));
                items.extend(fuzzy_filter_albums(&self.all_albums, &q));
                items.extend(fuzzy_filter_songs(&self.all_songs, &q));
                items
            }
        };
        if self.selected >= self.filtered_items.len() {
            self.selected = self.filtered_items.len().saturating_sub(1);
        }
        self.clamp_offset(20);
    }

    pub fn move_down(&mut self, visible_rows: usize) {
        if self.filtered_items.is_empty() {
            return;
        }
        if self.selected + 1 < self.filtered_items.len() {
            self.selected += 1;
        }
        self.clamp_offset(visible_rows);
    }

    pub fn move_up(&mut self, visible_rows: usize) {
        if self.selected > 0 {
            self.selected -= 1;
        }
        self.clamp_offset(visible_rows);
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
        self.list_offset = 0;
    }

    pub fn go_bottom(&mut self) {
        self.selected = self.filtered_items.len().saturating_sub(1);
        self.clamp_offset(20);
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

fn fuzzy_score(haystack: &str, needle: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(0);
    }
    let h = haystack.to_lowercase();
    let n = needle.to_lowercase();
    // Simple subsequence-based fuzzy match
    let mut hi = h.chars().peekable();
    let mut score: i64 = 0;
    let mut last_match = 0usize;
    for nc in n.chars() {
        let mut found = false;
        let mut pos = last_match;
        while let Some(hc) = hi.next() {
            pos += 1;
            if hc == nc {
                score -= pos as i64 - last_match as i64; // penalize gaps
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

fn fuzzy_filter_artists(artists: &[Artist], q: &str) -> Vec<LibraryItem> {
    if q.is_empty() {
        return artists.iter().cloned().map(LibraryItem::Artist).collect();
    }
    let mut scored: Vec<_> = artists
        .iter()
        .filter_map(|a| {
            fuzzy_score(&a.label, q).map(|s| (s, LibraryItem::Artist(a.clone())))
        })
        .collect();
    scored.sort_by_key(|(s, _)| *s);
    scored.into_iter().map(|(_, item)| item).collect()
}

fn fuzzy_filter_albums(albums: &[Album], q: &str) -> Vec<LibraryItem> {
    if q.is_empty() {
        return albums.iter().cloned().map(LibraryItem::Album).collect();
    }
    let mut scored: Vec<_> = albums
        .iter()
        .filter_map(|a| {
            let artist_str = a.artist.join(" ");
            let combined = format!("{} {}", a.label, artist_str);
            fuzzy_score(&combined, q).map(|s| (s, LibraryItem::Album(a.clone())))
        })
        .collect();
    scored.sort_by_key(|(s, _)| *s);
    scored.into_iter().map(|(_, item)| item).collect()
}

fn fuzzy_filter_songs(songs: &[Song], q: &str) -> Vec<LibraryItem> {
    if q.is_empty() {
        return songs.iter().cloned().map(LibraryItem::Song).collect();
    }
    let mut scored: Vec<_> = songs
        .iter()
        .filter_map(|s| {
            let artist_str = s.artist.join(" ");
            let combined = format!("{} {} {}", s.label, artist_str, s.album);
            fuzzy_score(&combined, q).map(|sc| (sc, LibraryItem::Song(s.clone())))
        })
        .collect();
    scored.sort_by_key(|(s, _)| *s);
    scored.into_iter().map(|(_, item)| item).collect()
}
