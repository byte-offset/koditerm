mod app;
mod config;
mod kodi;
mod player;
mod ui;

use anyhow::Result;
use app::{App, InputMode, PlaybackBackend, RepeatMode, SearchScope};
use clap::Parser;
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

#[derive(Parser)]
#[command(about = "Terminal UI for Kodi")]
struct Args {
    /// Name of the Kodi system to connect to
    #[arg(short, long)]
    system: Option<String>,

    /// Audio output device name (partial match, e.g. "pipewire" or "hdmi")
    #[arg(long)]
    device: Option<String>,

    /// List available audio output devices and exit
    #[arg(long)]
    list_devices: bool,
}

enum AppEvent {
    Key(KeyEvent),
    Tick,
    ArtistsLoaded(Vec<kodi::Artist>),
    AlbumsLoaded(Vec<kodi::Album>),
    SongsLoaded(Vec<kodi::Song>),
    StatusUpdate(kodi::PlayerStatus),
    RemoteQueueUpdate(Vec<kodi::Song>),
    LocalSongListReady { songs: Vec<kodi::Song>, clear: bool },
    LocalBytesReady { bytes: Vec<u8>, song: kodi::Song, clear: bool },
    Error(String),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.list_devices {
        let devices = player::list_devices();
        if devices.is_empty() {
            println!("No audio output devices found.");
        } else {
            for d in &devices {
                println!("{d}");
            }
        }
        return Ok(());
    }

    let device_name = args.device.clone();
    let cfg = config::load()?;
    let theme = cfg.theme.resolve();
    let (name, system) = config::resolve(&cfg, args.system.as_deref())?;
    let kodi = KodiClient::new(name, system.clone())?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Arc::new(Mutex::new(App::new(kodi, theme)));

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

