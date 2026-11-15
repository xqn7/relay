//! Decides whether a search result is the same recording as a target track.
//! Used for stand-ins now and for importing playlists later.

use crate::sources::Track;
use std::collections::BTreeSet;

const DURATION_TOLERANCE_MS: u64 = 3_000;
const MIN_SCORE: f32 = 0.6;

/// Version markers that change what a recording sounds like. A version only ever matches
/// the same version: "slowed" is never a stand-in for the original.
const VERSION_TAGS: &[(&str, &[&str])] = &[
    ("remix", &["remix", "rmx"]),
    ("vip", &["vip"]),
    ("flip", &["flip"]),
    ("bootleg", &["bootleg"]),
    ("mashup", &["mashup", "mash up"]),
    ("edit", &["edit"]),
    ("slowed", &["slowed"]),
    ("reverb", &["reverb"]),
    ("sped up", &["sped up", "speed up", "spedup"]),
    ("nightcore", &["nightcore"]),
    ("instrumental", &["instrumental"]),
    ("acoustic", &["acoustic", "unplugged"]),
    ("live", &["live"]),
    ("cover", &["cover", "originally performed", "in the style of", "tribute"]),
    ("karaoke", &["karaoke"]),
    ("bass boosted", &["bass boosted", "bassboosted"]),
];

/// Phrases that never change the recording and only add noise to titles.
const JUNK: &[&str] = &[
    "official music video", "official video", "official audio", "official lyric video", "lyric video",
    "lyrics video", "lyrics", "visualizer", "official visualizer", "audio", "video", "hq", "hd", "4k",
    "free download", "out now", "explicit", "clean", "radio edit version",
];

/// Removes tags such as "(Official Video)", "[HQ]", "prod. by X", "@handle", links and file extensions.
pub fn clean_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut s = String::with_capacity(lower.len());
    let mut depth = 0usize;
    let mut bracket = String::new();
    for ch in lower.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                bracket.clear();
            }
            ')' | ']' | '}' if depth > 0 => {
                depth -= 1;
                if !is_junk_group(&bracket) {
                    s.push(' ');
                    s.push_str(&bracket);
                    s.push(' ');
                }
                bracket.clear();
            }
            _ if depth > 0 => bracket.push(ch),
            _ => s.push(ch),
        }
    }
    let s = strip_after(&s, &["prod. by", "prod by", "prod.", "(prod"]);
    let s = strip_dash_suffix(&s);
    let words: Vec<&str> = s
        .split_whitespace()
        .filter(|w| !w.starts_with('@') && !w.starts_with("http") && !w.contains(".com"))
        .collect();
    let mut out = words.join(" ");
    for ext in [".mp3", ".wav", ".flac", ".m4a", ".ogg"] {
        out = out.replace(ext, "");
    }
    // Outside brackets only multi-word phrases are safe to drop ("Video Games" is a real title).
    for junk in JUNK.iter().filter(|j| j.contains(' ')) {
        out = remove_phrase(&out, junk);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_junk_group(inner: &str) -> bool {
    let t = inner.trim();
    t.is_empty()
        || JUNK.contains(&t)
        || t.starts_with("prod")
        || t.starts_with("feat")
        || t.starts_with("ft.")
        || t.contains("remaster")
        || t.contains("deluxe")
        || t.starts_with("from ")
}

/// Spotify and Deezer write edition notes after a dash: "Song - Remastered 2011".
fn strip_dash_suffix(s: &str) -> String {
    const NOTES: &[&str] = &["remaster", "deluxe", "mono", "stereo", "single version", "album version", "bonus track", "anniversary"];
    match s.rfind(" - ") {
        Some(i) if NOTES.iter().any(|n| s[i..].contains(n)) => s[..i].to_string(),
        _ => s.to_string(),
    }
}

fn strip_after(s: &str, markers: &[&str]) -> String {
    markers.iter().fold(s.to_string(), |acc, m| match acc.find(m) {
        Some(i) => acc[..i].to_string(),
        None => acc,
    })
}

fn remove_phrase(s: &str, phrase: &str) -> String {
    let padded = format!(" {s} ");
    padded.replace(&format!(" {phrase} "), " ").trim().to_string()
}

pub fn version_tags(title: &str) -> BTreeSet<&'static str> {
    let lower = format!(" {} ", title.to_lowercase().replace(['(', ')', '[', ']', '-', '+', '/'], " "));
    VERSION_TAGS
        .iter()
        .filter(|(_, words)| words.iter().any(|w| lower.contains(&format!(" {w} "))))
        .map(|(tag, _)| *tag)
        .collect()
}

