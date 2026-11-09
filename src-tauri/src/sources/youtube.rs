//! YouTube / YouTube Music. The catalog (search, albums, radio) comes from rustypipe's InnerTube
//! client; audio streams come from yt-dlp, which keeps up with YouTube's player changes
//! (signatures, PO tokens) much faster. See FIXING.md.

use super::*;
use crate::error::{AppError, AppResult};
use crate::tools::Tools;
use rustypipe::client::RustyPipe;
use rustypipe::error::{Error as RpError, ExtractionError};
use rustypipe::model::{AlbumItem, ArtistItem, MusicItem, MusicPlaylistItem, Thumbnail, TrackItem, TrackType};
use std::path::Path;
use std::sync::Arc;

pub struct YouTube {
    rp: RustyPipe,
    tools: Arc<Tools>,
}

impl YouTube {
    pub fn new(storage_dir: &Path, tools: Arc<Tools>) -> AppResult<Self> {
        tools.warm_up();
        let rp = RustyPipe::builder()
            .storage_dir(storage_dir)
            .no_botguard()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(map_err)?;
        Ok(Self { rp, tools })
    }
}

fn map_err(e: RpError) -> AppError {
    let code = match &e {
        RpError::Extraction(ExtractionError::NotFound { .. }) => "not_found",
        RpError::Extraction(ExtractionError::Unavailable { .. }) => "unavailable",
        RpError::HttpStatus(429, _) => "rate_limited",
        RpError::Http(_) => "network",
        _ => "source_failed",
    };
    AppError::new(code, format!("YouTube: {e}"))
}

/// Largest thumbnail up to ~600px wide: sharp enough for the big player, small enough to load fast.
fn best_thumb(thumbs: &[Thumbnail]) -> Option<String> {
    thumbs
        .iter()
        .filter(|t| t.width <= 600 && t.url.starts_with("https://"))
        .max_by_key(|t| t.width)
        .or_else(|| thumbs.iter().find(|t| t.url.starts_with("https://")))
        .map(|t| t.url.clone())
}

fn map_track(t: TrackItem) -> Track {
    let artists = t.artists.into_iter().map(|a| ArtistRef { id: a.id, name: a.name }).collect();
    Track {
        id: Track::make_id(YOUTUBE, &t.id),
        link: Some(format!("https://music.youtube.com/watch?v={}", t.id)),
        source: YOUTUBE.into(),
        title: t.name,
        artists,
        album: t.album.map(|a| AlbumRef { id: a.id, name: a.name }),
        duration_ms: t.duration.map(|d| d as u64 * 1000),
        cover: best_thumb(&t.cover),
        is_video: t.track_type != TrackType::Track,
        source_id: t.id,
    }
}

fn map_album(a: AlbumItem) -> Collection {
    let artists = a.artists.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ");
    let subtitle = match a.year {
        Some(y) => format!("{artists} · {y}"),
        None => artists,
    };
    Collection { source: YOUTUBE.into(), kind: "album".into(), cover: best_thumb(&a.cover), id: a.id, title: a.name, subtitle }
}

fn map_playlist(p: MusicPlaylistItem) -> Collection {
    Collection {
        source: YOUTUBE.into(),
        kind: "playlist".into(),
        cover: best_thumb(&p.thumbnail),
        subtitle: p.channel.map(|c| c.name).unwrap_or_default(),
        id: p.id,
        title: p.name,
    }
}

fn map_artist(a: ArtistItem) -> ArtistCard {
    ArtistCard { source: YOUTUBE.into(), avatar: best_thumb(&a.avatar), id: a.id, name: a.name }
}

impl Source for YouTube {
    async fn search(&self, query: &str, kind: SearchKind) -> AppResult<SearchResults> {
        let q = self.rp.query();
        let mut out = SearchResults { source: YOUTUBE.into(), ..Default::default() };
        match kind {
            SearchKind::All => {
                // The mixed search returns only a few songs, so fetch songs separately in parallel.
                let (main, songs) = tokio::join!(q.music_search_main(query), q.music_search_tracks(query));
                for item in main.map_err(map_err)?.items.items {
                    match item {
                        MusicItem::Album(a) => out.albums.push(map_album(a)),
                        MusicItem::Artist(a) => out.artists.push(map_artist(a)),
                        MusicItem::Playlist(p) => out.playlists.push(map_playlist(p)),
                        MusicItem::Track(t) if songs.is_err() => out.tracks.push(map_track(t)),
                        _ => {}
                    }
                }
                if let Ok(songs) = songs {
                    out.tracks = songs.items.items.into_iter().map(map_track).collect();
                }
            }
            SearchKind::Songs => {
                out.tracks = q.music_search_tracks(query).await.map_err(map_err)?.items.items.into_iter().map(map_track).collect();
            }
            SearchKind::Albums => {
                out.albums = q.music_search_albums(query).await.map_err(map_err)?.items.items.into_iter().map(map_album).collect();
            }
            SearchKind::Artists => {
                out.artists = q.music_search_artists(query).await.map_err(map_err)?.items.items.into_iter().map(map_artist).collect();
            }
            SearchKind::Playlists => {
                out.playlists =
                    q.music_search_playlists(query, false).await.map_err(map_err)?.items.items.into_iter().map(map_playlist).collect();
            }
        }
        out.tracks.retain(Track::is_sane);
        Ok(out)
    }

    async fn resolve(&self, track: &Track, quality: Quality) -> AppResult<ResolvedStream> {
        self.tools.ytdlp_stream(&track.source_id, quality).await
    }

    async fn collection_tracks(&self, kind: &str, id: &str) -> AppResult<Vec<Track>> {
        let q = self.rp.query();
        let tracks = match kind {
            "album" => {
                let album = q.music_album(id).await.map_err(map_err)?;
                let cover = best_thumb(&album.cover);
                let mut tracks: Vec<Track> = album.tracks.into_iter().map(map_track).collect();
                // Album track lists omit per-track covers; reuse the album cover.
                for t in tracks.iter_mut().filter(|t| t.cover.is_none()) {
                    t.cover = cover.clone();
                }
                tracks
            }
            "playlist" => q.music_playlist(id).await.map_err(map_err)?.tracks.items.into_iter().map(map_track).collect(),
            _ => return Err(AppError::new("invalid_input", "unknown collection kind")),
        };
        Ok(tracks.into_iter().filter(Track::is_sane).collect())
    }
}

impl YouTube {
    /// Songs YouTube Music would play after this one ("radio"), used for autoplay.
    pub async fn radio(&self, video_id: &str) -> AppResult<Vec<Track>> {
        let radio = self.rp.query().music_radio_track(video_id).await.map_err(map_err)?;
        Ok(radio.items.into_iter().map(map_track).filter(|t| t.is_sane() && t.source_id != video_id).collect())
    }
}
