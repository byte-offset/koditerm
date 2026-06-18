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
        let result = self
            .call(
                "AudioLibrary.GetArtists",
                json!({ "limits": { "start": 0, "end": 5000 }, "sort": { "method": "artist" } }),
            )
            .await?;
        let artists: Vec<Artist> =
            serde_json::from_value(result["artists"].clone()).unwrap_or_default();
        Ok(artists)
    }

    pub async fn get_albums(&self) -> Result<Vec<Album>> {
        let result = self
            .call(
                "AudioLibrary.GetAlbums",
                json!({
                    "limits": { "start": 0, "end": 10000 },
                    "properties": ["artist", "year"],
                    "sort": { "method": "album" }
                }),
            )
            .await?;
        let albums: Vec<Album> =
            serde_json::from_value(result["albums"].clone()).unwrap_or_default();
        Ok(albums)
    }

    pub async fn get_songs(&self) -> Result<Vec<Song>> {
        let result = self
            .call(
                "AudioLibrary.GetSongs",
                json!({
                    "limits": { "start": 0, "end": 50000 },
                    "properties": ["artist", "album", "track", "duration"],
                    "sort": { "method": "title" }
                }),
            )
            .await?;
        let songs: Vec<Song> = serde_json::from_value(result["songs"].clone()).unwrap_or_default();
        Ok(songs)
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
            })
        }
    }
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
