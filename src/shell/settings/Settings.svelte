<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import Avatar from "../../lib/components/Avatar.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import IconButton from "../../lib/components/IconButton.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { session } from "../../lib/stores/session.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { pages, type PageId } from "./pages";
  import AppearancePage from "./AppearancePage.svelte";
  import PlaybackPage from "./PlaybackPage.svelte";
  import StoragePage from "./StoragePage.svelte";

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

  $effect(() => {
    if (q && shown.length && !shown.some((p) => p.id === page)) page = shown[0].id;
  });

  function openProfile() {
    ui.settingsOpen = false;
    ui.open({ name: "profile" });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") ui.settingsOpen = false;
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
      </div>
      <IconButton icon="close" label={t("settings.close")} onclick={() => (ui.settingsOpen = false)} />
    </header>
    <div class="content">
      {#if current.id === "playback"}
        <PlaybackPage {visible} />
      {:else if current.id === "appearance"}
        <AppearancePage {visible} />
      {:else if current.id === "storage"}
        <StoragePage {visible} />
      {:else}
        <div class="soon">
          <Icon name={current.icon} size={28} />
          <p>{t("page.soon")}</p>
        </div>
      {/if}
    </div>
  </section>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 70;
    background: rgba(5, 5, 6, 0.4);
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
    background: rgba(14, 14, 16, 0.78);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.45);
  }
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 12px 10px;
    background: rgba(0, 0, 0, 0.35);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.07);
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
    background: rgba(255, 255, 255, 0.06);
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
    padding: 10px 24px 24px;
  }
  .soon {
    display: grid;
    justify-items: center;
    gap: 10px;
    margin-top: 120px;
    color: var(--text-3);
  }
  .soon p {
    margin: 0;
    font-size: var(--fs-sm);
  }
</style>
