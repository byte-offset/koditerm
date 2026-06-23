use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const MB_BASE: &str = "https://musicbrainz.org/ws/2";
const USER_AGENT: &str = "koditerm/0.1.0 (https://github.com/user/koditerm)";

pub struct MbClient {
    client: Client,
}

#[derive(Debug, Clone)]
pub struct ArtistRelation {
    pub mbid: String,
    pub name: String,
    pub relation_label: String,
}

#[derive(Deserialize)]
struct ArtistResp {
    relations: Option<Vec<Relation>>,
}

#[derive(Deserialize)]
struct Relation {
    #[serde(rename = "type")]
    rel_type: String,
    #[serde(rename = "target-type")]
    target_type: String,
    artist: Option<ArtistRef>,
}

#[derive(Deserialize)]
struct ArtistRef {
    id: String,
    name: String,
}

impl MbClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder().user_agent(USER_AGENT).build()?;
        Ok(MbClient { client })
    }

    /// Fetch artist relations. Sleeps 1 s first to respect MB rate limit.
    pub async fn get_artist_relations(&self, mbid: &str) -> Result<Vec<ArtistRelation>> {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let url = format!("{}/artist/{}?inc=artist-rels&fmt=json", MB_BASE, mbid);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("MusicBrainz: HTTP {}", resp.status()));
        }
        let data: ArtistResp = resp.json().await?;
        Ok(data
            .relations
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.target_type == "artist")
            .filter_map(|r| {
                let label = match r.rel_type.as_str() {
                    "collaboration" => "collaborated with",
                    "member of band" => "member of band",
                    "supporting musician"
                    | "instrumental supporting musician"
                    | "vocal supporting musician" => "session musician for",
                    "conductor" => "conducted by",
                    _ => return None,
                };
                let a = r.artist?;
                Some(ArtistRelation {
                    mbid: a.id,
                    name: a.name,
                    relation_label: label.to_string(),
                })
            })
            .collect())
    }
}
