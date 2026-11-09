//! Recognizes import links from every supported service. Pure string work, so it is fully
//! unit-tested; short links (spotify.link, deezer.page.link, on.soundcloud.com) are followed
//! by the importer and then parsed here again.

use super::{APPLE, DEEZER, SOUNDCLOUD, SPOTIFY, YOUTUBE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Playlist,
    Album,
    Track,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Playlist => "playlist",
            Kind::Album => "album",
            Kind::Track => "track",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// A link this app can read directly.
    Target { service: &'static str, kind: Kind, id: String, url: String },
    /// A share link that redirects to a real one.
    Short { service: &'static str, url: String },
}

const SHORT_HOSTS: &[(&str, &str)] = &[
    ("spotify.link", SPOTIFY),
    ("spotify.app.link", SPOTIFY),
    ("deezer.page.link", DEEZER),
    ("link.deezer.com", DEEZER),
    ("dzr.page.link", DEEZER),
    ("on.soundcloud.com", SOUNDCLOUD),
];

pub fn parse(input: &str) -> Option<Link> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("spotify:") {
        return parse_spotify_uri(rest);
    }
    let without_scheme = input.strip_prefix("https://").or_else(|| input.strip_prefix("http://")).unwrap_or(input);
    let (host_part, rest) = without_scheme.split_once('/').unwrap_or((without_scheme, ""));
    let lower = host_part.to_lowercase();
    let host = lower.strip_prefix("www.").or_else(|| lower.strip_prefix("m.")).unwrap_or(&lower);
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let path = path.split('#').next().unwrap_or("");
    let query = query.split('#').next().unwrap_or("");
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let url = format!("https://{without_scheme}");

    if let Some((_, service)) = SHORT_HOSTS.iter().find(|(h, _)| *h == host) {
        return (!segs.is_empty()).then_some(Link::Short { service, url });
    }
    match host {
        "open.spotify.com" | "play.spotify.com" => parse_spotify_path(&segs),
        "deezer.com" => parse_deezer(&segs),
        "music.apple.com" | "itunes.apple.com" => parse_apple(&segs, query, &url),
        "youtube.com" | "music.youtube.com" | "youtu.be" => parse_youtube(host, &segs, query),
        "soundcloud.com" => parse_soundcloud(&segs),
        _ => None,
    }
}

fn target(service: &'static str, kind: Kind, id: &str, url: String) -> Option<Link> {
    Some(Link::Target { service, kind, id: id.to_string(), url })
}

fn kind_of(s: &str) -> Option<Kind> {
    match s {
        "playlist" => Some(Kind::Playlist),
        "album" => Some(Kind::Album),
        "track" => Some(Kind::Track),
        _ => None,
    }
}

fn is_spotify_id(id: &str) -> bool {
    id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

fn spotify_target(kind: Kind, id: &str) -> Option<Link> {
    if !is_spotify_id(id) {
        return None;
    }
    target(SPOTIFY, kind, id, format!("https://open.spotify.com/{}/{id}", kind.as_str()))
}

fn parse_spotify_uri(rest: &str) -> Option<Link> {
    // spotify:playlist:ID or the old spotify:user:NAME:playlist:ID
    let parts: Vec<&str> = rest.split(':').collect();
    match parts.as_slice() {
        [kind, id] => spotify_target(kind_of(kind)?, id),
        ["user", _, kind, id] => spotify_target(kind_of(kind)?, id),
        _ => None,
    }
}

fn parse_spotify_path(segs: &[&str]) -> Option<Link> {
    // Skips "intl-de", "embed" and the old "user/<name>" prefixes.
    let mut i = 0;
    while let Some(s) = segs.get(i) {
        if s.starts_with("intl-") || *s == "embed" {
            i += 1;
        } else if *s == "user" {
            i += 2;
        } else {
            break;
        }
    }
    spotify_target(kind_of(segs.get(i)?)?, segs.get(i + 1)?)
}

fn parse_deezer(segs: &[&str]) -> Option<Link> {
    // deezer.com/en/playlist/123 or deezer.com/playlist/123
    let i = segs.iter().position(|s| kind_of(s).is_some())?;
    let kind = kind_of(segs[i])?;
    let id = segs.get(i + 1)?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    target(DEEZER, kind, id, format!("https://www.deezer.com/{}/{id}", kind.as_str()))
}

fn parse_apple(segs: &[&str], query: &str, url: &str) -> Option<Link> {
    // music.apple.com/<storefront>/<kind>/<slug>/<id>[?i=<song id>]; the slug is optional.
    let kind_at = segs.iter().position(|s| matches!(*s, "playlist" | "album" | "song"))?;
    let id = *segs.last()?;
    if kind_at + 1 >= segs.len() {
        return None;
    }
    let song_in_album = query_value(query, "i").filter(|s| s.chars().all(|c| c.is_ascii_digit()));
    let clean = url.split(['?', '#']).next().unwrap_or(url);
    match (segs[kind_at], song_in_album) {
        ("album", Some(song)) => target(APPLE, Kind::Track, song, format!("{clean}?i={song}")),
        ("album", None) => target(APPLE, Kind::Album, id, clean.to_string()),
        ("song", _) => target(APPLE, Kind::Track, id, clean.to_string()),
        ("playlist", _) if id.starts_with("pl.") => target(APPLE, Kind::Playlist, id, clean.to_string()),
        _ => None,
    }
}

fn query_value<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|p| p.split_once('=').filter(|(k, _)| *k == name).map(|(_, v)| v)).filter(|v| !v.is_empty())
}

