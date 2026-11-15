<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import Avatar from "../../lib/components/Avatar.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import IconButton from "../../lib/components/IconButton.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { menu } from "../../lib/stores/menu.svelte";
  import { session } from "../../lib/stores/session.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { pages, type PageId } from "./pages";
  import AppearancePage from "./AppearancePage.svelte";
  import PlaybackPage from "./PlaybackPage.svelte";
  import StoragePage from "./StoragePage.svelte";
  import AboutPage from "./AboutPage.svelte";
  import DiscordPage from "./DiscordPage.svelte";
  import MiniPlayerPage from "./MiniPlayerPage.svelte";
  import StartupPage from "./StartupPage.svelte";
  import YouTubePage from "./YouTubePage.svelte";
  import DocsPage from "./DocsPage.svelte";

  let { onSignOut }: { onSignOut: () => void } = $props();

  let page = $state<PageId>("playback");
  let query = $state("");
  const q = $derived(query.trim().toLowerCase());

  const hit = (key: Key) => t(key).toLowerCase().includes(q);
  const rowVisible = (label: Key, desc: Key) => !q || hit(label) || hit(desc);
  const pageMatches = (p: (typeof pages)[number]) => !q || hit(p.label) || p.rows.some(([l, d]) => hit(l) || hit(d));
  const shown = $derived(pages.filter(pageMatches));
  const groups = $derived([...new Set(shown.map((p) => p.group))]);
  const current = $derived(pages.find((p) => p.id === page)!);
  // When searching, rows are filtered too, unless the page name itself matched.
  const visible = $derived((l: Key, d: Key) => !q || hit(current.label) || rowVisible(l, d));

  // Each page opens at its top, not at the previous page's scroll position.
  let content = $state<HTMLElement>();
  $effect(() => {
    void page;
    content?.scrollTo({ top: 0 });
  });

  $effect(() => {
    if (q && shown.length && !shown.some((p) => p.id === page)) page = shown[0].id;
  });

  function openProfile() {
    ui.settingsOpen = false;
    ui.open({ name: "profile" });
  }

  function onKey(e: KeyboardEvent) {
    // Escape closes only the top layer: a dialog or menu over Settings goes first.
    if (e.key === "Escape" && !e.defaultPrevented && !dialogs.current && !menu.open) ui.settingsOpen = false;
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="scrim" role="presentation" onpointerdown={() => (ui.settingsOpen = false)} transition:fade={{ duration: 160 }}></div>
<div class="window glass" role="dialog" aria-modal="true" aria-label={t("nav.settings")} transition:scale={{ start: 0.97, duration: 180 }}>
  <aside>
    <label class="search">
      <Icon name="search" size={15} />
      <input bind:value={query} placeholder={t("settings.search")} spellcheck="false" />
    </label>
    <nav>
      {#each groups as group (group)}
        <div class="group">{t(group)}</div>
        {#each shown.filter((p) => p.group === group) as p (p.id)}
          <button class:on={p.id === page} onclick={() => (page = p.id)}><Icon name={p.icon} size={16} />{t(p.label)}</button>
        {/each}
      {/each}
      {#if shown.length === 0}
        <p class="none">{t("settings.noMatch", { q: query.trim() })}</p>
      {/if}
    </nav>
    {#if session.account}
      <div class="profile">
        <button class="who" title={t("profile.open")} onclick={openProfile}>
          <Avatar src={session.account.avatar} name={session.account.displayName} size={32} />
          <span class="names">
            <span class="name truncate">{session.account.displayName}</span>
            <span class="email truncate">{session.account.email}</span>
          </span>
        </button>
        <IconButton icon="logout" label={t("settings.signOut")} onclick={onSignOut} />
      </div>
    {/if}
  </aside>

  <section>
    <header>
      <div>
        <h2>{t(current.label)}</h2>
        {#if current.id === "playback"}<p>{t("settings.sub.playback")}</p>{/if}
        {#if current.id === "appearance"}<p>{t("settings.sub.appearance")}</p>{/if}
        {#if current.id === "docs"}<p>{t("settings.sub.docs")}</p>{/if}
      </div>
      <IconButton icon="close" label={t("settings.close")} onclick={() => (ui.settingsOpen = false)} />
    </header>
    <div class="content" bind:this={content}>
      {#if current.id === "playback"}
        <PlaybackPage {visible} />
      {:else if current.id === "appearance"}
        <AppearancePage {visible} />
      {:else if current.id === "storage"}
        <StoragePage {visible} />
      {:else if current.id === "miniPlayer"}
        <MiniPlayerPage {visible} />
      {:else if current.id === "youtube"}
        <YouTubePage {visible} />
      {:else if current.id === "discord"}
        <DiscordPage {visible} />
      {:else if current.id === "startup"}
        <StartupPage {visible} />
      {:else if current.id === "about"}
        <AboutPage />
      {:else if current.id === "docs"}
        <DocsPage onPage={(p) => ((query = ""), (page = p))} />
      {/if}
    </div>
  </section>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 70;
    background: rgba(var(--scrim), 0.4);
    backdrop-filter: blur(14px);
  }
  .window {
    position: fixed;
    left: 50%;
    top: 50%;
    width: min(940px, calc(100vw - 80px));
    height: min(720px, calc(100vh - 80px));
    translate: -50% -50%;
    z-index: 71;
    display: grid;
    grid-template-columns: 240px 1fr;
    overflow: hidden;
    border-radius: 16px;
    background: rgba(var(--panel), 0.8);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.45);
  }
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 12px 10px;
    background: var(--well);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--r-md);
    background: rgba(var(--ink), 0.07);
    color: var(--text-3);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    font-size: var(--fs-sm);
    color: var(--text);
  }
  .search input::placeholder {
    color: var(--text-3);
  }
  nav {
    flex: 1;
    overflow-y: auto;
    margin-top: 10px;
  }
  .group {
    margin: 14px 8px 6px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-3);
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-md);
    background: transparent;
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--text-2);
    text-align: left;
  }
  nav button:hover {
    color: var(--text);
    background: var(--hover);
  }
  nav button.on {
    color: var(--text);
    background: var(--selected);
  }
  .none {
    margin: 12px 8px;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
  .profile {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px;
    border-radius: var(--r-lg);
    background: rgba(var(--ink), 0.06);
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    margin: -4px;
    padding: 4px;
    border: 0;
    border-radius: var(--r-md);
    background: none;
    text-align: left;
  }
  .who:hover {
    background: var(--hover);
  }
  .names {
    display: grid;
    min-width: 0;
  }
  .name {
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .email {
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  section {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 20px 16px 10px 24px;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  header p {
    margin: 4px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    /* Room below the last row, so a toast (like Undo) never hides it for good. */
    padding: 10px 24px 72px;
  }
  /* Section headings and cards shared by the settings pages. */
  .content :global(h3) {
    margin: 22px 2px 10px;
    font-size: var(--fs-md);
    font-weight: 700;
  }
  .content :global(h3:first-child) {
    margin-top: 0;
  }
  .content :global(.card) {
    border-radius: var(--r-lg);
    background: rgba(var(--ink), 0.05);
  }
  .content :global(.note) {
    margin: 0;
    padding: 0 16px 14px;
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--text-3);
  }
</style>
