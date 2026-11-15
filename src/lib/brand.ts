/** Relay Dev is the build from the project folder (tauri.dev.conf.json builds the pages with
 * `--mode relaydev`). Known at build time, so the real Relay carries none of this. */
export const DEV_BUILD = import.meta.env.MODE === "relaydev";

/** The app's name on screen. */
export const APP_NAME = DEV_BUILD ? "Relay Dev" : "Relay";
