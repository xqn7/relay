//! "Other versions" of a song: slowed, sped up, nightcore, remix, live, acoustic and
//! instrumental uploads of the same song on YouTube and SoundCloud, recognised with the
//! matcher's version tags.

use crate::error::AppResult;
use crate::matcher::{clean_title, fold, version_tags};
use crate::sources::{SearchKind, Source, Track};
use crate::AppState;
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};

/// Version kinds in the order they are shown, with the word added to the search.
pub const KINDS: &[(&str, &str)] = &[
    ("slowed", "slowed"),
    ("slowedReverb", "slowed reverb"),
    ("spedUp", "sped up"),
    ("nightcore", "nightcore"),
    ("remix", "remix"),
    ("live", "live"),
    ("acoustic", "acoustic"),
    ("instrumental", "instrumental"),
];
const PER_KIND: usize = 4;
/// Share of the song's own title words (and artist words) a version has to contain.
const MIN_TITLE_OVERLAP: f32 = 0.99;
const MIN_ARTIST_OVERLAP: f32 = 0.5;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub kind: &'static str,
    pub track: Track,
}

/// Which version a title describes, if any of the ones offered.
pub fn kind_of(title: &str) -> Option<&'static str> {
    let tags = version_tags(title);
    let has = |t: &str| tags.contains(t);
    if has("slowed") {
        Some(if has("reverb") { "slowedReverb" } else { "slowed" })
    } else if has("sped up") {
        Some("spedUp")
    } else if has("nightcore") {
        Some("nightcore")
    } else if has("remix") || has("vip") || has("flip") || has("bootleg") {
        Some("remix")
    } else if has("live") {
        Some("live")
    } else if has("acoustic") {
        Some("acoustic")
    } else if has("instrumental") {
        Some("instrumental")
    } else {
        None
    }
}

const TAG_WORDS: &[&str] = &[
    "slowed", "reverb", "sped", "speed", "up", "spedup", "nightcore", "remix", "rmx", "vip", "flip", "bootleg", "live", "acoustic",
    "unplugged", "instrumental",
];

fn words(s: &str) -> BTreeSet<String> {
    fold(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "the" | "a" | "feat" | "ft" | "x" | "and" | "version"))
        .map(str::to_string)
        .collect()
}

/// The song's name without its version words: "Song (Slowed)" and "Song" share "song".
fn base_words(title: &str) -> BTreeSet<String> {
    words(&clean_title(title)).into_iter().filter(|w| !TAG_WORDS.contains(&w.as_str())).collect()
}

fn overlap(want: &BTreeSet<String>, have: &BTreeSet<String>) -> f32 {
    if want.is_empty() {
        return 0.0;
    }
    want.intersection(have).count() as f32 / want.len() as f32
}

/// Search results that are another version of `original`, grouped in `KINDS` order.
pub fn pick(original: &Track, found: &[Track]) -> Vec<Version> {
    let own_kind = kind_of(&original.title);
    let title = base_words(&original.title);
    let artist = words(&original.artist_line());
    let mut seen: HashSet<&str> = HashSet::from([original.id.as_str()]);
    let mut out: Vec<Version> = Vec::new();
    for t in found {
        let Some(kind) = kind_of(&t.title) else { continue };
        if Some(kind) == own_kind || !seen.insert(t.id.as_str()) {
            continue;
        }
        // Uploaders often put the artist into the title, so both fields count for each side.
        let all: BTreeSet<String> = words(&t.title).union(&words(&t.artist_line())).cloned().collect();
        if overlap(&title, &all) < MIN_TITLE_OVERLAP || overlap(&artist, &all) < MIN_ARTIST_OVERLAP {
            continue;
        }
        if out.iter().filter(|v| v.kind == kind).count() < PER_KIND {
            out.push(Version { kind, track: t.clone() });
        }
    }
    out.sort_by_key(|v| KINDS.iter().position(|(k, _)| *k == v.kind));
    out
}

