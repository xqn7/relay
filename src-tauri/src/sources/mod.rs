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

/// How several artists are joined in one credit ("A & B", "A x B", "A feat. B").
const ARTIST_JOINS: &[&str] = &[" feat. ", " ft. ", " featuring ", " Feat. ", " Ft. ", " x ", " X ", " & ", ", "];

/// Splits one credit naming several artists into one name each, so each can be opened on its
/// own. Names that are a single artist come back unchanged.
pub fn split_credit(credit: &str) -> Vec<String> {
    let mut parts = vec![credit.trim().to_string()];
    for join in ARTIST_JOINS {
        parts = parts.iter().flat_map(|p| p.split(join).map(|s| s.trim().to_string()).collect::<Vec<_>>()).collect();
    }
    // Uploads also write "A,B" without a space; a comma between digits ("10,000") stays.
    parts = parts.iter().flat_map(|p| split_bare_commas(p)).collect();
    parts.retain(|p| !p.is_empty());
    if parts.is_empty() {
        vec![credit.trim().to_string()]
    } else {
        parts
    }
}

/// SoundCloud uploads often name the song "Artists - Title". When the part before the dash
/// names the credited artist, it holds the song's artists and the rest is the real title.
/// Titles like "Intro - Live" (no credited artist before the dash) stay as they are.
pub fn split_soundcloud_title(title: &str, credited: &str) -> Option<(Vec<String>, String)> {
    let (left, right) = title.split_once(" - ").or_else(|| title.split_once(" \u{2013} "))?;
    let right = right.trim();
    if right.is_empty() {
        return None;
    }
    let names = split_credit(left);
    let known: Vec<String> = split_credit(credited).iter().map(|n| n.to_lowercase()).collect();
    names.iter().any(|n| known.contains(&n.to_lowercase())).then(|| (names, right.to_string()))
}

/// A SoundCloud song's title and artists with "Artists - Title" taken apart; the profile
/// link stays on the artist named like `linked_name` (the uploader).
pub fn clean_soundcloud(title: String, artists: Vec<ArtistRef>, linked_name: Option<&str>) -> (String, Vec<ArtistRef>) {
    let credited = artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
    let owner = artists.iter().find_map(|a| a.id.clone());
    match split_soundcloud_title(&title, &credited) {
        Some((names, clean)) => {
            let first = artists.first().map(|a| a.name.clone());
            let joined = vec![ArtistRef { id: owner, name: names.join(", ") }];
            (clean, split_artists(joined, linked_name.or(first.as_deref())))
        }
        None => (title, split_artists(artists, linked_name)),
    }
}

