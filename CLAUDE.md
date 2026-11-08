# Project rules — always follow these

This is my music player project. The full feature list and design are in prompt.md, the build plan and checklists are in PLAN.md. These rules apply in every session.

## Phase rules
- In the first session: do Step 1 from prompt.md (questions), then plan the WHOLE app (all phases, full architecture, data model, folder structure) so later features fit without rewrites. Save it as PLAN.md in the project folder. In PLAN.md, every single feature and setting from prompt.md gets its own checkbox under the phase that builds it; nothing from prompt.md may be missing. Only check an item off after you've tested it. Then build Phase 1 only.
- Every later session starts by reading prompt.md, PLAN.md, and the existing code, then builds only the phase I name.
- Git: the project folder is a git repository, connected to my public GitHub repo once I've set it up. Commit at stable points during a phase, and always at the end of a phase with a message like "Phase 3: offline and storage". Only push to GitHub when I ask. If a change breaks things, use git to go back.
- The repo is public, so secrets must never be committed: the updater's private signing key, passwords, tokens, .env files, my accounts/library database, and anything in the app's data folder. Also keep these planning files out of the repo (add them to .gitignore): prompt.md, PLAN.md, and START HERE.txt; they stay only on my PC. Also keep the design-reference folder out of the repo (it has screenshots of another app and other artists' covers). Set up .gitignore for this from the start and check before every commit. Never include code copied from Liner or other projects whose license forbids it.
- Don't break what works: before changing code that earlier phases depend on, write down which older features it affects, and re-test exactly those after the change.
- Each phase ends with: the app builds and runs, that phase's features work and are tested, earlier features still work, PLAN.md is updated (checked items, notes, anything changed), and a short summary for me. Then ask me: "Phase N is done. Start Phase N+1 (<its name>)?" Don't start it on your own.
- When I say yes, start the next phase in a fresh session to save usage: if you can start a new Claude Code session yourself (the Claude desktop app can), start one in this folder with the message "Read prompt.md, PLAN.md and the code, then build Phase N+1." Otherwise, give me that exact message to paste into a new session. Only continue in the same session if I ask for that.
- If something from a later phase is needed earlier (e.g. a settings page), build only the part needed now.
- Keep a placeholder for unfinished pages/settings instead of fake or half-working features.
- I won't test the app myself between phases, so you are the tester. After each phase: build and launch the real app, actually use every new feature (click through the UI, play real songs, import real playlists, check the logs), take screenshots to check the design, and fix every problem you find before you report the phase as done. Don't ask me to try things; tell me what you tested and what you fixed.

## Build standards

### Priorities (in order)
1. Correct: playback is reliable, never crashes, and handles network loss, expired streams, and broken sources gracefully.
2. Lightweight: small size, low RAM, low CPU, fast startup.
3. Simple: the least code and the fewest moving parts that fully solve the problem.
4. Polished: clean UI, smooth playback, feels fast and finished.

### Tech choices
- Prefer a lightweight native shell with the system webview (Tauri is preferred). Do not use Electron unless something truly can't work without it; if so, explain why first.
- Do network requests to the services from the backend (Rust/native side), not the webview, to avoid CORS problems and keep tokens out of the UI.
- Prefer the standard library and native features. Justify every dependency in one line.
- Store the library, match cache, offline songs index, and settings in a small embedded DB (SQLite).
- Don't bundle ffmpeg (it adds ~80 MB). Download it automatically the first time it's needed (importing a video file, Download as MP3, clip export), with a progress indicator, into the app's data folder, and verify the download. yt-dlp is handled the same way if you use it, with one-click updates.

### Lightweight rules
- As few dependencies as possible. Never add a library for something under ~30 lines.
- Load things only when needed. No background polling. Use events.
- Lazy-load and cache album art at small sizes. Virtualize long lists (5,000+ songs must scroll smoothly).
- No memory leaks during long listening sessions (test it for hours).
- Targets: idle CPU near zero, low RAM, startup under 1 second where possible. Report the numbers.

### Design rules — it must NOT look AI-made
- The background takes on the colors of the currently playing song's cover: extract 2–3 dominant colors and show them as a soft, slowly moving blurred color backdrop at the top, fading into near-black below. Transition smoothly when the song changes. If the cover's colors are blue or purple, tone them down toward neutral.
- NO purple or blue gradients anywhere else. No gradient buttons or gradient text.
- None of the generic "AI app" look: neon glows, sparkle icons, emoji in the UI, giant rounded cards with heavy shadows.
- Use a dark, minimal UI: a slim icon sidebar, bold section headings, subtle translucent panels, and a floating player bar at the bottom.
- Design principles:
  - The content is the color. The cover art colors the screen; the UI chrome stays neutral (white/black at low opacity on top).
  - Glass, not boxes. Menus, toasts, and dialogs are translucent with blur, not opaque grey panels.
  - The target responds. When I drag something onto a place (e.g. files onto the library), that place itself reacts, instead of a small box saying "drop here".
  - Calm motion. Short fades, soft springs, one easing curve everywhere.
  - Nothing one-off. Build a small set of shared components (button, glass panel, list row, dropdown, toast) and reuse them everywhere. Define design tokens (colors, fills, spacing, type scale) once.
- No accent color. The whole UI is neutral: near-black, blurry black glass, white, and greys (like Liner). Primary buttons are white with dark text; active/selected states, the playing song, the liked heart, toggles, and progress bars use white or light grey, never a color. The only color in the app comes from the current song's cover, shown softly in the backdrop behind the glass (or from my custom background). It can't be a copy of Spotify's green-on-black look.
- Album art is the main visual. Clear hierarchy, consistent spacing, dense where useful.
- Use real typography: a quality font, sensible sizes and weights, and good alignment.
- Write labels like a human product team would. No buzzwords ("Unleash", "Seamless", "AI-powered").
- Show a small, subtle badge on each track for the source it plays from.
- Use one consistent icon set. Keep animations subtle and fast: normal UI feedback under about 200 ms. Allowed exceptions (as described in their features): song transition (400–600 ms), like animation (300–400 ms), page transitions (150–200 ms), spring reorder, startup animation (under 1 s), idle screen fades, and the slow-moving backdrop.
- Before finishing, ask: "Would a person think AI generated this?" If yes, redesign those parts.

### Code quality rules
- Clear names, small functions, one responsibility each.
- No dead code, no TODOs, no fake or stub logic. Everything must actually work.
- Handle every error properly. Show clear, human messages.
- Validate all fetched data. Never trust external responses.
- Comments explain WHY, not WHAT. No AI-style filler comments.

### UX rules
- Respond instantly. Search is debounced and shows results per source as they arrive.
- Playback starts within ~1–2 seconds of clicking a song.
- After an import, show which songs couldn't be matched or played and why, and let me fix them.
- Media keys, and remembering the queue, volume, position, and EQ between runs.
- Handle empty, loading, offline, source-broken, and error states, not only the happy path.

## Verify before saying a phase (or the app) is done
After each phase, verify that phase's features plus a quick check that earlier phases still work. The full list below is for Phase 8.
- Build and run it yourself. Fix every error and warning.
- Test: playing from each source; a mixed-source queue; gapless transitions; a stream URL expiring mid-song; minimizing the app during playback; importing a 300+ song Spotify playlist, a YouTube playlist with deleted videos, and a SoundCloud set; re-importing; going offline and back online; the song cache size limit; offline-saved songs playing with no internet; radio refilling; Liked Songs from pasted links; network drops; and one source being completely broken.
- Add focused automated tests for core logic (queue, matching, link parsing for all three services, the resolver fallback order, importing).
- Measure the size, startup time, idle RAM, and RAM during playback, and report the numbers.
- Review your own code as a strict senior engineer would, and fix anything bloated, fragile, or unclear.
- Review the UI against the design rules, and fix anything that looks generic or AI-made.

## Delivery (at the end of Phase 8)
- A README covering how to run and build it, the requirements, and how to import playlists and Liked Songs.
- FIXING.md, as described in prompt.md (Robustness).
- A one-click or one-command way to run it.
- An honest summary covering the size and performance numbers, known limitations, and which parts are most likely to break.
- Don't say it's done until it builds, runs, and passes the tests.
