//! YouTube / YouTube Music. The catalog (search, albums, radio) comes from rustypipe's InnerTube
//! client; audio streams come from yt-dlp, which keeps up with YouTube's player changes
//! (signatures, PO tokens) much faster. See FIXING.md.

use super::*;
use crate::error::{AppError, AppResult};
use crate::tools::Tools;
use rustypipe::client::{RustyPipe, RustyPipeQuery};
use rustypipe::error::{Error as RpError, ExtractionError};
use rustypipe::model::{AlbumItem, AlbumType, ArtistItem, MusicItem, MusicPlaylistItem, Thumbnail, TrackItem, TrackType};
use std::path::Path;
use std::sync::Arc;

pub struct YouTube {
    rp: RustyPipe,
    tools: Arc<Tools>,
    http: Arc<crate::http::Http>,
    /// YouTube's visitor id for anonymous requests, and when it was fetched.
    visitor: tokio::sync::Mutex<Option<(String, std::time::Instant)>>,
}

/// rustypipe fetches the visitor id from music.youtube.com, which some countries and networks
/// answer with "not available in your area" — and rustypipe then crashes the request instead
/// of failing it. youtube.com hands out the same id everywhere, so Relay fetches it there.
const VISITOR_URL: &str = "https://www.youtube.com/";
/// Skips YouTube's cookie consent page (EU), as rustypipe does.
const CONSENT_COOKIE: &str = "SOCS=CAISAiAD";
/// A visitor id stays valid for a long time; a fresh one now and then looks like a normal visitor.
const VISITOR_FRESH: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

/// Visitor ids are a few hundred characters (446 in November 2026); anything far longer isn't one.
const MAX_VISITOR_LEN: usize = 2000;

/// The visitor id from a youtube.com page: `"visitorData":"<id>"`.
pub(crate) fn visitor_data_from(html: &str) -> Option<String> {
    const KEY: &str = "\"visitorData\":\"";
    let start = html.find(KEY)? + KEY.len();
    let id = &html[start..start + html[start..].find('"')?];
    (!id.is_empty() && id.len() <= MAX_VISITOR_LEN && id.chars().all(|c| c.is_ascii_alphanumeric() || "_-%".contains(c))).then(|| id.to_string())
}

impl YouTube {
    pub fn new(storage_dir: &Path, tools: Arc<Tools>, http: Arc<crate::http::Http>) -> AppResult<Self> {
        tools.warm_up();
        let rp = RustyPipe::builder()
            .storage_dir(storage_dir)
            .no_botguard()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(map_err)?;
        Ok(Self { rp, tools, http, visitor: tokio::sync::Mutex::new(None) })
    }

    /// A query with Relay's own visitor id, so rustypipe never asks music.youtube.com for one.
    async fn query(&self) -> RustyPipeQuery {
        let q = self.rp.query();
        match self.visitor_data().await {
            Some(id) => q.visitor_data(id),
            None => q,
        }
    }

    async fn visitor_data(&self) -> Option<String> {
        let mut cached = self.visitor.lock().await;
        if let Some((id, at)) = cached.as_ref() {
            if at.elapsed() < VISITOR_FRESH {
                return Some(id.clone());
            }
        }
        let fetched = async {
            let resp = self.http.send(VISITOR_URL, |c| c.get(VISITOR_URL).header(reqwest::header::COOKIE, CONSENT_COOKIE)).await?;
            crate::http::check_status(&resp)?;
            visitor_data_from(&resp.text().await?).ok_or_else(|| AppError::new("bad_response", "no visitor id on youtube.com"))
        };
        match fetched.await {
            Ok(id) => {
                *cached = Some((id.clone(), std::time::Instant::now()));
                Some(id)
            }
            Err(e) => {
                eprintln!("YouTube visitor id unavailable: {e}");
                // An older id still works better than letting rustypipe fetch its own.
                cached.as_ref().map(|(id, _)| id.clone())
            }
        }
    }
}

/// Runs a rustypipe call. Besides its errors, rustypipe sometimes crashes (an `unwrap` on data
/// YouTube changed); that becomes an error too, so the song falls back to SoundCloud and the
/// page gets an answer instead of waiting forever. The crash itself is still logged and
/// reported by the panic hook.
pub(crate) async fn rp<T>(call: impl std::future::Future<Output = Result<T, RpError>>) -> AppResult<T> {
    let mut call = std::pin::pin!(call);
    std::future::poll_fn(|cx| match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call.as_mut().poll(cx))) {
        Ok(std::task::Poll::Ready(result)) => std::task::Poll::Ready(result.map_err(map_err)),
        Ok(std::task::Poll::Pending) => std::task::Poll::Pending,
        Err(_) => std::task::Poll::Ready(Err(AppError::new("source_failed", "YouTube: the YouTube library crashed"))),
    })
    .await
}

