use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::KodiSystem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub artistid: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub albumid: u32,
    pub label: String,
    pub artist: Vec<String>,
    pub year: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    pub songid: u32,
    pub label: String,
    pub artist: Vec<String>,
    pub album: String,
    pub track: Option<u32>,
    pub duration: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PlayerStatus {
    pub player_id: Option<i64>,
    pub playing: bool,
    pub current_item: Option<CurrentItem>,
    pub position: u32,
    pub duration: u32,
    pub speed: i64,
    pub volume: u32,
    pub muted: bool,
    pub playlist_pos: usize,
}

#[derive(Debug, Clone)]
pub struct CurrentItem {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: u32,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        PlayerStatus {
            player_id: None,
            playing: false,
            current_item: None,
            position: 0,
            duration: 0,
            speed: 0,
            volume: 100,
            muted: false,
            playlist_pos: 0,
        }
    }
}

pub struct KodiClient {
    client: Client,
    pub name: String,
    system: KodiSystem,
}

impl KodiClient {
    pub fn new(name: &str, system: KodiSystem) -> Result<Self> {
        let client = Client::new();
        Ok(KodiClient { client, name: name.to_string(), system })
    }

    async fn count(&self, method: &str, mut params: Value) -> Result<usize> {
        params["limits"] = json!({ "start": 0, "end": 1 });
        let result = self.call(method, params).await?;
        Ok(result["limits"]["total"].as_u64().unwrap_or(0) as usize)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let url = format!("{}/jsonrpc", self.system.base_url());
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.system.username, Some(&self.system.password))
            .json(&body)
            .send()
            .await?;
        let val: Value = resp.json().await?;
        if let Some(err) = val.get("error") {
            return Err(anyhow!("Kodi error: {}", err));
        }
        Ok(val["result"].clone())
    }

    pub async fn get_artists(&self) -> Result<Vec<Artist>> {
        let total = self.count("AudioLibrary.GetArtists", json!({})).await?;
        let result = self
            .call(
                "AudioLibrary.GetArtists",
                json!({
                    "limits": { "start": 0, "end": total },
                    "sort": { "method": "artist" }
                }),
            )
            .await?;
        Ok(serde_json::from_value(result["artists"].clone()).unwrap_or_default())
    }

    pub async fn get_albums(&self) -> Result<Vec<Album>> {
        let total = self.count("AudioLibrary.GetAlbums", json!({})).await?;
        let result = self
            .call(
                "AudioLibrary.GetAlbums",
                json!({
                    "limits": { "start": 0, "end": total },
                    "properties": ["artist", "year"],
                    "sort": { "method": "album" }
                }),
            )
            .await?;
        Ok(serde_json::from_value(result["albums"].clone()).unwrap_or_default())
    }

    pub async fn get_songs(&self) -> Result<Vec<Song>> {
        let total = self
            .count("AudioLibrary.GetSongs", json!({ "properties": [] }))
            .await?;
        let result = self
            .call(
                "AudioLibrary.GetSongs",
                json!({
                    "limits": { "start": 0, "end": total },
                    "properties": ["artist", "album", "track", "duration"],
                    "sort": { "method": "title" }
                }),
            )
            .await?;
        Ok(serde_json::from_value(result["songs"].clone()).unwrap_or_default())
    }

    pub async fn get_songs_for_album(&self, album_id: u32) -> Result<Vec<Song>> {
        let result = self
            .call(
                "AudioLibrary.GetSongs",
                json!({
                    "filter": { "albumid": album_id },
                    "properties": ["artist", "album", "track", "duration"],
                    "sort": { "method": "track" }
                }),
            )
            .await?;
        let songs: Vec<Song> = serde_json::from_value(result["songs"].clone()).unwrap_or_default();
        Ok(songs)
    }

    pub async fn get_songs_for_artist(&self, artist_id: u32) -> Result<Vec<Song>> {
        let result = self
            .call(
                "AudioLibrary.GetSongs",
                json!({
                    "filter": { "artistid": artist_id },
                    "properties": ["artist", "album", "track", "duration"],
                    "sort": { "method": "album" }
                }),
            )
            .await?;
        let songs: Vec<Song> = serde_json::from_value(result["songs"].clone()).unwrap_or_default();
        Ok(songs)
    }

    pub async fn playlist_clear(&self) -> Result<()> {
        self.call("Playlist.Clear", json!({ "playlistid": 0 }))
            .await?;
        Ok(())
    }

    pub async fn queue_song(&self, song_id: u32) -> Result<()> {
        self.call(
            "Playlist.Add",
            json!({ "playlistid": 0, "item": { "songid": song_id } }),
        )
        .await?;
        Ok(())
    }

    pub async fn queue_album(&self, album_id: u32) -> Result<()> {
        self.call(
            "Playlist.Add",
            json!({ "playlistid": 0, "item": { "albumid": album_id } }),
        )
        .await?;
        Ok(())
    }

    pub async fn queue_artist(&self, artist_id: u32) -> Result<()> {
        self.call(
            "Playlist.Add",
            json!({ "playlistid": 0, "item": { "artistid": artist_id } }),
        )
        .await?;
        Ok(())
    }

    pub async fn play_song(&self, song_id: u32) -> Result<()> {
        self.playlist_clear().await?;
        self.queue_song(song_id).await?;
        self.call("Player.Open", json!({ "item": { "playlistid": 0 } }))
            .await?;
        Ok(())
    }

    pub async fn play_album(&self, album_id: u32) -> Result<()> {
        self.playlist_clear().await?;
        self.queue_album(album_id).await?;
        self.call("Player.Open", json!({ "item": { "playlistid": 0 } }))
            .await?;
        Ok(())
    }

    pub async fn play_artist(&self, artist_id: u32) -> Result<()> {
        self.playlist_clear().await?;
        self.queue_artist(artist_id).await?;
        self.call("Player.Open", json!({ "item": { "playlistid": 0 } }))
            .await?;
        Ok(())
    }

    pub async fn toggle_pause(&self, player_id: i64) -> Result<()> {
        self.call("Player.PlayPause", json!({ "playerid": player_id }))
            .await?;
        Ok(())
    }

    pub async fn stop(&self, player_id: i64) -> Result<()> {
        self.call("Player.Stop", json!({ "playerid": player_id }))
            .await?;
        Ok(())
    }

    pub async fn goto_position(&self, player_id: i64, pos: usize) -> Result<()> {
        self.call("Player.GoTo", json!({ "playerid": player_id, "to": pos }))
            .await?;
        Ok(())
    }

    pub async fn next_track(&self, player_id: i64) -> Result<()> {
        self.call(
            "Player.GoTo",
            json!({ "playerid": player_id, "to": "next" }),
        )
        .await?;
        Ok(())
    }

    pub async fn prev_track(&self, player_id: i64) -> Result<()> {
        self.call(
            "Player.GoTo",
            json!({ "playerid": player_id, "to": "previous" }),
        )
        .await?;
        Ok(())
    }

    pub async fn seek(&self, player_id: i64, seconds: i32) -> Result<()> {
        self.call(
            "Player.Seek",
            json!({ "playerid": player_id, "value": { "seconds": seconds } }),
        )
        .await?;
        Ok(())
    }

    pub async fn get_playlist(&self) -> Result<Vec<Song>> {
        #[derive(Deserialize)]
        struct Item {
            id: Option<u32>,
            label: String,
            #[serde(default)]
            artist: Vec<String>,
            #[serde(default)]
            album: String,
        }
        let result = self
            .call(
                "Playlist.GetItems",
                json!({ "playlistid": 0, "properties": ["artist", "album"] }),
            )
            .await?;
        let items: Vec<Item> =
            serde_json::from_value(result["items"].clone()).unwrap_or_default();
        Ok(items
            .into_iter()
            .map(|it| Song {
                songid: it.id.unwrap_or(0),
                label: it.label,
                artist: it.artist,
                album: it.album,
                track: None,
                duration: None,
            })
            .collect())
    }

    pub async fn get_song_file(&self, song_id: u32) -> Result<String> {
        let result = self
            .call(
                "AudioLibrary.GetSongDetails",
                json!({ "songid": song_id, "properties": ["file"] }),
            )
            .await?;
        let file = result["songdetails"]["file"]
            .as_str()
            .ok_or_else(|| anyhow!("no file path in song details"))?
            .to_string();
        Ok(file)
    }

    pub async fn fetch_vfs_bytes(&self, file_path: &str) -> Result<Vec<u8>> {
        let encoded = percent_encode(file_path);
        let url = format!("{}/vfs/{}", self.system.base_url(), encoded);
        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.system.username, Some(&self.system.password))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow!("VFS fetch failed: HTTP {}", resp.status()));
        }
        let bytes = resp.bytes().await?;
        if bytes.len() < 512 {
            return Err(anyhow!(
                "VFS response too small ({} bytes) — path may be wrong",
                bytes.len()
            ));
        }
        Ok(bytes.to_vec())
    }

    pub async fn set_volume(&self, volume: u32) -> Result<()> {
        self.call("Application.SetVolume", json!({ "volume": volume }))
            .await?;
        Ok(())
    }

    pub async fn get_status(&self) -> Result<PlayerStatus> {
        // Get active players
        let players = self
            .call("Player.GetActivePlayers", json!({}))
            .await?;
        let players = players.as_array().cloned().unwrap_or_default();

        // Get volume
        let app_props = self
            .call(
                "Application.GetProperties",
                json!({ "properties": ["volume", "muted"] }),
            )
            .await?;
        let volume = app_props["volume"].as_u64().unwrap_or(100) as u32;
        let muted = app_props["muted"].as_bool().unwrap_or(false);

        // Find audio player
        let audio_player = players.iter().find(|p| p["type"] == "audio");
        let player_id = audio_player.and_then(|p| p["playerid"].as_i64());

        if let Some(pid) = player_id {
            let props = self
                .call(
                    "Player.GetProperties",
                    json!({
                        "playerid": pid,
                        "properties": ["speed", "time", "totaltime", "position"]
                    }),
                )
                .await?;

            let speed = props["speed"].as_i64().unwrap_or(0);
            let playlist_pos = props["position"].as_u64().unwrap_or(0) as usize;
            let pos_secs = time_to_seconds(&props["time"]);
            let dur_secs = time_to_seconds(&props["totaltime"]);

            let item = self
                .call(
                    "Player.GetItem",
                    json!({
                        "playerid": pid,
                        "properties": ["title", "artist", "album", "duration"]
                    }),
                )
                .await?;

            let current = CurrentItem {
                title: item["item"]["label"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                artist: item["item"]["artist"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                album: item["item"]["album"].as_str().unwrap_or("").to_string(),
                duration: dur_secs,
            };

            Ok(PlayerStatus {
                player_id: Some(pid),
                playing: speed != 0,
                current_item: Some(current),
                position: pos_secs,
                duration: dur_secs,
                speed,
                volume,
                muted,
                playlist_pos,
            })
        } else {
            Ok(PlayerStatus {
                player_id: None,
                playing: false,
                current_item: None,
                position: 0,
                duration: 0,
                speed: 0,
                volume,
                muted,
                playlist_pos: 0,
            })
        }
    }
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn time_to_seconds(v: &Value) -> u32 {
    let h = v["hours"].as_u64().unwrap_or(0);
    let m = v["minutes"].as_u64().unwrap_or(0);
    let s = v["seconds"].as_u64().unwrap_or(0);
    (h * 3600 + m * 60 + s) as u32
}

pub fn format_duration(secs: u32) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