    // Periodic status + playlist polling
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            let (status, queue) =
                tokio::join!(kodi_ref2.get_status(), kodi_ref2.get_playlist());
            if let Ok(s) = status {
                let _ = tx_status.send(AppEvent::StatusUpdate(s));
            }
            if let Ok(q) = queue {
                let _ = tx_status.send(AppEvent::RemoteQueueUpdate(q));
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
    let mut local_player: Option<player::LocalPlayer> = None;

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
                Ok(AppEvent::RemoteQueueUpdate(q)) => {
                    let mut app = app.lock().unwrap();
                    app.remote_queue = q;
                }
                Ok(AppEvent::Error(e)) => {
                    let mut app = app.lock().unwrap();
                    app.status_message = Some(e);
                    app.loading = false;
                }
                Ok(AppEvent::Tick) => {
                    if let Some(ref lp) = local_player {
                        let next = {
                            let mut app = app.lock().unwrap();
                            if app.backend == PlaybackBackend::Local {
                                app.local_paused = lp.is_paused();
                                app.local_position = lp.position();
                                if lp.empty() && !app.local_fetching && app.local_current_song.is_some() {
                                    match app.repeat_mode {
                                        RepeatMode::Track => {
                                            let s = app.local_queue[app.local_queue_pos].clone();
                                            app.local_fetching = true;
                                            Some(s)
                                        }
                                        _ => {
                                            let next_pos = app.local_queue_pos + 1;
                                            if next_pos < app.local_queue.len() {
                                                app.local_queue_pos = next_pos;
                                                let s = app.local_queue[next_pos].clone();
                                                app.local_current_song = Some(s.clone());
                                                app.local_fetching = true;
                                                Some(s)
                                            } else if app.repeat_mode == RepeatMode::Queue && !app.local_queue.is_empty() {
                                                app.local_queue_pos = 0;
                                                let s = app.local_queue[0].clone();
                                                app.local_current_song = Some(s.clone());
                                                app.local_fetching = true;
                                                Some(s)
                                            } else {
                                                app.local_current_song = None;
                                                None
                                            }
                                        }
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        };
                        if let Some(song) = next {
                            let kodi = Arc::clone(&kodi_ref);
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                match fetch_local_bytes(&kodi, song.songid).await {
                                    Ok(bytes) => { let _ = tx.send(AppEvent::LocalBytesReady { bytes, song, clear: true }); }
                                    Err(e) => { let _ = tx.send(AppEvent::Error(format!("Auto-advance: {e}"))); }
                                }
                            });
                        }
                    }
                }
                Ok(AppEvent::LocalSongListReady { songs, clear }) => {
                    if songs.is_empty() {
                        app.lock().unwrap().status_message = Some("No songs found".to_string());
                    } else {
                        let fetch = {
                            let mut app = app.lock().unwrap();
                            let was_idle = app.local_current_song.is_none();

                            if clear {
                                app.local_queue.clear();
                                app.local_queue_pos = 0;
                                app.local_current_song = None;
                                app.local_fetching = false;
                                app.local_position = 0;
                            }

                            let start_pos = app.local_queue.len();
                            app.local_queue.extend(songs.iter().cloned());
                            app.apply_filter();

                            if clear || was_idle {
                                let pos = if clear { 0 } else { start_pos };
                                app.local_queue_pos = pos;
                                app.local_fetching = true;
                                Some((songs[0].clone(), clear))
                            } else {
                                app.status_message =
                                    Some(format!("{} songs added to queue", songs.len()));
                                None
                            }
                        };
                        if let Some((song, use_clear)) = fetch {
                            let kodi = Arc::clone(&kodi_ref);
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                match fetch_local_bytes(&kodi, song.songid).await {
                                    Ok(bytes) => { let _ = tx.send(AppEvent::LocalBytesReady { bytes, song, clear: use_clear }); }
                                    Err(e) => { let _ = tx.send(AppEvent::Error(format!("Local fetch: {e}"))); }
                                }
                            });
                        }
                    }
                }
                Ok(AppEvent::LocalBytesReady { bytes, song, clear }) => {
                    if let Some(ref mut lp) = local_player {
                        let kb = bytes.len() / 1024;
                        let result = if clear { lp.play_bytes(bytes) } else { lp.queue_bytes(bytes) };
                        let mut app = app.lock().unwrap();
                        app.local_fetching = false;
                        match result {
                            Ok(()) => {
                                if clear {
                                    app.local_current_song = Some(song.clone());
                                    app.local_paused = false;
                                    app.local_position = 0;
                                    app.status_message = Some(format!(
                                        "Playing locally: {} ({kb} KB)", song.label
                                    ));
                                } else if app.local_current_song.is_none() {
                                    app.local_current_song = app.local_queue.first().cloned();
                                    app.local_queue_pos = 0;
                                    app.status_message = Some(format!(
                                        "Queued: {} ({kb} KB)", song.label
                                    ));
                                }
                            }
                            Err(e) => {
                                app.status_message = Some(format!("Playback error: {e}"));
                            }
                        }
                    }
                }
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
                        if a == "toggle_backend" {
                            let new_backend = {
                                let mut app = app.lock().unwrap();
                                app.toggle_backend();
                                app.backend.clone()
                            };
                            if new_backend == PlaybackBackend::Local {
                                match init_local_player(device_name.as_deref()) {
                                    Ok(lp) => {
                                        local_player = Some(lp);
                                        app.lock().unwrap().status_message =
                                            Some("Local playback mode (songs only)".to_string());
                                    }
                                    Err(e) => {
                                        let mut app = app.lock().unwrap();
                                        app.toggle_backend(); // revert
                                        app.status_message =
                                            Some(format!("Local audio unavailable: {e}"));
                                    }
                                }
                            } else {
                                app.lock().unwrap().status_message =
                                    Some("Remote playback mode".to_string());
                            }
                        } else if a.starts_with("local_volume:") {
                            let vol: u32 = a.splitn(2, ':').nth(1).unwrap_or("100").parse().unwrap_or(100);
                            if let Some(ref lp) = local_player {
                                lp.set_volume(vol as f32 / 100.0);
                            }
                            app.lock().unwrap().local_volume = vol;
                        } else if a == "local_toggle_pause" {
                            if let Some(ref lp) = local_player {
                                lp.toggle_pause();
                            }
                        } else if a == "local_stop" {
                            if let Some(ref mut lp) = local_player {
                                lp.stop();
                            }
                            app.lock().unwrap().local_current_song = None;
                        } else if a.starts_with("local_play_song:") {
                            let id: u32 = a.splitn(2, ':').nth(1).unwrap_or("0").parse().unwrap_or(0);
                            let song = {
                                let app = app.lock().unwrap();
                                app.all_songs.iter().find(|s| s.songid == id).cloned()
                            };
                            if let Some(song) = song {
                                {
                                    let mut app = app.lock().unwrap();
                                    app.clear_local_queue();
                                    app.push_local_queue(song.clone());
                                    app.local_queue_pos = 0;
                                    app.local_fetching = true;
                                }
                                let kodi = Arc::clone(&kodi_ref);
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    match fetch_local_bytes(&kodi, song.songid).await {
                                        Ok(bytes) => { let _ = tx.send(AppEvent::LocalBytesReady { bytes, song, clear: true }); }
                                        Err(e) => { let _ = tx.send(AppEvent::Error(format!("Local fetch: {e}"))); }
                                    }
                                });
                            }
                        } else if a.starts_with("local_queue_song:") {
                            let id: u32 = a.splitn(2, ':').nth(1).unwrap_or("0").parse().unwrap_or(0);
                            let song = {
                                let app = app.lock().unwrap();
                                app.all_songs.iter().find(|s| s.songid == id).cloned()
                            };
                            if let Some(song) = song {
                                {
                                    let mut app = app.lock().unwrap();
                                    app.push_local_queue(song.clone());
                                    app.local_fetching = true;
                                }
                                let kodi = Arc::clone(&kodi_ref);
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    match fetch_local_bytes(&kodi, song.songid).await {
                                        Ok(bytes) => { let _ = tx.send(AppEvent::LocalBytesReady { bytes, song, clear: false }); }
                                        Err(e) => { let _ = tx.send(AppEvent::Error(format!("Local fetch: {e}"))); }
                                    }
                                });
                            }
                        } else if a.starts_with("local_skip_fwd:") || a.starts_with("local_skip_bck:") {
                            let n: usize = a.splitn(2, ':').nth(1).unwrap_or("1").parse().unwrap_or(1);
                            let forward = a.starts_with("local_skip_fwd:");
                            let song = {
                                let mut app = app.lock().unwrap();
                                if app.local_queue.is_empty() {
                                    None
                                } else {
                                    let new_pos = if forward {
                                        (app.local_queue_pos + n).min(app.local_queue.len() - 1)
                                    } else {
                                        app.local_queue_pos.saturating_sub(n)
                                    };
                                    app.local_queue_pos = new_pos;
                                    let s = app.local_queue[new_pos].clone();
                                    app.local_current_song = Some(s.clone());
                                    app.local_fetching = true;
                                    app.local_position = 0;
                                    Some(s)
                                }
                            };
                            if let Some(song) = song {
                                if let Some(ref mut lp) = local_player {
                                    lp.stop();
                                }
                                let kodi = Arc::clone(&kodi_ref);
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    match fetch_local_bytes(&kodi, song.songid).await {
                                        Ok(bytes) => { let _ = tx.send(AppEvent::LocalBytesReady { bytes, song, clear: true }); }
                                        Err(e) => { let _ = tx.send(AppEvent::Error(format!("Skip: {e}"))); }
                                    }
                                });
                            }
                        } else if a.starts_with("local_play_album:")
                            || a.starts_with("local_play_artist:")
                            || a.starts_with("local_queue_album:")
                            || a.starts_with("local_queue_artist:")
                        {
                            let parts: Vec<&str> = a.splitn(2, ':').collect();
                            let id: u32 = parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
                            let clear = parts[0] == "local_play_album" || parts[0] == "local_play_artist";
                            let is_album = parts[0].contains("album");
                            let kodi = Arc::clone(&kodi_ref);
                            let tx = tx.clone();
                            tokio::spawn(async move {
                                match fetch_song_list(&kodi, id, is_album).await {
                                    Ok(songs) => { let _ = tx.send(AppEvent::LocalSongListReady { songs, clear }); }
                                    Err(e) => { let _ = tx.send(AppEvent::Error(format!("Fetch songs: {e}"))); }
                                }
                            });
                        } else if a == "toggle_repeat" {
                            let (backend, pid, current_repeat) = {
                                let app = app.lock().unwrap();
                                (app.backend.clone(), app.status.player_id, app.status.repeat.clone())
                            };
                            if backend == PlaybackBackend::Local {
                                app.lock().unwrap().cycle_repeat();
                            } else if let Some(pid) = pid {
                                let next = match current_repeat.as_str() {
                                    "off" => "one",
                                    "one" => "all",
                                    _ => "off",
                                };
                                let kodi = Arc::clone(&kodi_ref);
                                let tx = tx.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = kodi.set_repeat(pid, next).await {
                                        let _ = tx.send(AppEvent::Error(format!("Set repeat: {e}")));
                                    }
                                    tokio::time::sleep(Duration::from_millis(300)).await;
                                    if let Ok(s) = kodi.get_status().await {
                                        let _ = tx.send(AppEvent::StatusUpdate(s));
                                    }
                                });
                            }
                        } else {
                            // Remote Kodi action
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
                                // Give Kodi a moment to update its playlist
                                tokio::time::sleep(Duration::from_millis(300)).await;
                                let (s_res, q_res) =
                                    tokio::join!(kodi.get_status(), kodi.get_playlist());
                                if let Ok(s) = s_res {
                                    let _ = tx.send(AppEvent::StatusUpdate(s));
                                }
                                if let Ok(q) = q_res {
                                    let _ = tx.send(AppEvent::RemoteQueueUpdate(q));
                                }
                            });
                        }
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
    std::process::exit(0);
}

