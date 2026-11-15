# Fixing Relay when a service changes

Relay reads YouTube, SoundCloud, Spotify, Apple Music and Deezer through their public web
pages and internal web APIs, not official keys. Those change without notice. Each service lives
in its own file, so a fix touches one place. This page lists what usually breaks, how it shows,
and where to look.

First step for any report: **Settings → Cache & Storage → Check connection** (also offered on
failure toasts). It tests each service separately and "Copy error details" gives the exact
error. Rust log lines go to the terminal when Relay runs with `npm run tauri dev`.

All paths below are under `src-tauri/src/`.

## YouTube audio (yt-dlp)

How it shows: YouTube songs fail and play from SoundCloud instead ("YouTube playback is failing
right now. Trying SoundCloud."); the connection check marks YouTube.

| Symptom | Cause | Fix |
|---|---|---|
| Every YouTube song fails after a YouTube update | yt-dlp is out of date (signature / "n" parameter, PO tokens) | Settings → About → Update the YouTube helper. Nothing to change in code. |
| "Sign in to confirm you're not a bot" | YouTube blocks the IP (VPNs, shared IPs) | Settings → YouTube → Sign in. The error is recognized in `tools.rs` `classify_ytdlp_error` (code `signin_required`). |
| New yt-dlp error text not recognized | yt-dlp reworded a message | Add the text to `classify_ytdlp_error` in `tools.rs` (unit-tested there). |
| Wrong format / no audio | Format selector no longer matches | `Tools::ytdlp_stream` in `tools.rs` (the `-f` argument). |
| yt-dlp download fails | GitHub release layout or checksum file changed | `YTDLP_BASE`, `install_ytdlp`, `expected_hash` in `tools.rs`. |

Signed-in playback passes the youtube.com cookies to yt-dlp through a temporary file
(`youtube_account.rs` `netscape`, `sync_tools`).

## YouTube Music catalog (search, albums, artists, radio, lyrics fallback)

Uses the `rustypipe` crate (InnerTube). How it shows: search returns only SoundCloud results,
artist/album pages show an error, radio and daily mixes don't fill.

- First try updating the crate: `cargo update -p rustypipe`, then run the tests.
- Mapping from rustypipe's types to Relay's lives in `sources/youtube.rs` (`map_album`,
  `map_artist`, `best_thumb`, `search`, `album_page`, `artist_page`, `radio`, `lyrics`).
- rustypipe caches its client data in the app's local data folder (`youtube/`); deleting that
  folder forces a fresh start.
- The signed-in library ("Sync library") uses a second client in `youtube_account.rs`
  (`with_client`, `playlists`, `playlist_tracks`). If Google refuses the embedded login page,
  the login window is `open_login` / `capture` there.

## SoundCloud

How it shows: SoundCloud results vanish from search, SoundCloud songs fail, sets don't import.

| Symptom | Cause | Fix |
|---|---|---|
| 401/403 on every request | The public `client_id` changed | Refreshed automatically. If scraping fails, the regex is in `find_client_id` (`sources/soundcloud.rs`, unit-tested). |
| Songs fail but search works | Stream "transcodings" changed shape | `pick_transcoding` and `resolve` in `sources/soundcloud.rs`. HLS joining is in `streams.rs`. |
| Tracks show as previews | Go+ detection changed | `is_preview_only` in `sources/soundcloud.rs`. |
| Sets import empty or cut short | api-v2 playlist shape changed | `import` and `collection` in `sources/soundcloud.rs`. |

## Spotify (import only)

Two paths, both in `sources/spotify.rs`:

1. **Embed page** (`https://open.spotify.com/embed/...`, first 100 songs). Reads the
   `__NEXT_DATA__` JSON. If imports fail completely, check `next_data`, `entity`, `parse_embed`.
2. **Full playlists over 100 songs**: the embed page's anonymous token plus the web player's
   `fetchPlaylist` query (`api-partner.spotify.com/pathfinder/v2/query`). The query's hash is read
   from the web player script (`find_player_script`, `find_query_hash`) and re-read once when
   Spotify answers 412. When this path breaks, imports keep the first 100 songs and say the
   playlist was cut off; fix `query_hash`, `playlist_page` or `parse_page`.

Liked Songs: pasted links are parsed in `sources/links.rs` (`spotify_track_ids`), the data export
in `parse_your_library`.

## Apple Music (import only)

Reads the song list from the public page's embedded JSON (`sources/apple.rs`: `page_data`,
`parse_page`, `map_lockup`). When Apple changes the page, imports say the playlist can't be
read. Save the page HTML, look for the new JSON block, adjust `page_data` and the field names.

## Deezer (import only)

Public API at `api.deezer.com` (`sources/deezer.rs`). Stable for years. Errors come back as an
`error` object in a 200 response, handled in `check`. Paging is `fetch` (100 per page).

## Links

Every pasted link goes through `sources/links.rs` (`parse`), with unit tests per service. New
URL shapes (a new short-link domain, a new path prefix) are added there. Short links
(`spotify.link`, `deezer.page.link`) are followed in `importer.rs` `expand_short`.

## Matching imported songs

`matcher.rs` (title cleaning, version tags, transliteration, duration) and `matching.rs` (search
order: YouTube Music songs, then videos, then SoundCloud). When a kind of upload keeps winning
wrongly (karaoke, "sped up" without the word in the title), add a version word or junk pattern
in `matcher.rs` and a test next to the existing ones. Users can fix single songs with Fix match.

## Lyrics and translation

- LRCLIB (`lyrics.rs`, `lrclib`, `pick`): a free community service; when it's down, lyrics come
  from YouTube Music instead (`youtube`). The connection check tests it.
- Translation (`translate.rs`): Google's free web endpoint (`google_request`, `parse_google`).
  If Google changes or blocks it, a DeepL Free key in Settings → Appearance takes over.

## Covers

Covers load through `relay://img/...` (`covers.rs`). HD cover rules (rewriting Google/Apple
image URLs to 1200 px, YouTube `maxresdefault`) are in `hd_variant`.

## Other moving parts

- **ffmpeg download** (video import, MP3 export, waveforms): `tools.rs`, from Gyan's GitHub
  mirror with gyan.dev as fallback (`FFMPEG_RELEASE_API`, `FFMPEG_ZIP`).
- **Discord Rich Presence**: local IPC pipe, `discord.rs`. Discord rarely changes this.
- **Lock screen picture**: `lockscreen.rs` (WinRT `LockScreen` API).
- **Updates**: `updater.rs` reads `latest.json` from the newest GitHub release and checks the
  minisign signature against `PUBLIC_KEY`. Releases are made with `scripts/release.ps1`.
- **Online/offline detection**: `network.rs` (Windows network events plus a probe of
  msftconnecttest.com / google.com/generate_204).

## After a fix

```
cd src-tauri
cargo test
cargo clippy --all-targets
cd ..
npm run check
npm test
```

Then play a song from the fixed service in the real app, and run the connection check.