/// Cyrillic letters in Latin (a simple, common transliteration), so "Элевен Эйт" can match
/// "Eleven Eight" uploads.
const CYRILLIC: &[(char, &str)] = &[
    ('а', "a"), ('б', "b"), ('в', "v"), ('г', "g"), ('д', "d"), ('е', "e"), ('ё', "e"), ('ж', "zh"), ('з', "z"), ('и', "i"),
    ('й', "y"), ('к', "k"), ('л', "l"), ('м', "m"), ('н', "n"), ('о', "o"), ('п', "p"), ('р', "r"), ('с', "s"), ('т', "t"),
    ('у', "u"), ('ф', "f"), ('х', "kh"), ('ц', "ts"), ('ч', "ch"), ('ш', "sh"), ('щ', "shch"), ('ъ', ""), ('ы', "y"), ('ь', ""),
    ('э', "e"), ('ю', "yu"), ('я', "ya"), ('і', "i"), ('ї', "yi"), ('є', "ye"), ('ґ', "g"), ('ў', "u"),
];

/// Accented Latin letters and their plain form. Covers Western, Central European and
/// Turkish letters; anything else is kept as it is.
const ACCENTS: &[(&str, &str)] = &[
    ("àáâãäåāăą", "a"), ("æ", "ae"), ("çćĉċč", "c"), ("ďđ", "d"), ("èéêëēĕėęě", "e"), ("ĝğġģ", "g"), ("ĥħ", "h"),
    ("ìíîïĩīĭįı", "i"), ("ĵ", "j"), ("ķ", "k"), ("ĺļľŀł", "l"), ("ñńņňŉ", "n"), ("òóôõöøōŏő", "o"), ("œ", "oe"),
    ("ŕŗř", "r"), ("śŝşšș", "s"), ("ß", "ss"), ("ţťŧț", "t"), ("ùúûüũūŭůűų", "u"), ("ŵ", "w"), ("ýÿŷ", "y"), ("źżž", "z"), ("þ", "th"),
];

/// Lowercase, accents removed, Cyrillic written in Latin: the form both sides are compared in.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars().flat_map(char::to_lowercase) {
        if let Some((_, latin)) = CYRILLIC.iter().find(|(c, _)| *c == ch) {
            out.push_str(latin);
        } else if let Some((_, plain)) = ACCENTS.iter().find(|(set, _)| set.contains(ch)) {
            out.push_str(plain);
        } else {
            out.push(ch);
        }
    }
    out
}

/// Folded alphanumeric words without filler.
fn words(s: &str) -> BTreeSet<String> {
    fold(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "the" | "a" | "feat" | "ft" | "x" | "and"))
        .map(str::to_string)
        .collect()
}

fn overlap(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f32 {
    if a.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f32 / a.len() as f32
}

/// 0.0 (different song) to 1.0 (certain). Hard rules first, then word overlap.
pub fn score(target: &Track, candidate: &Track) -> f32 {
    if version_tags(&target.title) != version_tags(&candidate.title) {
        return 0.0;
    }
    if let (Some(a), Some(b)) = (target.duration_ms, candidate.duration_ms) {
        if a.abs_diff(b) > DURATION_TOLERANCE_MS {
            return 0.0;
        }
    }
    let target_title = words(&clean_title(&target.title));
    let cand_title = words(&clean_title(&candidate.title));
    let target_artist = words(&target.artist_line());
    // Uploaders often put the artist in the title ("Artist - Song"), so check both fields.
    let cand_all: BTreeSet<String> = cand_title.union(&words(&candidate.artist_line())).cloned().collect();
    let title_score = overlap(&target_title, &cand_all);
    let artist_score = overlap(&target_artist, &cand_all);
    let mut s = 0.65 * title_score + 0.35 * artist_score;
    if candidate.is_video {
        s -= 0.05;
    }
    s
}

pub fn best_match<'a>(target: &Track, candidates: &'a [Track]) -> Option<&'a Track> {
    best_scored(target, candidates).map(|(c, _)| c)
}