fn handle_key(app: &mut App, key: KeyEvent) -> Option<String> {
    // Overlays intercept all keys — any key closes them
    if app.show_help {
        app.show_help = false;
        return None;
    }
    if app.show_track_info {
        app.show_track_info = false;
        return None;
    }
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
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            // Queue without leaving search
            queue_selected(app)
        }
        KeyCode::Enter => {
            app.input_mode = InputMode::Normal;
            play_selected(app)
        }
        KeyCode::Tab => {
            app.toggle_search_mode();
            None
        }
        KeyCode::Backspace => {
            app.pop_search_char();
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
            app.pending_count.clear();
            app.go_top();
            return None;
        } else {
            app.pending_g = true;
            app.pending_count.clear();
            return None;
        }
    }
    app.pending_g = false;

    // Accumulate numeric count prefix (digits only; no modifier)
    if let KeyCode::Char(c) = key.code {
        if c.is_ascii_digit() && key.modifiers.is_empty() {
            app.pending_count.push(c);
            return None;
        }
    }

    // Consume and clear the accumulated count for the next action
    let count: usize = if app.pending_count.is_empty() {
        1
    } else {
        app.pending_count.parse().unwrap_or(1)
    };
    app.pending_count.clear();

    match key.code {
        KeyCode::Char('?') => {
            app.show_help = true;
            None
        }
        KeyCode::Char('q') => Some("quit".to_string()),
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Search;
            app.clear_search();
            None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.move_down_by(count, app.visible_rows);
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.move_up_by(count, app.visible_rows);
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
        KeyCode::Char('L') => Some("toggle_backend".to_string()),
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => queue_selected(app),
        KeyCode::Enter => play_selected(app),
        KeyCode::Char(' ') => {
            if app.backend == PlaybackBackend::Local {
                if app.local_current_song.is_some() {
                    Some("local_toggle_pause".to_string())
                } else {
                    play_selected(app)
                }
            } else if let Some(pid) = app.status.player_id {
                Some(format!("toggle_pause:{pid}"))
            } else {
                play_selected(app)
            }
        }
        KeyCode::Char('n') => {
            if app.backend == PlaybackBackend::Local {
                if app.local_queue.is_empty() { None } else { Some(format!("local_skip_fwd:{count}")) }
            } else if let Some(pid) = app.status.player_id {
                if count <= 1 {
                    Some(format!("next:{pid}"))
                } else {
                    Some(format!("skip_fwd:{count}"))
                }
            } else {
                None
            }
        }
        KeyCode::Char('p') => {
            if app.backend == PlaybackBackend::Local {
                if app.local_queue.is_empty() { None } else { Some(format!("local_skip_bck:{count}")) }
            } else if let Some(pid) = app.status.player_id {
                if count <= 1 {
                    Some(format!("prev:{pid}"))
                } else {
                    Some(format!("skip_bck:{count}"))
                }
            } else {
                None
            }
        }
        KeyCode::Char('s') => {
            if app.backend == PlaybackBackend::Local {
                Some("local_stop".to_string())
            } else {
                app.status.player_id.map(|pid| format!("stop:{pid}"))
            }
        }
        KeyCode::Char('+') | KeyCode::Char('=') if key.modifiers.contains(KeyModifiers::ALT) => {
            if app.backend == PlaybackBackend::Local {
                let vol = (app.local_volume + 1).min(100);
                Some(format!("local_volume:{vol}"))
            } else {
                let vol = (app.status.volume + 1).min(100);
                Some(format!("volume:{vol}"))
            }
        }

        KeyCode::Char('+') | KeyCode::Char('=') => {
            if app.backend == PlaybackBackend::Local {
                let vol = (app.local_volume + 5).min(100);
                Some(format!("local_volume:{vol}"))
            } else {
                let vol = (app.status.volume + 5).min(100);
                Some(format!("volume:{vol}"))
            }
        }
        KeyCode::Char('-') if key.modifiers.contains(KeyModifiers::ALT) => {
            if app.backend == PlaybackBackend::Local {
                let vol = app.local_volume.saturating_sub(1);
                Some(format!("local_volume:{vol}"))
            } else {
                let vol = app.status.volume.saturating_sub(1);
                Some(format!("volume:{vol}"))
            }
        }
        KeyCode::Char('-') => {
            if app.backend == PlaybackBackend::Local {
                let vol = app.local_volume.saturating_sub(5);
                Some(format!("local_volume:{vol}"))
            } else {
                let vol = app.status.volume.saturating_sub(5);
                Some(format!("volume:{vol}"))
            }
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
        KeyCode::Char('r') => Some("toggle_repeat".to_string()),
        KeyCode::Char('i') => {
            app.show_track_info = true;
            None
        }
        _ => None,
    }
}

