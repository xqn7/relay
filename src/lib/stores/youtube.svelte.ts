// The optional YouTube sign-in: its state, and the account's YouTube Music playlists for the
// library ("Sync library"). The login itself happens in Google's own page in a private window.
import { listen } from "@tauri-apps/api/event";
import { api, type Collection } from "../api";
import { t } from "../i18n/index.svelte";
import { settings } from "./settings.svelte";
import { ui } from "./ui.svelte";

class YouTubeStore {
  signedIn = $state(false);
  /** Signed in, but YouTube no longer accepts the login. */
  expired = $state(false);
  playlists = $state<Collection[]>([]);
  private attached = false;

  async attach(): Promise<void> {
    if (this.attached) return;
    this.attached = true;
    await listen<boolean>("youtube-account", ({ payload }) => {
      if (payload) ui.toast(t("youtube.signedIn"), "check");
      this.refresh();
    });
    await this.refresh();
  }

  async refresh(): Promise<void> {
    const s = await api.youtubeStatus().catch(() => ({ signedIn: false, expired: false }));
    this.signedIn = s.signedIn;
    this.expired = s.expired;
    await this.loadPlaylists();
  }

  async loadPlaylists(): Promise<void> {
    if (!this.signedIn || this.expired || !settings.values.youtubeSyncLibrary) {
      this.playlists = [];
      return;
    }
    this.playlists = await api.youtubePlaylists().catch((e) => {
      console.error("YouTube playlists failed", e);
      return [];
    });
  }

  signIn(): void {
    api.youtubeSignIn().catch(() => ui.toast(t("youtube.signInFailed"), "alert"));
  }

  async signOut(): Promise<void> {
    await api.youtubeSignOut().catch(() => {});
  }
}

export const youtube = new YouTubeStore();
