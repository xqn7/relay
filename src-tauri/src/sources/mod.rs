//! Shared source interface. Each service lives in its own module, so when a service changes,
//! the fix stays inside that one file.

pub mod apple;
pub mod deezer;
pub mod links;
pub mod soundcloud;
pub mod spotify;
pub mod youtube;

use crate::error::AppResult;
use serde::{Deserialize, Serialize};

pub const YOUTUBE: &str = "youtube";
pub const SOUNDCLOUD: &str = "soundcloud";
pub const SPOTIFY: &str = "spotify";
pub const DEEZER: &str = "deezer";
pub const APPLE: &str = "apple";
pub const LOCAL: &str = "local";

/// Services used only to import lists. Their songs play through a matched YouTube Music
/// (or SoundCloud) track, never their own audio.
pub fn is_import_only(source: &str) -> bool {
    matches!(source, SPOTIFY | DEEZER | APPLE)
}

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
    /// For imported songs: the source the matched audio comes from, or "none" when nothing
    /// matched. Filled in when lists are read from the library; never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
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
            && matches!(self.source.as_str(), YOUTUBE | SOUNDCLOUD | SPOTIFY | DEEZER | APPLE | LOCAL)
            && self.title.len() < 1024
            && self.artists.len() < 64
    }

    /// A track as an import service describes it, before matching.
    pub fn imported(source: &str, source_id: &str, title: &str, artists: Vec<String>, duration_ms: Option<u64>, cover: Option<String>, album: Option<String>) -> Self {
        Track {
            id: Self::make_id(source, source_id),
            source: source.into(),
            source_id: source_id.into(),
            title: title.trim().into(),
            artists: artists.into_iter().filter(|a| !a.trim().is_empty()).map(|name| ArtistRef { id: None, name: name.trim().into() }).collect(),
            album: album.map(|name| AlbumRef { id: String::new(), name }),
            duration_ms,
            cover: cover.filter(|c| c.starts_with("https://")),
            is_video: false,
            link: None,
            via: None,
        }
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
    pub year: Option<u16>,
}

/// An album (or EP/single) page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumPage {
    pub source: String,
    pub id: String,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub cover: Option<String>,
    pub year: Option<u16>,
    /// "album", "ep" or "single"
    pub kind: String,
    pub tracks: Vec<Track>,
    pub link: Option<String>,
}

/// An artist page: hero, about text, top songs and releases.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistPage {
    pub source: String,
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub about: Option<String>,
    pub top_tracks: Vec<Track>,
    pub albums: Vec<Collection>,
    pub singles: Vec<Collection>,
    pub similar: Vec<ArtistCard>,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistCard {
    pub source: String,
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
}

/// Why a song from an imported list isn't in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub title: String,
    pub artist: String,
    /// "deleted", "private", "unavailable", "no_match"
    pub reason: String,
}

/// A playlist, album or single song read from an import link.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteList {
    /// "<service>:<kind>:<id>", the same for every link to this list, so re-imports update it.
    pub origin: String,
    pub service: String,
    pub kind: String,
    pub title: String,
    pub owner: String,
    pub cover: Option<String>,
    pub tracks: Vec<Track>,
    pub skipped: Vec<Skipped>,
    /// The service only gave part of the list (e.g. Spotify's 100-song embed page).
    pub truncated: bool,
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