fn play_selected(app: &App) -> Option<String> {
    let item = app.selected_item()?;
    if app.backend == PlaybackBackend::Local {
        match item {
            app::LibraryItem::Song(s) => Some(format!("local_play_song:{}", s.songid)),
            app::LibraryItem::Album(a) => Some(format!("local_play_album:{}", a.albumid)),
            app::LibraryItem::Artist(a) => Some(format!("local_play_artist:{}", a.artistid)),
        }
    } else {
        match item {
            app::LibraryItem::Artist(a) => Some(format!("play_artist:{}", a.artistid)),
            app::LibraryItem::Album(a) => Some(format!("play_album:{}", a.albumid)),
            app::LibraryItem::Song(s) => Some(format!("play_song:{}", s.songid)),
        }
    }
}

fn queue_selected(app: &App) -> Option<String> {
    let item = app.selected_item()?;
    if app.backend == PlaybackBackend::Local {
        match item {
            app::LibraryItem::Song(s) => Some(format!("local_queue_song:{}", s.songid)),
            app::LibraryItem::Album(a) => Some(format!("local_queue_album:{}", a.albumid)),
            app::LibraryItem::Artist(a) => Some(format!("local_queue_artist:{}", a.artistid)),
        }
    } else {
        match item {
            app::LibraryItem::Artist(a) => Some(format!("queue_artist:{}", a.artistid)),
            app::LibraryItem::Album(a) => Some(format!("queue_album:{}", a.albumid)),
            app::LibraryItem::Song(s) => Some(format!("queue_song:{}", s.songid)),
        }
    }
}

