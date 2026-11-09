//! Lyrics commands for the webview: lookup, preloading, translation, the lyrics editor and the
//! DeepL key.

use crate::commands::St;
use crate::error::{AppError, AppResult};
use crate::lyrics::{self, Line, Lyrics};
use crate::sources::Track;
use crate::translate::{self, Translation};

/// Songs ahead in the queue whose lyrics are fetched early.
const PRELOAD_MAX: usize = 5;

fn check(track: &Track) -> AppResult<()> {
    if track.is_sane() {
        Ok(())
    } else {
        Err(AppError::new("invalid_input", "bad track"))
    }
}

fn check_lang(lang: &str) -> AppResult<()> {
    if translate::LANGS.contains(&lang) {
        Ok(())
    } else {
        Err(AppError::new("invalid_input", "unsupported language"))
    }
}

#[tauri::command]
pub async fn lyrics_get(state: St<'_>, track: Track) -> AppResult<Option<Lyrics>> {
    check(&track)?;
    lyrics::get(state.inner(), &track).await
}

/// Looks up lyrics (and translations, when `lang` is set) for the next songs in the background,
/// so they show the moment a song starts.
#[tauri::command]
pub fn lyrics_preload(state: St, tracks: Vec<Track>, lang: Option<String>) -> AppResult<()> {
    if let Some(l) = &lang {
        check_lang(l)?;
    }
    let tracks: Vec<Track> = tracks.into_iter().filter(Track::is_sane).take(PRELOAD_MAX).collect();
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        // One after another: preloading is never urgent, and the services prefer it calm.
        for track in tracks {
            if !state.net.online() {
                return;
            }
            let found = lyrics::get(&state, &track).await;
            if let (Ok(Some(_)), Some(lang)) = (found, &lang) {
                // A failed preload is tried again (and reported) when the song plays.
                let _ = translate::translate(&state, &track.id, lang).await;
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn lyrics_translate(state: St<'_>, track_id: String, lang: String) -> AppResult<Translation> {
    check_lang(&lang)?;
    if track_id.is_empty() || track_id.len() > 200 {
        return Err(AppError::new("invalid_input", "bad track id"));
    }
    translate::translate(state.inner(), &track_id, &lang).await
}

#[tauri::command]
pub fn lyrics_save(state: St, track: Track, lines: Vec<Line>, plain: String) -> AppResult<Lyrics> {
    check(&track)?;
    state.db.upsert_track(&track)?;
    lyrics::save_user(&state.db, &track.id, lines, &plain)
}

/// Drops the user's own lyrics and looks the song up again.
#[tauri::command]
pub async fn lyrics_reset(state: St<'_>, track: Track) -> AppResult<Option<Lyrics>> {
    check(&track)?;
    lyrics::reset_user(&state.db, &track.id)?;
    lyrics::get(state.inner(), &track).await
}

#[tauri::command]
pub fn deepl_key_saved() -> bool {
    crate::secrets::get(translate::DEEPL_SECRET).is_some()
}

/// Saves (after checking it with DeepL) or removes the DeepL key. Translations made with the
/// other service stay; new songs use the new choice.
#[tauri::command]
pub async fn deepl_key_set(state: St<'_>, key: Option<String>) -> AppResult<()> {
    match key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        Some(key) => {
            translate::check_deepl_key(&state, &key).await?;
            crate::secrets::set(translate::DEEPL_SECRET, &key)
        }
        None => {
            crate::secrets::delete(translate::DEEPL_SECRET);
            Ok(())
        }
    }
}