pub(crate) fn map_err(e: RpError) -> AppError {
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

pub(crate) fn map_track(t: TrackItem) -> Track {
    let artists = t
        .artists
        .into_iter()
        .flat_map(|a| match a.id {
            Some(id) => vec![ArtistRef { id: Some(id), name: a.name }],
            None => split_credit(&a.name).into_iter().map(|name| ArtistRef { id: None, name }).collect(),
        })
        .collect();
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
        listens: t.view_count,
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

pub(crate) fn map_playlist(p: MusicPlaylistItem) -> Collection {
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
        let q = self.query().await;
        let mut out = SearchResults { source: YOUTUBE.into(), ..Default::default() };
        match kind {
            SearchKind::All => {
                // The mixed search returns only a few songs, so fetch songs separately in parallel.
                let (main, songs) = tokio::join!(rp(q.music_search_main(query)), rp(q.music_search_tracks(query)));
                for item in main?.items.items {
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
                out.tracks = rp(q.music_search_tracks(query)).await?.items.items.into_iter().map(map_track).collect();
            }
            SearchKind::Albums => {
                out.albums = rp(q.music_search_albums(query)).await?.items.items.into_iter().map(map_album).collect();
            }
            SearchKind::Artists => {
                out.artists = rp(q.music_search_artists(query)).await?.items.items.into_iter().map(map_artist).collect();
            }
            SearchKind::Playlists => {
                out.playlists =
                    rp(q.music_search_playlists(query, false)).await?.items.items.into_iter().map(map_playlist).collect();
            }
        }
        out.tracks.retain(Track::is_sane);
        Ok(out)
    }

    async fn resolve(&self, track: &Track, quality: Quality) -> AppResult<ResolvedStream> {
        self.tools.ytdlp_stream(&track.source_id, quality).await
    }

    async fn collection_tracks(&self, kind: &str, id: &str) -> AppResult<Vec<Track>> {
        let q = self.query().await;
        let tracks = match kind {
            "album" => {
                let album = rp(q.music_album(id)).await?;
                let cover = best_thumb(&album.cover);
                let mut tracks: Vec<Track> = album.tracks.into_iter().map(map_track).collect();
                // Album track lists omit per-track covers; reuse the album cover.
                for t in tracks.iter_mut().filter(|t| t.cover.is_none()) {
                    t.cover = cover.clone();
                }
                tracks
            }
            "playlist" => rp(q.music_playlist(id)).await?.tracks.items.into_iter().map(map_track).collect(),
            _ => return Err(AppError::new("invalid_input", "unknown collection kind")),
        };
        Ok(tracks.into_iter().filter(Track::is_sane).collect())
    }
}

impl YouTube {
    /// Songs YouTube Music would play after this one ("radio"), used for autoplay.
    pub async fn radio(&self, video_id: &str) -> AppResult<Vec<Track>> {
        let radio = rp(self.query().await.music_radio_track(video_id)).await?;
        Ok(radio.items.into_iter().map(map_track).filter(|t| t.is_sane() && t.source_id != video_id).collect())
    }
}

impl YouTube {
    /// YouTube Music's own lyrics for a song (plain text, no timing), with the credit line
    /// YouTube shows under them. Ok(None) when YouTube has no lyrics for it.
    pub async fn lyrics(&self, video_id: &str) -> AppResult<Option<(String, String)>> {
        let q = self.query().await;
        let details = rp(q.music_details(video_id)).await?;
        let Some(lyrics_id) = details.lyrics_id else { return Ok(None) };
        match q.music_lyrics(&lyrics_id).await {
            Ok(l) if !l.body.trim().is_empty() => Ok(Some((l.body, l.footer))),
            Ok(_) | Err(RpError::Extraction(ExtractionError::NotFound { .. })) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    /// Song search on YouTube Music. With `videos`, searches music videos instead, which
    /// the matcher only uses when no plain song matched.
    pub async fn search_songs(&self, query: &str, videos: bool) -> AppResult<Vec<Track>> {
        let q = self.query().await;
        let items = if videos {
            rp(q.music_search_videos(query)).await?.items.items
        } else {
            rp(q.music_search_tracks(query)).await?.items.items
        };
        Ok(items.into_iter().map(map_track).filter(Track::is_sane).collect())
    }

    /// Reads a playlist, album or song for import. Deleted and private videos are left out
    /// by YouTube; the difference to the stated song count is reported as skipped.
    pub async fn import(&self, kind: &str, id: &str) -> AppResult<RemoteList> {
        let q = self.query().await;
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
                let mut pl = rp(q.music_playlist(id)).await?;
                rp(pl.tracks.extend_limit(&q, MAX_PLAYLIST)).await?;
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
                let details = rp(q.music_details(id)).await?;
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
        let album = rp(self.query().await.music_album(id)).await?;
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
        let a = rp(self.query().await.music_artist(id, false)).await?;
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
            top_tracks: most_played(a.tracks.into_iter().map(map_track).filter(Track::is_sane).collect()),
            albums: by_year(albums.into_iter().map(map_album).collect()),
            singles: by_year(singles.into_iter().map(map_album).collect()),
            similar: a.similar_artists.into_iter().map(map_artist).collect(),
            id: a.id,
            name: a.name,
        })
    }

    /// The YouTube Music artist for a name, for songs whose artist has no id (imports).
    pub async fn find_artist(&self, name: &str) -> AppResult<Option<ArtistCard>> {
        let found = rp(self.query().await.music_search_artists(name)).await?.items.items;
        let wanted = crate::matcher::fold(name);
        Ok(found.into_iter().map(map_artist).find(|a| crate::matcher::fold(&a.name) == wanted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crashing_call_becomes_an_error() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let crashed: AppResult<u8> = rt.block_on(rp(async { panic!("visitor data missing") }));
        assert_eq!(crashed.unwrap_err().code, "source_failed");
        let fine: AppResult<u8> = rt.block_on(rp(async { Ok(7) }));
        assert_eq!(fine.unwrap(), 7);
    }

    #[test]
    fn reads_the_visitor_id_from_the_page() {
        let page = r#"...,"visitorData":"CgtGandpczcxU0t%3D%3D","other":1..."#;
        assert_eq!(visitor_data_from(page).as_deref(), Some("CgtGandpczcxU0t%3D%3D"));
        assert_eq!(visitor_data_from("YouTube Music is not available in your area"), None);
        assert_eq!(visitor_data_from(r#""visitorData":"bad id<script>""#), None);
        let real_length = "C".repeat(446);
        assert_eq!(visitor_data_from(&format!(r#""visitorData":"{real_length}""#)).map(|v| v.len()), Some(446));
    }
}