fn init_local_player(device_name: Option<&str>) -> anyhow::Result<player::LocalPlayer> {
    player::LocalPlayer::new(device_name)
}

async fn fetch_song_list(kodi: &kodi::KodiClient, id: u32, is_album: bool) -> anyhow::Result<Vec<kodi::Song>> {
    if is_album {
        kodi.get_songs_for_album(id).await
    } else {
        kodi.get_songs_for_artist(id).await
    }
}

async fn fetch_local_bytes(kodi: &kodi::KodiClient, song_id: u32) -> anyhow::Result<Vec<u8>> {
    let file_path = kodi.get_song_file(song_id).await?;
    let bytes = kodi.fetch_vfs_bytes(&file_path).await?;
    Ok(bytes)
}

async fn dispatch_action(
    kodi: &KodiClient,
    action: &str,
    status: &kodi::PlayerStatus,
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
        "skip_fwd" | "skip_bck" => {
            if let Some(pid) = status.player_id {
                let n: usize = parts[1].parse()?;
                let new_pos = if parts[0] == "skip_fwd" {
                    status.playlist_pos + n
                } else {
                    status.playlist_pos.saturating_sub(n)
                };
                kodi.goto_position(pid, new_pos).await?
            }
        }
        _ => {}
    }
    Ok(())
}
