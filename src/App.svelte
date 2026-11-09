<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import SignIn from "./auth/SignIn.svelte";
  import { api, type Account } from "./lib/api";
  import Toasts from "./lib/components/Toasts.svelte";
  import { t } from "./lib/i18n/index.svelte";
  import { imports } from "./lib/stores/imports.svelte";
  import { library } from "./lib/stores/library.svelte";
  import { player } from "./lib/stores/player.svelte";
  import { restoreSession, session } from "./lib/stores/session.svelte";
  import { settings } from "./lib/stores/settings.svelte";
  import { ui } from "./lib/stores/ui.svelte";
  import Shell from "./shell/Shell.svelte";

  let attached = false;

  async function enter(account: Account) {
    await settings.load().catch(() => settings.reset());
    if (!attached) {
      attached = true;
      await Promise.all([player.attach(), library.attach(), imports.attach()]);
    }
    await library.refresh().catch((e) => console.error("library load failed", e));
    await player.restore();
    api.trashPurgeExpired().catch(() => {});
    session.account = account;
  }

  async function signOut() {
    await player.saveNow().catch(() => {});
    await player.reset();
    await api.signOut();
    library.reset();
    settings.reset();
    ui.resetNav();
    session.account = null;
  }

  function hideSplash() {
    const el = document.getElementById("splash");
    if (!el) return;
    el.classList.add("done");
    setTimeout(() => el.remove(), 260);
  }

  onMount(() => {
    let helperToast: number | null = null;
    const off = listen<{ received: number; total: number | null; done: boolean }>("tool-progress", ({ payload }) => {
      const pct = payload.total ? Math.round((payload.received / payload.total) * 100) : 0;
      helperToast = payload.done
        ? ui.progressToast(helperToast, t("toast.helperReady"), null)
        : ui.progressToast(helperToast, t("toast.helperDownloading", { pct }), pct / 100);
    });

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
      off.then((f) => f());
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
