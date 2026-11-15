import { mount } from "svelte";
import "../lib/tokens.css";
import { i18nReady } from "../lib/i18n/index.svelte";

// One page for both overlay windows; each loads only its own view.
const view = new URLSearchParams(location.search).get("w");
const load = view === "lyrics" ? import("./DesktopLyrics.svelte") : import("./MiniPlayer.svelte");
// Drawn once its language is there, so it never shows English first.
Promise.all([load, i18nReady]).then(([m]) => mount(m.default, { target: document.getElementById("app")! }));
