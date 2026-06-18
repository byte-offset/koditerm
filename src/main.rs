mod app;
mod config;
mod kodi;
mod ui;

use anyhow::Result;
use app::{App, InputMode, SearchScope};
use config::Config;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use kodi::KodiClient;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

enum AppEvent {
    Key(KeyEvent),
    Tick,
    ArtistsLoaded(Vec<kodi::Artist>),
    AlbumsLoaded(Vec<kodi::Album>),
    SongsLoaded(Vec<kodi::Song>),
    StatusUpdate(kodi::PlayerStatus),
    Error(String),
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let kodi = KodiClient::new(config)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Arc::new(Mutex::new(App::new(kodi)));

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Clone kodi ref for background tasks via app
    let kodi_ref = {
        let app = app.lock().unwrap();
        Arc::clone(&app.kodi)
    };
    let kodi_ref2 = Arc::clone(&kodi_ref);

    let tx_lib = tx.clone();
    let tx_status = tx.clone();
    let tx_key = tx.clone();
    let tx_tick = tx.clone();

    // Load each library type independently so the UI can show results as they arrive
    let tx_artists = tx_lib.clone();
    let tx_albums = tx_lib.clone();
    let tx_songs = tx_lib.clone();
    let kodi_artists = Arc::clone(&kodi_ref);
    let kodi_albums = Arc::clone(&kodi_ref);
    let kodi_songs = Arc::clone(&kodi_ref);

    tokio::spawn(async move {
        match kodi_artists.get_artists().await {
            Ok(a) => { let _ = tx_artists.send(AppEvent::ArtistsLoaded(a)); }
            Err(_) => { let _ = tx_artists.send(AppEvent::Error("Failed to load artists".to_string())); }
        }
    });
    tokio::spawn(async move {
        match kodi_albums.get_albums().await {
            Ok(a) => { let _ = tx_albums.send(AppEvent::AlbumsLoaded(a)); }
            Err(_) => { let _ = tx_albums.send(AppEvent::Error("Failed to load albums".to_string())); }
        }
    });
    tokio::spawn(async move {
        match kodi_songs.get_songs().await {
            Ok(s) => { let _ = tx_songs.send(AppEvent::SongsLoaded(s)); }
            Err(_) => { let _ = tx_songs.send(AppEvent::Error("Failed to load songs".to_string())); }
        }
    });

    // Periodic status polling
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            if let Ok(s) = kodi_ref2.get_status().await {
                let _ = tx_status.send(AppEvent::StatusUpdate(s));
            }
        }
    });

    // Key event reader
    tokio::spawn(async move {
        loop {
            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    let _ = tx_key.send(AppEvent::Key(key));
                }
            }
        }
    });

    // Tick for redraw
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(200));
        loop {
            interval.tick().await;
            let _ = tx_tick.send(AppEvent::Tick);
        }
    });

    let mut quit = false;

    while !quit {
        // Draw
        {
            let mut app = app.lock().unwrap();
            terminal.draw(|f| ui::draw(f, &mut app))?;
        }

        // Handle events
        for _ in 0..20 {
            match rx.try_recv() {
                Ok(AppEvent::ArtistsLoaded(artists)) => {
                    let mut app = app.lock().unwrap();
                    app.set_artists(artists);
                }
                Ok(AppEvent::AlbumsLoaded(albums)) => {
                    let mut app = app.lock().unwrap();
                    app.set_albums(albums);
                }
                Ok(AppEvent::SongsLoaded(songs)) => {
                    let mut app = app.lock().unwrap();
                    app.set_songs(songs);
                }
                Ok(AppEvent::StatusUpdate(s)) => {
                    let mut app = app.lock().unwrap();
                    app.status = s;
                }
                Ok(AppEvent::Error(e)) => {
                    let mut app = app.lock().unwrap();
                    app.status_message = Some(e);
                    app.loading = false;
                }
                Ok(AppEvent::Tick) => {}
                Ok(AppEvent::Key(key)) => {
                    let action = {
                        let mut app = app.lock().unwrap();
                        handle_key(&mut app, key)
                    };
                    if let Some(a) = action {
                        if a == "quit" {
                            quit = true;
                            break;
                        }
                        // Spawn async action
                        let kodi = {
                            let app = app.lock().unwrap();
                            Arc::clone(&app.kodi)
                        };
                        let status = {
                            let app = app.lock().unwrap();
                            app.status.clone()
                        };
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = dispatch_action(&kodi, &a, &status).await {
                                let _ = tx.send(AppEvent::Error(format!("Error: {e}")));
                            }
                            // Refresh status after action
                            if let Ok(s) = kodi.get_status().await {
                                let _ = tx.send(AppEvent::StatusUpdate(s));
                            }
                        });
                    }
                }
                Err(_) => break,
            }
        }

        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) -> Option<String> {
    match &app.input_mode {
        InputMode::Search => handle_key_search(app, key),
        InputMode::Normal | InputMode::Command => handle_key_normal(app, key),
    }
}

