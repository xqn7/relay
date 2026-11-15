# Relay

A small desktop music player for Windows 10 and 11. Search and play from YouTube Music and
SoundCloud in one place, mix both in the same queue and playlists, and bring your playlists over
from Spotify, Apple Music, Deezer, YouTube and SoundCloud by pasting a link.

Relay plays the audio itself (no embedded web players), so it keeps playing in the background,
has gapless playback, crossfade, an equalizer, synced lyrics, an offline song cache and a
floating mini player. Accounts are local to your PC; nothing is sent to a Relay server, because
there isn't one.

Copyright (C) 2026 xqn7. Licensed under the GNU General Public License v3.0 or later (see
[LICENSE](LICENSE)).

Relay isn't made by or connected to YouTube, SoundCloud, Spotify, Apple or Deezer. It uses their
public web pages and web APIs, which can change at any time; see [FIXING.md](FIXING.md).

## Install

Download `Relay_<version>_x64-setup.exe` from the
[latest release](https://github.com/xqn7/relay/releases/latest) and run it. It installs for
your Windows user only (no admin rights), with Start menu and desktop shortcuts and a normal
uninstaller. Relay updates itself: new versions download in the background and install when you
restart it.

Windows SmartScreen may warn on first install because the installer isn't code-signed with a paid
certificate. Click "More info" → "Run anyway".

On first start Relay downloads two helper programs in the background into its own data folder,
each checked against its published SHA-256: yt-dlp (about 24 MB, plays YouTube audio) and ffmpeg
(about 115 MB, for video files, MP3 export and waveforms). Until yt-dlp is ready, songs play from
SoundCloud.

## Getting your music in

### Playlists, albums and songs

1. Copy a link: Share → Copy link in Spotify, Apple Music, Deezer, YouTube / YouTube Music or
   SoundCloud. Short links (`spotify.link`, `deezer.page.link`) and `spotify:` URIs work too.
2. In Relay: Library → **Import**, paste it. You see the name, cover and song count first.
3. Click Import. Songs from Spotify, Apple Music and Deezer are matched to YouTube Music
   (SoundCloud as backup) by artist, title and length. A report lists anything that couldn't be
   found or played, and why. Wrong matches are fixed per song with **Fix match**.

Importing the same link again updates the playlist (new songs added, removed ones removed)
instead of making a copy. M3U/M3U8 files are imported from the Import box ("M3U file") and exported from a playlist's menu.

Spotify playlists with more than 100 songs are read in full through Spotify's web player. If that
ever fails, Relay imports the first 100 and says so.

### Spotify Liked Songs

Liked Songs are private on Spotify, so there are two ways (both explained in the app under
Liked Songs → Import Liked Songs):

- **Paste the links:** in the Spotify desktop app open Liked Songs, click one song, press
  Ctrl+A, then Ctrl+C, and paste into Relay.
- **Data export:** request your data on spotify.com (Account → Privacy → Download your data).
  The zip you get by email contains `YourLibrary.json`; pick that file in Relay.

### Your own files

Drag audio files (mp3, m4a, aac, flac, wav, ogg, opus) or videos (mp4, mkv, webm, mov) onto the
window, or use **Add files** on the Local music page. Relay copies them into
`Music\Relay\Local music` (changeable in Settings → Cache & Storage); originals are never moved
or changed. Files you put into that folder yourself appear on their own.

## When something stops working

Settings → Cache & Storage → **Check connection** tests your internet and each service. Most
YouTube problems are fixed by Settings → About → **Update** next to the YouTube helper. If YouTube
asks to confirm you're not a bot (common on VPNs), sign in under Settings → YouTube. Details per
service, for developers: [FIXING.md](FIXING.md).

## Building from source

Requirements:

- Windows 10 or 11 (x64) with WebView2 (built into Windows 11; Windows 10 gets it with Edge)
- [Node.js](https://nodejs.org) 24 with npm
- [Rust](https://rustup.rs) 1.90 or newer, `x86_64-pc-windows-msvc`
- Visual Studio 2022+ Build Tools with "Desktop development with C++"

```
npm install
npm start
```

`npm start` runs Relay in development mode with live reload. `npm run app` builds the release
app and its installer into `src-tauri\target\release\bundle\nsis\`.

Checks:

```
npm run check        # TypeScript and Svelte
npm test             # frontend unit tests
cd src-tauri
cargo test           # backend unit tests
cargo clippy --all-targets
```

### Releasing

`scripts\release.ps1 -Version 1.2.3 -Notes "What changed"` sets the version everywhere, runs the
tests, builds the installer, signs it for the updater with the key in
`%USERPROFILE%\.relay-keys\relay-updater.key` and writes `latest.json`. Add `-Publish` to push
and create the GitHub release. The private key is never part of the repository; its public half
is `PUBLIC_KEY` in `src-tauri/src/updater.rs`.

## How it's put together

- **Tauri 2** shell (Rust backend, the system's WebView2) with a **Svelte 5** frontend.
- All network requests run in Rust: YouTube Music catalog through `rustypipe`, YouTube audio
  through yt-dlp, SoundCloud's web API, and the import-only readers for Spotify, Apple Music and
  Deezer. Each service is one file in `src-tauri/src/sources/`.
- Songs resolve in a fixed order: on disk (Local music, offline saves, song cache), then the
  song's own service, then a stand-in from the other one (YouTube first, SoundCloud second),
  20 seconds at most.
- Audio streams through Rust to the page (`relay://`), which plays it with the Web Audio API:
  two decks for gapless and crossfade, normalization, a 10-band EQ, speed and pitch.
- Everything is stored in one SQLite database in `%APPDATA%\app.relay.player`; songs, covers and
  helper programs in `%LOCALAPPDATA%\app.relay.player`.

## Data and privacy

Accounts, passwords (Argon2id hashes), playlists, history and settings stay on your PC. Relay
talks to the music services, LRCLIB (lyrics), Google Translate (only when you turn on lyrics
translation; or DeepL with your own key), SponsorBlock (song hashes only, to skip talking
intros in music videos) and GitHub (updates and helper downloads). The YouTube sign-in is
optional; its cookies are kept in Windows Credential Manager.