fn is_yt_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn parse_youtube(host: &str, segs: &[&str], query: &str) -> Option<Link> {
    if let Some(list) = query_value(query, "list").filter(|l| is_yt_id(l)) {
        // Auto-generated mixes ("RD…") are endless radios, not lists anyone can import.
        if !list.starts_with("RD") {
            return target(YOUTUBE, Kind::Playlist, list, format!("https://www.youtube.com/playlist?list={list}"));
        }
    }
    let video = if host == "youtu.be" {
        segs.first().copied()
    } else {
        match segs.first().copied() {
            Some("watch") => query_value(query, "v"),
            Some("shorts") | Some("live") => segs.get(1).copied(),
            Some("browse") => {
                let id = segs.get(1).filter(|id| id.starts_with("MPREb_") && is_yt_id(id))?;
                return target(YOUTUBE, Kind::Album, id, format!("https://music.youtube.com/browse/{id}"));
            }
            _ => None,
        }
    }?;
    if video.len() != 11 || !is_yt_id(video) {
        return None;
    }
    target(YOUTUBE, Kind::Track, video, format!("https://music.youtube.com/watch?v={video}"))
}

fn parse_soundcloud(segs: &[&str]) -> Option<Link> {
    const NOT_USERS: &[&str] = &["discover", "search", "you", "charts", "stations", "feed", "upload", "settings", "pages"];
    const NOT_TRACKS: &[&str] = &["tracks", "albums", "sets", "likes", "reposts", "popular-tracks", "followers", "following"];
    let user = segs.first().filter(|u| !NOT_USERS.contains(u))?;
    let canonical = |p: &str| format!("https://soundcloud.com/{p}");
    match segs {
        [_, "sets", name, ..] => {
            let path = format!("{user}/sets/{name}");
            target(SOUNDCLOUD, Kind::Playlist, &path, canonical(&path))
        }
        [_, track] if !NOT_TRACKS.contains(track) => {
            let path = format!("{user}/{track}");
            target(SOUNDCLOUD, Kind::Track, &path, canonical(&path))
        }
        _ => None,
    }
}

/// Finds the first link to a supported service inside a page (short-link landing pages
/// sometimes only redirect with JavaScript).
pub fn find_in_html(html: &str, service: &str) -> Option<Link> {
    let prefixes: &[&str] = match service {
        SPOTIFY => &["https://open.spotify.com/"],
        DEEZER => &["https://www.deezer.com/", "https://deezer.com/"],
        SOUNDCLOUD => &["https://soundcloud.com/"],
        _ => return None,
    };
    for prefix in prefixes {
        for part in html.split(prefix).skip(1) {
            let end = part.find(|c: char| matches!(c, '"' | '\'' | '<' | '\\') || c.is_whitespace()).unwrap_or(part.len());
            if let Some(link @ Link::Target { .. }) = parse(&format!("{prefix}{}", &part[..end])) {
                return Some(link);
            }
        }
    }
    None
}