/// Best candidate and its score. Among near-equal candidates the plain song (audio upload)
/// wins over a music video of the same recording, which may have an intro or skits. A full
/// tie goes to the candidate the service ranked higher.
pub fn best_scored<'a>(target: &Track, candidates: &'a [Track]) -> Option<(&'a Track, f32)> {
    let scored: Vec<(f32, &Track)> =
        candidates.iter().filter(|c| c.id != target.id).map(|c| (score(target, c), c)).filter(|(s, _)| *s >= MIN_SCORE).collect();
    let top = scored.iter().map(|(s, _)| *s).fold(f32::MIN, f32::max);
    // `max_by` keeps the last of equal elements, so walk from the bottom of the ranking.
    scored
        .iter()
        .rev()
        .filter(|(s, _)| top - *s < 0.08)
        .max_by(|a, b| (!a.1.is_video).cmp(&!b.1.is_video).then(a.0.total_cmp(&b.0)))
        .map(|(s, c)| (*c, *s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::ArtistRef;

    fn t(source: &str, id: &str, title: &str, artist: &str, secs: u64) -> Track {
        Track {
            id: Track::make_id(source, id),
            source: source.into(),
            source_id: id.into(),
            title: title.into(),
            artists: vec![ArtistRef { id: None, name: artist.into() }],
            album: None,
            duration_ms: Some(secs * 1000),
            cover: None,
            is_video: false,
            link: None,
            via: None,
            listens: None,
        }
    }

    #[test]
    fn cleans_junk() {
        assert_eq!(clean_title("Song Name (Official Video) [HQ]"), "song name");
        assert_eq!(clean_title("Song Name (Remastered 2011)"), "song name");
        assert_eq!(clean_title("Song Name prod. by Someone"), "song name");
        assert_eq!(clean_title("Song Name @label https://x.com/y"), "song name");
        assert_eq!(clean_title("track.mp3"), "track");
        assert_eq!(clean_title("Song (Slowed + Reverb)"), "song slowed + reverb");
    }

    #[test]
    fn detects_versions() {
        assert_eq!(version_tags("Song (Slowed + Reverb)").into_iter().collect::<Vec<_>>(), vec!["reverb", "slowed"]);
        assert!(version_tags("Song - Live at Wembley").contains("live"));
        assert!(version_tags("Song").is_empty());
        assert!(version_tags("Delivery").is_empty(), "substring 'live' must not count");
    }

    #[test]
    fn never_matches_other_version() {
        let target = t("youtube", "a", "addiction", "LONOWN", 209);
        let slowed = t("soundcloud", "1", "addiction (slowed)", "LONOWN", 209);
        assert_eq!(score(&target, &slowed), 0.0);
    }

    #[test]
    fn rejects_wrong_duration() {
        let target = t("youtube", "a", "addiction", "LONOWN", 209);
        assert_eq!(score(&target, &t("soundcloud", "1", "addiction", "LONOWN", 215)), 0.0);
        assert!(score(&target, &t("soundcloud", "1", "addiction", "LONOWN", 211)) > 0.9);
    }

    #[test]
    fn handles_artist_in_title() {
        let target = t("youtube", "a", "starly", "LONOWN, riserayss", 135);
        let cand = t("soundcloud", "1", "LONOWN, riserayss - starly", "some uploader", 136);
        assert!(score(&target, &cand) >= MIN_SCORE);
    }

    #[test]
    fn transliterates_and_folds_accents() {
        assert_eq!(fold("ФРУКТОВЫЙ"), "fruktovyy");
        assert_eq!(fold("Beyoncé Señorita Ærø"), "beyonce senorita aero");
        let target = t("spotify", "a", "ФРУКТОВЫЙ", "ЭЛЕВЕН ЭЙТ", 125);
        let cand = t("youtube", "b", "Fruktovyy", "Eleven Eyt", 125);
        assert!(score(&target, &cand) >= MIN_SCORE);
    }

    #[test]
    fn drops_edition_notes_after_dash() {
        assert_eq!(clean_title("Hey Jude - Remastered 2015"), "hey jude");
        assert_eq!(clean_title("Song - Live"), "song - live");
    }

    #[test]
    fn prefers_audio_over_music_video() {
        let target = t("spotify", "a", "Fever", "Buckshot", 145);
        let mut video = t("youtube", "v", "Fever (Official Video)", "Buckshot", 146);
        video.is_video = true;
        let audio = t("youtube", "s", "Fever", "Buckshot", 147);
        let cands = vec![video, audio];
        assert_eq!(best_match(&target, &cands).unwrap().source_id, "s");
    }

    #[test]
    fn picks_best_and_skips_self() {
        let target = t("youtube", "a", "Fever", "Buckshot", 145);
        let cands = vec![
            target.clone(),
            t("soundcloud", "1", "Fever (Remix)", "Buckshot", 145),
            t("soundcloud", "2", "Fever", "Buckshot", 146),
            t("soundcloud", "3", "Something else", "Other", 145),
        ];
        assert_eq!(best_match(&target, &cands).unwrap().source_id, "2");
    }

    #[test]
    fn skips_karaoke_and_tribute_versions() {
        let target = t("local", "f", "Wake Me Up", "Avicii", 247);
        let karaoke = t("youtube", "k", "Wake Me up (Originally Performed by Avicii) [Karaoke Version]", "The Karaoke Channel", 245);
        let tribute = t("youtube", "t", "Wake Me Up (A Tribute to Avicii)", "Double Ignition Trax", 247);
        assert_eq!(score(&target, &karaoke), 0.0);
        assert_eq!(score(&target, &tribute), 0.0);
    }

    #[test]
    fn equal_scores_keep_the_service_ranking() {
        let target = t("spotify", "a", "Fever", "Buckshot", 145);
        let cands = vec![t("youtube", "first", "Fever", "Buckshot", 146), t("youtube", "second", "Fever", "Buckshot", 145)];
        assert_eq!(best_match(&target, &cands).unwrap().source_id, "first");
    }
}
