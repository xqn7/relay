//! YouTube / YouTube Music. The catalog (search, albums, radio) comes from rustypipe's InnerTube
//! client; audio streams come from yt-dlp, which keeps up with YouTube's player changes
//! (signatures, PO tokens) much faster. See FIXING.md.

use super::*;
use crate::error::{AppError, AppResult};
use crate::tools::Tools;
use rustypipe::client::RustyPipe;
use rustypipe::error::{Error as RpError, ExtractionError};
use rustypipe::model::{AlbumItem, AlbumType, ArtistItem, MusicItem, MusicPlaylistItem, Thumbnail, TrackItem, TrackType};
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

/// Most songs a YouTube playlist import reads (YouTube itself caps playlists at 5,000).
const MAX_PLAYLIST: usize = 5_000;

fn album_kind(t: AlbumType) -> &'static str {
    match t {
        AlbumType::Single => "single",
        AlbumType::Ep => "ep",
        _ => "album",
    }
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
        via: None,
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
    Collection { source: YOUTUBE.into(), kind: "album".into(), cover: best_thumb(&a.cover), year: a.year, id: a.id, title: a.name, subtitle }
}

fn map_playlist(p: MusicPlaylistItem) -> Collection {
    Collection {
        source: YOUTUBE.into(),
        kind: "playlist".into(),
        cover: best_thumb(&p.thumbnail),
        subtitle: p.channel.map(|c| c.name).unwrap_or_default(),
        year: None,
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

impl YouTube {
    /// Song search on YouTube Music. With `videos`, searches music videos instead, which
    /// the matcher only uses when no plain song matched.
    /// YouTube Music's own lyrics for a song (plain text, no timing), with the credit line
    /// YouTube shows under them. Ok(None) when YouTube has no lyrics for it.
    pub async fn lyrics(&self, video_id: &str) -> AppResult<Option<(String, String)>> {
        let q = self.rp.query();
        let details = q.music_details(video_id).await.map_err(map_err)?;
        let Some(lyrics_id) = details.lyrics_id else { return Ok(None) };
        match q.music_lyrics(&lyrics_id).await {
            Ok(l) if !l.body.trim().is_empty() => Ok(Some((l.body, l.footer))),
            Ok(_) | Err(RpError::Extraction(ExtractionError::NotFound { .. })) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    pub async fn search_songs(&self, query: &str, videos: bool) -> AppResult<Vec<Track>> {
        let q = self.rp.query();
        let items = if videos {
            q.music_search_videos(query).await.map_err(map_err)?.items.items
        } else {
            q.music_search_tracks(query).await.map_err(map_err)?.items.items
        };
        Ok(items.into_iter().map(map_track).filter(Track::is_sane).collect())
    }

    /// Reads a playlist, album or song for import. Deleted and private videos are left out
    /// by YouTube; the difference to the stated song count is reported as skipped.
    pub async fn import(&self, kind: &str, id: &str) -> AppResult<RemoteList> {
        let q = self.rp.query();
        let mut list = RemoteList {
            origin: format!("{YOUTUBE}:{kind}:{id}"),
            service: YOUTUBE.into(),
            kind: kind.into(),
            title: String::new(),
            owner: String::new(),
            cover: None,
            tracks: Vec::new(),
            skipped: Vec::new(),
            truncated: false,
        };
        match kind {
            "playlist" => {
                let mut pl = q.music_playlist(id).await.map_err(map_err)?;
                pl.tracks.extend_limit(&q, MAX_PLAYLIST).await.map_err(map_err)?;
                list.title = pl.name;
                list.owner = pl.channel.map(|c| c.name).unwrap_or_default();
                list.cover = best_thumb(&pl.thumbnail);
                list.tracks = pl.tracks.items.into_iter().map(map_track).filter(Track::is_sane).collect();
                let missing = pl.track_count.map(|n| n as usize).unwrap_or(0).saturating_sub(list.tracks.len());
                list.truncated = list.tracks.len() >= MAX_PLAYLIST;
                if !list.truncated {
                    list.skipped = (0..missing).map(|_| Skipped { title: String::new(), artist: String::new(), reason: "deleted".into() }).collect();
                }
            }
            "album" => {
                let album = self.album_page(id).await?;
                list.title = album.title;
                list.owner = album.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
                list.cover = album.cover;
                list.tracks = album.tracks;
            }
            "track" => {
                let details = q.music_details(id).await.map_err(map_err)?;
                let track = map_track(details.track);
                list.title = track.title.clone();
                list.owner = track.artist_line();
                list.cover = track.cover.clone();
                list.tracks = vec![track].into_iter().filter(Track::is_sane).collect();
            }
            _ => return Err(AppError::new("invalid_input", "unknown YouTube link")),
        }
        Ok(list)
    }

    pub async fn album_page(&self, id: &str) -> AppResult<AlbumPage> {
        let album = self.rp.query().music_album(id).await.map_err(map_err)?;
        let cover = best_thumb(&album.cover);
        let mut tracks: Vec<Track> = album.tracks.into_iter().map(map_track).filter(Track::is_sane).collect();
        // Album track lists omit per-track covers and album links; fill them in.
        for t in tracks.iter_mut() {
            if t.cover.is_none() {
                t.cover = cover.clone();
            }
            if t.album.is_none() {
                t.album = Some(AlbumRef { id: album.id.clone(), name: album.name.clone() });
            }
        }
        Ok(AlbumPage {
            source: YOUTUBE.into(),
            link: Some(format!("https://music.youtube.com/browse/{}", album.id)),
            id: album.id,
            title: album.name,
            artists: album.artists.into_iter().map(|a| ArtistRef { id: a.id, name: a.name }).collect(),
            cover,
            year: album.year,
            kind: album_kind(album.album_type).into(),
            tracks,
        })
    }

    pub async fn artist_page(&self, id: &str) -> AppResult<ArtistPage> {
        let a = self.rp.query().music_artist(id, false).await.map_err(map_err)?;
        let (singles, albums): (Vec<AlbumItem>, Vec<AlbumItem>) = a.albums.into_iter().partition(|x| x.album_type == AlbumType::Single);
        let by_year = |mut v: Vec<Collection>| {
            v.sort_by_key(|c| std::cmp::Reverse(c.year));
            v
        };
        Ok(ArtistPage {
            source: YOUTUBE.into(),
            link: Some(format!("https://music.youtube.com/channel/{}", a.id)),
            avatar: best_thumb(&a.header_image),
            banner: a.header_image.iter().filter(|t| t.url.starts_with("https://")).max_by_key(|t| t.width).map(|t| t.url.clone()),
            about: a.description.filter(|d| !d.trim().is_empty()),
            top_tracks: a.tracks.into_iter().map(map_track).filter(Track::is_sane).collect(),
            albums: by_year(albums.into_iter().map(map_album).collect()),
            singles: by_year(singles.into_iter().map(map_album).collect()),
            similar: a.similar_artists.into_iter().map(map_artist).collect(),
            id: a.id,
            name: a.name,
        })
    }

    /// The YouTube Music artist for a name, for songs whose artist has no id (imports).
    pub async fn find_artist(&self, name: &str) -> AppResult<Option<ArtistCard>> {
        let found = self.rp.query().music_search_artists(name).await.map_err(map_err)?.items.items;
        let wanted = crate::matcher::fold(name);
        Ok(found.into_iter().map(map_artist).find(|a| crate::matcher::fold(&a.name) == wanted))
    }
}
