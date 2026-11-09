//! Shared source interface. Each service lives in its own module, so when a service changes,
//! the fix stays inside that one file.

pub mod soundcloud;
pub mod youtube;

use crate::error::AppResult;
use serde::{Deserialize, Serialize};

pub const YOUTUBE: &str = "youtube";
pub const SOUNDCLOUD: &str = "soundcloud";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArtistRef {
    pub id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AlbumRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Stable across PCs: "<source>:<source_id>".
    pub id: String,
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    pub duration_ms: Option<u64>,
    pub cover: Option<String>,
    /// Music video rather than a plain audio upload; the matcher prefers plain audio.
    #[serde(default)]
    pub is_video: bool,
    pub link: Option<String>,
}

impl Track {
    pub fn make_id(source: &str, source_id: &str) -> String {
        format!("{source}:{source_id}")
    }

    pub fn artist_line(&self) -> String {
        self.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
    }

    /// Ids and text come from external services; reject anything that could not be a real track.
    pub fn is_sane(&self) -> bool {
        !self.source_id.is_empty()
            && self.source_id.len() < 128
            && !self.title.trim().is_empty()
            && self.id == Self::make_id(&self.source, &self.source_id)
            && (self.source == YOUTUBE || self.source == SOUNDCLOUD)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub source: String,
    /// "album" or "playlist"
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistCard {
    pub source: String,
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub source: String,
    pub tracks: Vec<Track>,
    pub albums: Vec<Collection>,
    pub artists: Vec<ArtistCard>,
    pub playlists: Vec<Collection>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SearchKind {
    All,
    Songs,
    Albums,
    Artists,
    Playlists,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    Best,
    DataSaver,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum StreamKind {
    /// A single file that supports HTTP Range requests.
    Direct,
    /// An HLS playlist whose segments are joined into one file.
    Hls,
}

#[derive(Debug, Clone)]
pub struct ResolvedStream {
    pub url: String,
    pub kind: StreamKind,
    pub mime: String,
    /// Unix seconds after which the URL stops working.
    pub expires_at: i64,
    pub size: Option<u64>,
    /// Relative loudness reported by the service (YouTube); used for normalization.
    pub loudness_db: Option<f32>,
    /// Some stream URLs only work with the User-Agent of the client that requested them.
    pub user_agent: Option<String>,
}

#[allow(async_fn_in_trait)]
pub trait Source {
    async fn search(&self, query: &str, kind: SearchKind) -> AppResult<SearchResults>;
    async fn resolve(&self, track: &Track, quality: Quality) -> AppResult<ResolvedStream>;
    /// Songs of an album or playlist, used to play it from search results.
    async fn collection_tracks(&self, kind: &str, id: &str) -> AppResult<Vec<Track>>;
}
