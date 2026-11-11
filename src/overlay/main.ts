import { mount } from "svelte";
import "../lib/tokens.css";

// One page for both overlay windows; each loads only its own view.
const view = new URLSearchParams(location.search).get("w");
const load = view === "lyrics" ? import("./DesktopLyrics.svelte") : import("./MiniPlayer.svelte");
load.then((m) => mount(m.default, { target: document.getElementById("app")! }));
