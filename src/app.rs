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
    pub artists_loaded: bool,
    pub albums_loaded: bool,
    pub songs_loaded: bool,
    pub visible_rows: usize,
    // For g/G double-key detection
    pub pending_g: bool,
    pub show_help: bool,
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
            artists_loaded: false,
            albums_loaded: false,
            songs_loaded: false,
            visible_rows: 20,
            pending_g: false,
            show_help: false,
        }
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
        self.clamp_offset(self.visible_rows);
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

fn matches(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn fuzzy_filter_artists(artists: &[Artist], q: &str) -> Vec<LibraryItem> {
    artists
        .iter()
        .filter(|a| q.is_empty() || matches(&a.label, q))
        .cloned()
        .map(LibraryItem::Artist)
        .collect()
}

fn fuzzy_filter_albums(albums: &[Album], q: &str) -> Vec<LibraryItem> {
    albums
        .iter()
        .filter(|a| {
            if q.is_empty() {
                return true;
            }
            let combined = format!("{} {}", a.label, a.artist.join(" "));
            matches(&combined, q)
        })
        .cloned()
        .map(LibraryItem::Album)
        .collect()
}

fn fuzzy_filter_songs(songs: &[Song], q: &str) -> Vec<LibraryItem> {
    songs
        .iter()
        .filter(|s| {
            if q.is_empty() {
                return true;
            }
            let combined = format!("{} {} {}", s.label, s.artist.join(" "), s.album);
            matches(&combined, q)
        })
        .cloned()
        .map(LibraryItem::Song)
        .collect()
}
