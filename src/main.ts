import { mount } from "svelte";
import "./lib/tokens.css";
import App from "./App.svelte";
import { APP_NAME } from "./lib/brand";
import { i18nReady } from "./lib/i18n/index.svelte";

document.title = APP_NAME;

/** Loading shorter than this shows no logo at all, so a fast start never flickers. */
const SPLASH_AFTER_MS = 200;
const splashTimer = window.setTimeout(() => document.getElementById("splash")?.classList.add("show"), SPLASH_AFTER_MS);

// The app draws once its language is loaded, so it never shows English first.
i18nReady.then(() => mount(App, { target: document.getElementById("app")!, props: { splashTimer } }));