/// Spotify track ids from pasted text: links (one per line, as the desktop app copies them)
/// and spotify:track: URIs. Duplicates are dropped, order is kept.
pub fn spotify_track_ids(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter_map(|tok| match parse(tok) {
            Some(Link::Target { service: SPOTIFY, kind: Kind::Track, id, .. }) => Some(id),
            _ => None,
        })
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(input: &str) -> Option<(&'static str, Kind, String)> {
        match parse(input)? {
            Link::Target { service, kind, id, .. } => Some((service, kind, id)),
            Link::Short { .. } => None,
        }
    }

    #[test]
    fn spotify_links() {
        let id = "37i9dQZF1DXcBWIGoYBM5M";
        assert_eq!(t(&format!("https://open.spotify.com/playlist/{id}?si=abc")), Some((SPOTIFY, Kind::Playlist, id.into())));
        assert_eq!(t(&format!("https://open.spotify.com/intl-de/album/{id}")), Some((SPOTIFY, Kind::Album, id.into())));
        assert_eq!(t(&format!("open.spotify.com/embed/track/{id}")), Some((SPOTIFY, Kind::Track, id.into())));
        assert_eq!(t(&format!("https://open.spotify.com/user/someone/playlist/{id}")), Some((SPOTIFY, Kind::Playlist, id.into())));
        assert_eq!(t(&format!("spotify:playlist:{id}")), Some((SPOTIFY, Kind::Playlist, id.into())));
        assert_eq!(t(&format!("spotify:user:x:playlist:{id}")), Some((SPOTIFY, Kind::Playlist, id.into())));
        assert_eq!(t("https://open.spotify.com/playlist/short"), None);
        assert!(matches!(parse("https://spotify.link/AbCdEf"), Some(Link::Short { service: SPOTIFY, .. })));
    }

    #[test]
    fn deezer_links() {
        assert_eq!(t("https://www.deezer.com/en/playlist/908622995"), Some((DEEZER, Kind::Playlist, "908622995".into())));
        assert_eq!(t("https://deezer.com/album/302127"), Some((DEEZER, Kind::Album, "302127".into())));
        assert_eq!(t("https://www.deezer.com/track/3135556?x=1"), Some((DEEZER, Kind::Track, "3135556".into())));
        assert_eq!(t("https://www.deezer.com/playlist/abc"), None);
        assert!(matches!(parse("https://deezer.page.link/xyz"), Some(Link::Short { service: DEEZER, .. })));
        assert!(matches!(parse("https://link.deezer.com/s/xyz"), Some(Link::Short { service: DEEZER, .. })));
    }

    #[test]
    fn apple_links() {
        assert_eq!(
            t("https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb"),
            Some((APPLE, Kind::Playlist, "pl.f4d106fed2bd41149aaacabb233eb5eb".into()))
        );
        assert_eq!(t("https://music.apple.com/us/album/choosin-texas/1844932149"), Some((APPLE, Kind::Album, "1844932149".into())));
        assert_eq!(t("https://music.apple.com/us/album/choosin-texas/1844932149?i=1844932150"), Some((APPLE, Kind::Track, "1844932150".into())));
        assert_eq!(t("https://music.apple.com/de/song/x/1844932150"), Some((APPLE, Kind::Track, "1844932150".into())));
    }

    #[test]
    fn youtube_links() {
        let pl = "PLx0sYbCqOb8TBPRdmBHs5Iftvv9TPboYG";
        assert_eq!(t(&format!("https://www.youtube.com/playlist?list={pl}")), Some((YOUTUBE, Kind::Playlist, pl.into())));
        assert_eq!(t("https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=OLAK5uy_abc"), Some((YOUTUBE, Kind::Playlist, "OLAK5uy_abc".into())));
        assert_eq!(t("https://youtu.be/dQw4w9WgXcQ"), Some((YOUTUBE, Kind::Track, "dQw4w9WgXcQ".into())));
        assert_eq!(t("https://m.youtube.com/watch?v=dQw4w9WgXcQ&list=RDdQw4w9WgXcQ"), Some((YOUTUBE, Kind::Track, "dQw4w9WgXcQ".into())));
        assert_eq!(t("https://music.youtube.com/browse/MPREb_abc123"), Some((YOUTUBE, Kind::Album, "MPREb_abc123".into())));
        assert_eq!(t("https://www.youtube.com/@channel"), None);
    }

    #[test]
    fn soundcloud_links() {
        assert_eq!(t("https://soundcloud.com/artist/sets/my-set?si=1"), Some((SOUNDCLOUD, Kind::Playlist, "artist/sets/my-set".into())));
        assert_eq!(t("https://m.soundcloud.com/artist/song"), Some((SOUNDCLOUD, Kind::Track, "artist/song".into())));
        assert_eq!(t("https://soundcloud.com/artist/likes"), None);
        assert_eq!(t("https://soundcloud.com/discover/sets/x"), None);
        assert!(matches!(parse("https://on.soundcloud.com/AbC"), Some(Link::Short { service: SOUNDCLOUD, .. })));
    }

    #[test]
    fn unknown_links() {
        assert_eq!(parse("hello"), None);
        assert_eq!(parse("https://example.com/playlist/1"), None);
    }

    #[test]
    fn finds_link_in_landing_page() {
        let html = r#"<a href="https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=x">open</a>"#;
        assert!(matches!(find_in_html(html, SPOTIFY), Some(Link::Target { kind: Kind::Playlist, .. })));
    }

    #[test]
    fn reads_pasted_liked_songs() {
        let text = "https://open.spotify.com/track/3zOYmiNOuyDL0ClxJcEJ0T\r\nhttps://open.spotify.com/track/6OmhkSOpvYBokMKQxpIGx2\nspotify:track:3zOYmiNOuyDL0ClxJcEJ0T\nnot a link";
        assert_eq!(spotify_track_ids(text), vec!["3zOYmiNOuyDL0ClxJcEJ0T", "6OmhkSOpvYBokMKQxpIGx2"]);
    }
}