/// Searches YouTube (videos, where fan versions live) for each kind, and SoundCloud once.
/// A failing search only costs its kind; an error comes back only if everything failed.
pub async fn find(state: &AppState, track: &Track) -> AppResult<Vec<Version>> {
    let artist = track.artists.first().map(|a| a.name.as_str()).unwrap_or("");
    // The title as written, minus version words, so a slowed song searches for its siblings.
    let base: String = clean_title(&track.title)
        .split_whitespace()
        .filter(|w| !TAG_WORDS.contains(&fold(w).trim_matches(|c: char| !c.is_alphanumeric())))
        .collect::<Vec<_>>()
        .join(" ");
    let q = |word: &str| format!("{artist} {base} {word}").trim().to_string();
    let yt = |word: &'static str| {
        let query = q(word);
        async move { state.yt.search_songs(&query, true).await }
    };
    let sc_query = format!("{artist} {base}");
    let (a, b, c, d, e, f, g, sc) = tokio::join!(
        yt("slowed reverb"),
        yt("sped up"),
        yt("nightcore"),
        yt("remix"),
        yt("live"),
        yt("acoustic"),
        yt("instrumental"),
        state.sc.search(&sc_query, SearchKind::Songs),
    );
    let mut found = Vec::new();
    let mut first_err = None;
    for r in [a, b, c, d, e, f, g, sc.map(|s| s.tracks)] {
        match r {
            Ok(list) => found.extend(list),
            Err(e) => first_err = first_err.or(Some(e)),
        }
    }
    match first_err {
        Some(e) if found.is_empty() => Err(e),
        _ => Ok(pick(track, &found)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, title: &str, artist: &str) -> Track {
        Track::imported("youtube", id, title, vec![artist.into()], Some(200_000), None, None)
    }

    #[test]
    fn names_the_version() {
        assert_eq!(kind_of("Song (Slowed + Reverb)"), Some("slowedReverb"));
        assert_eq!(kind_of("Song - slowed"), Some("slowed"));
        assert_eq!(kind_of("Song (Sped Up)"), Some("spedUp"));
        assert_eq!(kind_of("Song [Nightcore]"), Some("nightcore"));
        assert_eq!(kind_of("Song (X VIP)"), Some("remix"));
        assert_eq!(kind_of("Song (Live at Wembley)"), Some("live"));
        assert_eq!(kind_of("Song"), None);
        assert_eq!(kind_of("Song (Cover)"), None);
    }

    #[test]
    fn keeps_versions_of_the_same_song_only() {
        let original = t("o", "Blinding Lights", "The Weeknd");
        let found = vec![
            t("1", "The Weeknd - Blinding Lights (slowed + reverb)", "some channel"),
            t("2", "Blinding Lights (Sped Up)", "The Weeknd"),
            t("3", "Save Your Tears (sped up)", "The Weeknd"),
            t("4", "Blinding Lights", "The Weeknd"),
            t("5", "Blinding Lights (Live)", "Other Band"),
            t("2", "Blinding Lights (Sped Up)", "The Weeknd"),
            t("6", "Blinding Lights (Remix)", "The Weeknd"),
        ];
        let picked = pick(&original, &found);
        let got: Vec<(&str, &str)> = picked.iter().map(|v| (v.kind, v.track.source_id.as_str())).collect();
        assert_eq!(got, vec![("slowedReverb", "1"), ("spedUp", "2"), ("remix", "6")]);
    }

    #[test]
    fn a_version_lists_the_original_kind_never() {
        let original = t("o", "Song (Slowed)", "Artist");
        let found = vec![t("1", "Song (slowed)", "Artist"), t("2", "Song (Nightcore)", "Artist")];
        let got: Vec<&str> = pick(&original, &found).iter().map(|v| v.kind).collect();
        assert_eq!(got, vec!["nightcore"]);
    }
}
