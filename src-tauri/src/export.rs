//! "Download as MP3" and clip export: a song (or part of it) as an MP3 with title, artist,
//! album and cover tags, in a folder the user picks.

use crate::error::{AppError, AppResult};
use crate::sources::Track;
use crate::util::safe_file_stem;
use crate::AppState;
use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::{Tag, TagType};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;

const FOLDER_KEY: &str = "export_folder";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDone {
    pub saved: usize,
    /// Titles of the songs that could not be saved.
    pub failed: Vec<String>,
    pub folder: String,
    /// The file, when exactly one was saved.
    pub file: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportProgress {
    done: usize,
    total: usize,
}

/// Asks for the target folder, starting where the last export went.
pub async fn pick_folder(app: &tauri::AppHandle, state: &AppState) -> AppResult<Option<PathBuf>> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file();
    if let Some(last) = state.db.device_get(FOLDER_KEY)?.map(PathBuf::from).filter(|p| p.is_dir()) {
        dialog = dialog.set_directory(last);
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    dialog.pick_folder(move |p| {
        let _ = tx.send(p);
    });
    let Some(Some(picked)) = rx.await.ok() else { return Ok(None) };
    let folder = picked.into_path().map_err(|e| AppError::new("io", e.to_string()))?;
    state.db.device_set(FOLDER_KEY, &folder.to_string_lossy())?;
    Ok(Some(folder))
}

/// "m:ss" for clip names; dots instead of colons, which file names can't hold.
fn clock(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}.{:02}", s / 60, s % 60)
}

pub fn file_stem(track: &Track, range: Option<(u64, u64)>) -> String {
    let artist = track.artist_line();
    let name = if artist.is_empty() { track.title.clone() } else { format!("{artist} - {}", track.title) };
    let name = match range {
        Some((a, b)) => format!("{name} (clip {}-{})", clock(a), clock(b)),
        None => name,
    };
    safe_file_stem(&name, "Song")
}

/// Exports each song in turn, reporting progress; one failing song doesn't stop the rest.
pub async fn export_all(state: &Arc<AppState>, tracks: &[Track], range: Option<(u64, u64)>, folder: &Path) -> ExportDone {
    let mut done = ExportDone { saved: 0, failed: Vec::new(), folder: folder.to_string_lossy().into_owned(), file: None };
    for (i, track) in tracks.iter().enumerate() {
        let _ = state.app.emit("export-progress", ExportProgress { done: i, total: tracks.len() });
        match to_mp3(state, track, range, folder).await {
            Ok(path) => {
                done.saved += 1;
                done.file = Some(path.to_string_lossy().into_owned());
            }
            Err(e) => {
                eprintln!("export of {} failed: {e}", track.id);
                done.failed.push(track.title.clone());
            }
        }
    }
    if done.saved != 1 {
        done.file = None;
    }
    let _ = state.app.emit("export-progress", ExportProgress { done: tracks.len(), total: tracks.len() });
    done
}

async fn to_mp3(state: &Arc<AppState>, track: &Track, range: Option<(u64, u64)>, folder: &Path) -> AppResult<PathBuf> {
    let source = crate::streams::audio_file(state, track).await?;
    let stem = file_stem(track, range);
    let tmp = folder.join(format!("{stem}.relay-export.mp3"));
    let result = convert(state, &source, range, &tmp).await;
    let result = match result {
        Ok(()) => write_tags(state, track, &tmp).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    let dest = crate::local::free_path(folder, &stem, "mp3");
    tokio::fs::rename(&tmp, &dest).await?;
    Ok(dest)
}

/// An MP3 whole song is copied as it is; everything else is encoded (VBR ~245 kbit/s, the
/// highest LAME quality setting), and only the asked-for part when there's a range.
async fn convert(state: &AppState, source: &Path, range: Option<(u64, u64)>, dest: &Path) -> AppResult<()> {
    let is_mp3 = source.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
    if is_mp3 && range.is_none() {
        tokio::fs::copy(source, dest).await?;
        return Ok(());
    }
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    if let Some((a, b)) = range {
        if b <= a {
            return Err(AppError::new("invalid_input", "clip end is before its start"));
        }
        args.extend(["-ss".into(), format!("{:.3}", a as f64 / 1000.0).into(), "-to".into(), format!("{:.3}", b as f64 / 1000.0).into()]);
    }
    args.extend(["-i".into(), source.as_os_str().to_owned()]);
    for a in ["-vn", "-map", "0:a:0", "-map_metadata", "-1", "-c:a", "libmp3lame", "-q:a", "0", "-f", "mp3"] {
        args.push(a.into());
    }
    args.push(dest.as_os_str().to_owned());
    let refs: Vec<&std::ffi::OsStr> = args.iter().map(|a| a.as_os_str()).collect();
    state.tools.run_ffmpeg(&refs).await?;
    Ok(())
}

fn tag_error(e: impl std::fmt::Display) -> AppError {
    AppError::new("tags", e.to_string())
}

async fn write_tags(state: &AppState, track: &Track, path: &Path) -> AppResult<()> {
    // A missing cover only means an MP3 without one.
    let cover = match &track.cover {
        Some(url) => crate::covers::best(state, url).await.ok(),
        None => None,
    };
    let (title, artist, album) = (track.title.clone(), track.artist_line(), track.album.as_ref().map(|a| a.name.clone()));
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let mut file = lofty::read_from_path(&path).map_err(tag_error)?;
        if file.tag(TagType::Id3v2).is_none() {
            file.insert_tag(Tag::new(TagType::Id3v2));
        }
        let tag = file.tag_mut(TagType::Id3v2).ok_or_else(|| AppError::new("tags", "no ID3 tag"))?;
        tag.set_title(title);
        if !artist.is_empty() {
            tag.set_artist(artist);
        }
        if let Some(album) = album {
            tag.set_album(album);
        }
        if let Some((bytes, mime)) = cover {
            let mime = match mime.as_str() {
                "image/png" => Some(MimeType::Png),
                "image/jpeg" => Some(MimeType::Jpeg),
                _ => None,
            };
            // Players don't show WebP covers in MP3 tags, so those are left out.
            if let Some(mime) = mime {
                tag.remove_picture_type(PictureType::CoverFront);
                tag.push_picture(Picture::unchecked(bytes.to_vec()).pic_type(PictureType::CoverFront).mime_type(mime).build());
            }
        }
        tag.save_to_path(&path, WriteOptions::default()).map_err(tag_error)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::new("internal", e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_files_after_the_song() {
        let t = Track::imported("youtube", "x", "Song: Part 1?", vec!["A".into(), "B".into()], Some(1), None, None);
        assert_eq!(file_stem(&t, None), "A, B - Song_ Part 1_");
        assert_eq!(file_stem(&t, Some((62_300, 92_000))), "A, B - Song_ Part 1_ (clip 1.02-1.32)");
    }
}
