<script lang="ts">
  import { onMount } from "svelte";
  import SignIn from "./auth/SignIn.svelte";
  import { api, type Account } from "./lib/api";
  import Toasts from "./lib/components/Toasts.svelte";
  import { backgrounds } from "./lib/stores/backgrounds.svelte";
  import { desktop } from "./lib/stores/desktop.svelte";
  import { youtube } from "./lib/stores/youtube.svelte";
  import { history } from "./lib/stores/history.svelte";
  import { home } from "./lib/stores/home.svelte";
  import { imports } from "./lib/stores/imports.svelte";
  import { library } from "./lib/stores/library.svelte";
  import { offline } from "./lib/stores/offline.svelte";
  import { player } from "./lib/stores/player.svelte";
  import { restoreSession, session } from "./lib/stores/session.svelte";
  import { settings } from "./lib/stores/settings.svelte";
  import { sound } from "./lib/stores/sound.svelte";
  import { speedDial } from "./lib/stores/speedDial.svelte";
  import { ui } from "./lib/stores/ui.svelte";
  import Shell from "./shell/Shell.svelte";

  let attached = false;

  async function enter(account: Account) {
    await settings.load().catch(() => settings.reset());
    if (!attached) {
      attached = true;
      await Promise.all([player.attach(), library.attach(), imports.attach(), offline.attach(), history.attach()]);
    }
    await library.refresh().catch((e) => console.error("library load failed", e));
    await offline.refresh().catch((e) => console.error("offline state load failed", e));
    await Promise.all([
      history.refresh().catch((e) => console.error("play counts load failed", e)),
      speedDial.refresh().catch((e) => console.error("speed dial load failed", e)),
      backgrounds.refresh().catch((e) => console.error("backgrounds load failed", e)),
    ]);
    await player.restore();
    await desktop.attach().catch((e) => console.error("desktop integration failed", e));
    youtube.attach().catch((e) => console.error("YouTube account state failed", e));
    desktop.autoPlay();
    // After the queue's volume is back, so startup keeps it; then each device's own EQ applies.
    sound.attach().catch((e) => console.error("sound setup failed", e));
    api.trashPurgeExpired().catch(() => {});
    session.account = account;
  }

  async function signOut() {
    await player.saveNow().catch(() => {});
    await player.reset();
    await api.signOut();
    library.reset();
    offline.reset();
    history.reset();
    speedDial.reset();
    home.reset();
    backgrounds.reset();
    settings.reset();
    ui.resetNav();
    session.account = null;
  }

  /** Started in main.ts; shows the logo if loading takes a while. */
  let { splashTimer }: { splashTimer: number } = $props();

  function hideSplash() {
    clearTimeout(splashTimer);
    const el = document.getElementById("splash");
    if (!el) return;
    // A logo that never showed goes at once; a shown one fades out.
    if (!el.classList.contains("show")) return el.remove();
    el.classList.add("done");
    setTimeout(() => el.remove(), 260);
  }

  onMount(() => {
    restoreSession()
      .then(() => (session.account ? enter(session.account) : undefined))
      .catch((e) => console.error("startup failed", e))
      .finally(hideSplash);

    // Right-clicks outside our own menus would show the browser's menu, which has no place here.
    const noContext = (e: MouseEvent) => {
      if (!(e.target instanceof HTMLInputElement)) e.preventDefault();
    };
    window.addEventListener("contextmenu", noContext);
    return () => {
      window.removeEventListener("contextmenu", noContext);
    };
  });
</script>

{#if session.ready}
  {#if session.account}
    <Shell onSignOut={signOut} />
  {:else}
    <SignIn onSignedIn={enter} />
  {/if}
{/if}
<Toasts />
