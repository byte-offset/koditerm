# koditerm — Developer Notes

Terminal UI for controlling Kodi via JSON-RPC, with optional local audio playback via rodio.

## Quick orientation

```
src/
  main.rs    — event loop, key handling, action dispatch, local player lifetime
  app.rs     — App struct (all UI state), filter/navigation logic
  ui.rs      — ratatui rendering (draw_now_playing, draw_library, draw_queue)
  kodi.rs    — KodiClient: JSON-RPC calls, VFS streaming, status polling
  player.rs  — LocalPlayer wrapping rodio 0.22 (MixerDeviceSink + Player)
  config.rs  — TOML config at ~/.config/koditerm/config.toml
```

## Config

```toml
# ~/.config/koditerm/config.toml
[my-kodi]
host = "192.168.1.5"
port = 80
username = "kodi"
password = "kodi"
default = true
```

Multiple systems are supported; `--system <name>` selects a non-default one. The file is
auto-created with a placeholder on first run.

## Architecture

**Main loop** runs inside `#[tokio::main]` but the UI event loop is synchronous
(`rx.try_recv()` up to 20 events, then `tokio::time::sleep(16ms)`). Everything async
runs in spawned tasks and communicates back via an `mpsc::unbounded_channel<AppEvent>`.

**App state** is wrapped in `Arc<Mutex<App>>`. The mutex is locked briefly per event;
it is never held across an `.await`.

**AppEvent variants:**
- `ArtistsLoaded`, `AlbumsLoaded`, `SongsLoaded` — library load results
- `StatusUpdate(PlayerStatus)` — remote Kodi player state (polled every 2s)
- `RemoteQueueUpdate(Vec<Song>)` — Kodi playlist (polled alongside status)
- `Key(KeyEvent)` — from crossterm
- `Tick` — 200ms timer, drives local playback state sync
- `LocalBytesReady { bytes, song, clear }` — fetched audio bytes ready to play
- `Error(String)` — displayed in status bar

## Backend modes

`App::backend: PlaybackBackend` is either `Remote` or `Local`, toggled with `L`.

### Remote mode
Actions (play, queue, pause, stop, volume, next, prev) are dispatched to Kodi via
`dispatch_action()` → JSON-RPC. After any action, a 300ms delay then a
`tokio::join!(get_status(), get_playlist())` re-fetches state.

### Local mode
`local_player: Option<LocalPlayer>` lives in `main()` (not in `App`) because rodio
is not `Send`. It is created on first switch to local mode and kept alive thereafter.

**Audio device selection** (CLI flags):
- `--list-devices` — prints output device names and exits
- `--device <partial>` — matches by substring (e.g. `--device pipewire`)

**Local playback flow:**
1. User presses Enter on a Song → `local_play_song:{id}` action
2. Main loop: `clear_local_queue()`, `push_local_queue(song)`, `local_fetching = true`
3. Spawned task: `get_song_file(id)` → `fetch_vfs_bytes(path)` → `LocalBytesReady`
4. Main loop handles `LocalBytesReady`: calls `lp.play_bytes(bytes)`, sets
   `local_current_song`, clears `local_fetching`, resets `local_position = 0`
5. Every Tick: `app.local_paused = lp.is_paused()`, `app.local_position = lp.position()`
6. Auto-advance: on Tick, if `lp.empty() && !local_fetching && local_current_song.is_some()`
   and there is a next song in the queue, spawn a fetch for it

**VFS streaming**: Kodi exposes files at `{base_url}/vfs/{percent_encoded_path}`.
The file path comes from `AudioLibrary.GetSongDetails` with `["file"]` property.
The percent-encoding function encodes everything except unreserved chars (including `/`).
A response < 512 bytes is treated as an error (Kodi sometimes returns an error page).

## rodio 0.22 API (important — broke from 0.17)

Old API (`OutputStream` + `Sink`) was removed. New API:

```rust
// Open device
let mut sink = DeviceSinkBuilder::open_default_sink()?;
// or: DeviceSinkBuilder::from_device(cpal_device)?.open_stream()?;
sink.log_on_drop(false);  // suppress drop message to stderr

// Create player (equivalent of old Sink)
let player = Player::connect_new(&sink.mixer());

// Play
player.stop(); player.play();  // clear current, un-pause
player.append(source);         // append() blocks ~5ms if stopped flag is pending

// Position
player.get_pos() -> Duration   // accurate even through pauses
```

`MixerDeviceSink` must be kept alive for the duration of playback. `Player::drop()`
stops audio. cpal is re-exported as `rodio::cpal` — no direct cpal dep needed.