fn handle_key_search(app: &mut App, key: KeyEvent) -> Option<String> {
    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.clear_search();
            None
        }
        KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
            play_selected(app)
        }
        KeyCode::F(5) => {
            // Queue without leaving search
            queue_selected(app)
        }
        KeyCode::Backspace => {
            app.pop_search_char();
            None
        }
        KeyCode::Char(c) => {
            app.push_search_char(c);
            None
        }
        KeyCode::Down => {
            app.move_down(app.visible_rows);
            None
        }
        KeyCode::Up => {
            app.move_up(app.visible_rows);
            None
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.half_page_down();
            None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.half_page_up();
            None
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.next_scope();
            None
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.prev_scope();
            None
        }
        KeyCode::PageDown => {
            app.page_down();
            None
        }
        KeyCode::PageUp => {
            app.page_up();
            None
        }
        KeyCode::F(1) => {
            app.set_scope(SearchScope::All);
            None
        }
        KeyCode::F(2) => {
            app.set_scope(SearchScope::Artists);
            None
        }
        KeyCode::F(3) => {
            app.set_scope(SearchScope::Albums);
            None
        }
        KeyCode::F(4) => {
            app.set_scope(SearchScope::Songs);
            None
        }
        _ => None,
    }
}

fn handle_key_normal(app: &mut App, key: KeyEvent) -> Option<String> {
    if key.code == KeyCode::Char('g') {
        if app.pending_g {
            app.pending_g = false;
            app.go_top();
            return None;
        } else {
            app.pending_g = true;
            return None;
        }
    }
    app.pending_g = false;

    match key.code {
        KeyCode::Char('q') => Some("quit".to_string()),
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Search;
            app.clear_search();
            None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.move_down(app.visible_rows);
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.move_up(app.visible_rows);
            None
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.half_page_down();
            None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.half_page_up();
            None
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.next_scope();
            None
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.prev_scope();
            None
        }
        KeyCode::PageDown => {
            app.page_down();
            None
        }
        KeyCode::PageUp => {
            app.page_up();
            None
        }
        KeyCode::Char('G') => {
            app.go_bottom();
            None
        }
        KeyCode::Enter => play_selected(app),
        KeyCode::Char('a') => queue_selected(app),
        KeyCode::Char(' ') => {
            if let Some(pid) = app.status.player_id {
                Some(format!("toggle_pause:{pid}"))
            } else {
                play_selected(app)
            }
        }
        KeyCode::Char('n') => app.status.player_id.map(|pid| format!("next:{pid}")),
        KeyCode::Char('p') => app.status.player_id.map(|pid| format!("prev:{pid}")),
        KeyCode::Char('s') => app.status.player_id.map(|pid| format!("stop:{pid}")),
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let vol = (app.status.volume + 5).min(100);
            Some(format!("volume:{vol}"))
        }
        KeyCode::Char('-') => {
            let vol = app.status.volume.saturating_sub(5);
            Some(format!("volume:{vol}"))
        }
        KeyCode::F(1) => {
            app.set_scope(SearchScope::All);
            None
        }
        KeyCode::F(2) => {
            app.set_scope(SearchScope::Artists);
            None
        }
        KeyCode::F(3) => {
            app.set_scope(SearchScope::Albums);
            None
        }
        KeyCode::F(4) => {
            app.set_scope(SearchScope::Songs);
            None
        }
        _ => None,
    }
}

fn play_selected(app: &App) -> Option<String> {
    match app.selected_item()? {
        app::LibraryItem::Artist(a) => Some(format!("play_artist:{}", a.artistid)),
        app::LibraryItem::Album(a) => Some(format!("play_album:{}", a.albumid)),
        app::LibraryItem::Song(s) => Some(format!("play_song:{}", s.songid)),
    }
}

fn queue_selected(app: &App) -> Option<String> {
    match app.selected_item()? {
        app::LibraryItem::Artist(a) => Some(format!("queue_artist:{}", a.artistid)),
        app::LibraryItem::Album(a) => Some(format!("queue_album:{}", a.albumid)),
        app::LibraryItem::Song(s) => Some(format!("queue_song:{}", s.songid)),
    }
}

async fn dispatch_action(
    kodi: &KodiClient,
    action: &str,
    _status: &kodi::PlayerStatus,
) -> Result<()> {
    let parts: Vec<&str> = action.splitn(2, ':').collect();
    match parts[0] {
        "play_song" => kodi.play_song(parts[1].parse()?).await?,
        "play_album" => kodi.play_album(parts[1].parse()?).await?,
        "play_artist" => kodi.play_artist(parts[1].parse()?).await?,
        "queue_song" => kodi.queue_song(parts[1].parse()?).await?,
        "queue_album" => kodi.queue_album(parts[1].parse()?).await?,
        "queue_artist" => kodi.queue_artist(parts[1].parse()?).await?,
        "toggle_pause" => kodi.toggle_pause(parts[1].parse()?).await?,
        "stop" => kodi.stop(parts[1].parse()?).await?,
        "next" => kodi.next_track(parts[1].parse()?).await?,
        "prev" => kodi.prev_track(parts[1].parse()?).await?,
        "volume" => kodi.set_volume(parts[1].parse()?).await?,
        _ => {}
    }
    Ok(())
}