fn split_bare_commas(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out = vec![String::new()];
    for (i, &c) in chars.iter().enumerate() {
        let in_number = i > 0 && chars[i - 1].is_ascii_digit() && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        if c == ',' && !in_number {
            out.push(String::new());
        } else if let Some(last) = out.last_mut() {
            last.push(c);
        }
    }
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

impl Track {
    /// Songs saved before credits were split (or saved with "A,B") are split when read, so
    /// every artist on them can be opened. Only for the services that give one joined credit;
    /// imported songs already list each artist on their own.
    pub fn with_split_credits(mut self) -> Self {
        let artists = std::mem::take(&mut self.artists);
        self.artists = match self.source.as_str() {
            SOUNDCLOUD => {
                let (title, artists) = clean_soundcloud(std::mem::take(&mut self.title), artists, None);
                self.title = title;
                artists
            }
            // YouTube links each real artist on its own; only unlinked text can be a joined credit.
            YOUTUBE => artists
                .into_iter()
                .flat_map(|a| match a.id {
                    Some(_) => vec![a],
                    None => split_credit(&a.name).into_iter().map(|name| ArtistRef { id: None, name }).collect(),
                })
                .collect(),
            _ => artists,
        };
        self
    }
}

#[cfg(test)]
mod credit_tests {
    use super::*;

    #[test]
    fn splits_joined_credits() {
        assert_eq!(split_credit("Calvin Harris & Dua Lipa"), vec!["Calvin Harris", "Dua Lipa"]);
        assert_eq!(split_credit("A x B feat. C"), vec!["A", "B", "C"]);
        assert_eq!(split_credit("Adele"), vec!["Adele"]);
        assert_eq!(split_credit("Lessobaby,YND SOULJA,BLACK 016"), vec!["Lessobaby", "YND SOULJA", "BLACK 016"]);
        assert_eq!(split_credit("10,000 Maniacs"), vec!["10,000 Maniacs"]);
    }

    #[test]
    fn takes_artists_out_of_soundcloud_titles() {
        let (names, title) = split_soundcloud_title("Lacazette x AVIE x KOLJA GOLDSTEIN - XXX", "Lacazette").unwrap();
        assert_eq!(names, vec!["Lacazette", "AVIE", "KOLJA GOLDSTEIN"]);
        assert_eq!(title, "XXX");
        assert_eq!(split_soundcloud_title("Intro - Live", "Lacazette"), None);
        assert_eq!(split_soundcloud_title("No dash here", "Lacazette"), None);
        let (title, artists) = clean_soundcloud("Lacazette x AVIE - XXX".into(), vec![ArtistRef { id: Some("7".into()), name: "Lacazette".into() }], None);
        assert_eq!(title, "XXX");
        assert_eq!(artists, vec![ArtistRef { id: Some("7".into()), name: "Lacazette".into() }, ArtistRef { id: None, name: "AVIE".into() }]);
    }

    #[test]
    fn saved_songs_are_split_when_read() {
        let mut t = Track::imported("x", "1", "Song", vec![], None, None, None);
        t.source = SOUNDCLOUD.into();
        t.artists = vec![ArtistRef { id: Some("9".into()), name: "Lessobaby,kolja goldstein".into() }];
        let names: Vec<String> = t.clone().with_split_credits().artists.into_iter().map(|a| a.name).collect();
        assert_eq!(names, vec!["Lessobaby", "kolja goldstein"]);
        t.source = YOUTUBE.into();
        t.artists = vec![ArtistRef { id: Some("UC1".into()), name: "Tyler, The Creator".into() }];
        assert_eq!(t.with_split_credits().artists.len(), 1, "a linked YouTube artist stays whole");
    }

    #[test]
    fn keeps_the_link_on_the_matching_part() {
        let a = split_artists(vec![ArtistRef { id: Some("42".into()), name: "Anna & Ben".into() }], Some("ben"));
        assert_eq!(a, vec![ArtistRef { id: None, name: "Anna".into() }, ArtistRef { id: Some("42".into()), name: "Ben".into() }]);
    }
}

/// A source's artist list with joined credits split. A credit that links to a profile keeps
/// that link on the part whose name matches it (or the first part); the other parts are found
/// by name when clicked.
pub fn split_artists(artists: Vec<ArtistRef>, linked_name: Option<&str>) -> Vec<ArtistRef> {
    artists
        .into_iter()
        .flat_map(|a| {
            let names = split_credit(&a.name);
            if names.len() == 1 {
                return vec![a];
            }
            let owner = linked_name.and_then(|n| names.iter().position(|p| p.eq_ignore_ascii_case(n))).unwrap_or(0);
            names.into_iter().enumerate().map(|(i, name)| ArtistRef { id: if i == owner { a.id.clone() } else { None }, name }).collect()
        })
        .collect()
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
    /// How often the song was played or viewed on its service, when the service says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listens: Option<u64>,
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
            listens: None,
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
/// An artist's songs, most played first (songs without a count keep their order, at the end).
pub fn most_played(mut tracks: Vec<Track>) -> Vec<Track> {
    tracks.sort_by_key(|t| std::cmp::Reverse(t.listens.unwrap_or(0)));
    tracks
}

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