`Player::clear()` blocks (calls `sleep_until_end()`). Prefer `stop()` + `play()` before
`append()` to replace current playback without a synchronous wait.

## Library loading

Three parallel spawned tasks load artists, albums, songs independently. Each sends
its result as an AppEvent so the UI shows data as it arrives.

**Unlimited loading**: A two-request pattern avoids the Kodi default limit:
1. Call with `limits: {start:0, end:1}` → get `limits.total` from response
2. Call with `limits: {start:0, end:total}` → get all items

Never paginate in a loop — Kodi's JSON-RPC is fast enough that a single large request
is better than many small ones, and pagination stalls on 100k+ song libraries.

## UI layout

```
┌─ Now Playing ─────────────────────────────────────────────┐  (4 rows)
│ ▶ Song Title   Artist — Album         0:32 / 4:15  🔊 80% │
│ ████████████░░░░░░░░░░░░░░░░░░░░░░░░   0:32 / 4:15  🔊100%│
└───────────────────────────────────────────────────────────┘
┌─ Library pane (F1-F4 scope tabs) ──┐ ┌─ Queue N/M ─────┐  (Min(0))
│ ART  Artist                        │ │ ▶ Current Song   │
│ ALB  Album        Artist (2019)    │ │   Next Song      │
│ SNG  Song Title   Artist — Album   │ │   ...            │
└────────────────────────────────────┘ └─────────────────-┘
┌─ koditerm ────────────────┐ ┌─ key hints ──────────────┐  (3 rows)
│ status / search query     │ │ /search  j/k nav  ...    │
└───────────────────────────┘ └──────────────────────────┘
```

Queue pane is always visible (34 chars wide). In remote mode it shows Kodi's playlist
(polled every 2s). In local mode it shows `app.local_queue`.

## Key bindings reference

| Key | Normal | Search |
|-----|--------|--------|
| `/` | Enter search | — |
| `Enter` | Play (clear+play) | Play |
| `a` | Add to queue | — |
| `F5` | — | Add to queue |
| `Space` | Pause/resume (or play if nothing) | — |
| `n` / `p` | Next / prev track | — |
| `s` | Stop | — |
| `+`/`=` / `-` | Volume ±5% | — |
| `L` | Toggle local/remote backend | — |
| `j`/`k`, `↓`/`↑` | Navigate | Navigate |
| `Ctrl+d`/`u` | Half page down/up | Half page down/up |
| `Ctrl+n`/`p` | Next/prev scope | Next/prev scope |
| `F1`–`F4` | Set scope (All/Artists/Albums/Songs) | Set scope |
| `gg` / `G` | Top / bottom | — |
| `Tab` | — | Toggle exact/fuzzy search |
| `?` | Help overlay | — |
| `q` | Quit | — |

In local mode: volume keys control `LocalPlayer.set_volume()` (not Kodi).
In remote mode: volume keys call `Application.SetVolume` on Kodi.

## Pitfalls & lessons learned

**Queue pane design**: An early attempt made the queue replace the library pane.
User rejected this — the queue must be a persistent right-hand pane, always visible
alongside the library.

**ALSA stderr noise**: On Linux with PipeWire's ALSA compatibility layer, rodio/cpal
prints "cannot find card 0" messages during init. An earlier version redirected stderr
to `/dev/null` during init to suppress these — but this hid real errors and was the
likely cause of silent audio failures. The suppression has been removed. If ALSA noise
is a problem in the future, the right fix is `--device pipewire` to bypass the ALSA
compat layer entirely.

**rodio `LocalPlayer` not Send**: rodio's audio types are not `Send`. `LocalPlayer`
must live in the `main()` function, not inside `Arc<Mutex<App>>`. The tick event
handler accesses it directly in the main loop.

**Status polling after actions**: Remote Kodi state (playlist, player position) takes
a moment to update after an action. The code waits 300ms after any action before
re-fetching. Without this delay, the queue/status would show stale data.

**Kodi playlist ID**: Audio playlist is always `playlistid: 0`.

**Song `duration` field**: Comes from `AudioLibrary.GetSongs` with `["duration"]`
property. It's `Option<u32>` because `Playlist.GetItems` doesn't return duration,
so remote queue songs have `duration: None`. The local progress bar handles this
gracefully (`dur = 0` → ratio = 0.0).

## Potential improvements

- Seek support (`Player.Seek` for remote; `player.try_seek()` for local)
- Artist/album drill-down in local mode (currently only songs play locally)
- Persistent local queue across backend switches
- Kodi WebSocket notifications instead of polling (eliminates the 2s lag)
- Config editing via command or TUI
- Multiple Kodi system quick-switch
