<script lang="ts">
  import logo from "../lib/assets/logo.svg";
  import Avatar from "../lib/components/Avatar.svelte";
  import Cover from "../lib/components/Cover.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { newFolder, newPlaylist, openFolderMenu, openPlaylistMenu } from "../lib/playlistMenu";
  import { desktop } from "../lib/stores/desktop.svelte";
  import { dialogs } from "../lib/stores/dialogs.svelte";
  import { dnd, dropTarget, type Payload } from "../lib/stores/dnd.svelte";
  import { library } from "../lib/stores/library.svelte";
  import { menu } from "../lib/stores/menu.svelte";
  import { session } from "../lib/stores/session.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import { folderDrop, folderPress, playlistDrop, playlistPress } from "../pages/libraryDrag";

  /** Folders shown open in the rail. Hovering a folder while dragging opens it too. */
  let open = $state<Set<string>>(new Set());
  let hoverTimer = 0;

  const topFolders = $derived(library.overview.folders.filter((f) => !f.parentId));
  const loose = $derived(library.overview.playlists.filter((p) => !p.folderId));
  const childrenOf = (id: string) => ({
    folders: library.overview.folders.filter((f) => f.parentId === id),
    playlists: library.overview.playlists.filter((p) => p.folderId === id),
  });

  function toggle(id: string) {
    const next = new Set(open);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    open = next;
  }

  // While something is dragged over a closed folder, open it after a moment so its
  // playlists can be reached.
  $effect(() => {
    const target = dnd.target ?? "";
    clearTimeout(hoverTimer);
    if (target.startsWith("folder:")) {
      const id = target.slice(7);
      if (!open.has(id)) hoverTimer = window.setTimeout(() => toggle(id), 600);
    }
  });

  const likedDrop = {
    key: "liked",
    accepts: (p: Payload) => p.kind === "tracks",
    drop: (p: Payload) => {
      if (p.kind !== "tracks") return;
      for (const track of p.tracks.filter((x) => !library.isLiked(x))) library.toggleLike(track);
    },
  };

  function createMenu(e: MouseEvent) {
    menu.show(e, [
      { label: t("library.newPlaylist"), icon: "plus", action: () => newPlaylist() },
      { label: t("library.newFolder"), icon: "folderPlus", action: () => newFolder() },
      { label: t("library.import"), icon: "download", divider: true, action: () => dialogs.open({ kind: "import" }) },
      { label: t("liked.import"), icon: "heart", action: () => dialogs.open({ kind: "importLiked" }) },
    ]);
  }

  const isRoute = (name: string, id?: string) => ui.route.name === name && (id == null || ("id" in ui.route && ui.route.id === id));
</script>

<nav class="sidebar">
  <div class="logo" data-tauri-drag-region><img class="logo-mark" src={logo} alt="Relay" width="30" height="30" draggable="false" />{#if desktop.info?.dev}<span class="dev">Dev</span>{/if}</div>
  <div class="rail">
    <button class:active={isRoute("home")} aria-label={t("nav.home")} title={t("nav.home")} onclick={() => ui.open({ name: "home" })}>
      <Icon name="home" size={20} />
    </button>
    <button class:active={ui.searchOpen} aria-label={t("nav.search")} title={t("nav.search")} onclick={() => (ui.searchOpen = true)}>
      <Icon name="search" size={20} />
    </button>
    <button class:active={isRoute("library")} class:files={ui.filesOver} aria-label={t("nav.library")} title={t("nav.library")} onclick={() => ui.open({ name: "library" })}>
      <Icon name="library" size={20} />
    </button>
  </div>

  <div class="rail lists">
    <button
      class="item"
      class:active={isRoute("liked")}
      class:over={dnd.target === "liked"}
      title={t("liked.title")}
      aria-label={t("liked.title")}
      use:dropTarget={likedDrop}
      onclick={() => ui.open({ name: "liked" })}>
      <span class="glyph"><Icon name="heart" size={18} fill /></span>
    </button>
    <button class="item" class:active={isRoute("local")} class:over={ui.filesOver} title={t("local.title")} aria-label={t("local.title")} onclick={() => ui.open({ name: "local" })}>
      <span class="glyph"><Icon name="folder" size={18} /></span>
    </button>
    <div class="sep"></div>
    <div class="scroll">
      {#each topFolders as f (f.id)}
        {@const kids = childrenOf(f.id)}
        <button
          class="item"
          class:active={isRoute("folder", f.id)}
          class:over={dnd.target === `folder:${f.id}`}
          class:opened={open.has(f.id)}
          title={f.name}
          aria-label={f.name}
          aria-expanded={open.has(f.id)}
          use:dropTarget={folderDrop(f.id)}
          onclick={() => toggle(f.id)}
          ondblclick={() => ui.open({ name: "folder", id: f.id })}
          oncontextmenu={(e) => openFolderMenu(e, f.id)}
          onpointerdown={(e) => folderPress(e, f)}>
          <span class="glyph"><Icon name="folder" size={18} /></span>
        </button>
        {#if open.has(f.id)}
          <div class="nested">
            {#each kids.folders as sub (sub.id)}
              <button
                class="item small"
                class:active={isRoute("folder", sub.id)}
                class:over={dnd.target === `folder:${sub.id}`}
                title={sub.name}
                aria-label={sub.name}
                use:dropTarget={folderDrop(sub.id)}
                onclick={() => ui.open({ name: "folder", id: sub.id })}
                oncontextmenu={(e) => openFolderMenu(e, sub.id)}
                onpointerdown={(e) => folderPress(e, sub)}>
                <span class="glyph"><Icon name="folder" size={15} /></span>
              </button>
            {/each}
            {#each kids.playlists as p (p.id)}
              <button
                class="item small"
                class:active={isRoute("playlist", p.id)}
                class:over={dnd.target === `playlist:${p.id}`}
                title={p.name}
                aria-label={p.name}
                use:dropTarget={playlistDrop(p.id)}
                onclick={() => ui.open({ name: "playlist", id: p.id })}
                oncontextmenu={(e) => openPlaylistMenu(e, p)}
                onpointerdown={(e) => playlistPress(e, p)}>
                <Cover src={p.cover} size={32} radius={6} fallback="list" />
              </button>
            {/each}
          </div>
        {/if}
      {/each}
      {#each loose as p (p.id)}
        <button
          class="item"
          class:active={isRoute("playlist", p.id)}
          class:over={dnd.target === `playlist:${p.id}`}
          title={p.name}
          aria-label={p.name}
          use:dropTarget={playlistDrop(p.id)}
          onclick={() => ui.open({ name: "playlist", id: p.id })}
          oncontextmenu={(e) => openPlaylistMenu(e, p)}
          onpointerdown={(e) => playlistPress(e, p)}>
          <Cover src={p.cover} size={36} radius={7} fallback="list" />
        </button>
      {/each}
    </div>
    <button class="add" aria-label={t("library.create")} title={t("library.create")} onclick={createMenu}>
      <Icon name="plus" size={18} />
    </button>
  </div>

  {#if session.account}
    <button class="me" class:active={isRoute("profile")} aria-label={t("profile.open")} title={session.account.displayName} onclick={() => ui.open({ name: "profile" })}>
      <Avatar src={session.account.avatar} name={session.account.displayName} size={28} />
    </button>
  {/if}
  <button class="settings" class:active={ui.settingsOpen} aria-label={t("nav.settings")} title={t("nav.settings")} onclick={() => (ui.settingsOpen = true)}>
    <Icon name="settings" size={20} />
  </button>
</nav>

<style>
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    width: calc(var(--sidebar-w) + 12px);
    z-index: 20;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 12px 0 14px 6px;
  }
  .logo {
    display: grid;
    place-items: center;
    height: 40px;
    margin-bottom: 14px;
  }
  .logo img {
    pointer-events: none;
  }
  .logo {
    position: relative;
  }
  .dev {
    position: absolute;
    bottom: -8px;
    left: 50%;
    transform: translateX(-50%);
    padding: 0 5px;
    border-radius: 4px;
    background: var(--glass-raised);
    color: var(--text-2);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    line-height: 15px;
    pointer-events: none;
  }
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 6px;
    border-radius: var(--r-lg);
    background: var(--well);
  }
  .lists {
    margin-top: 10px;
    min-height: 0;
    flex: 0 1 auto;
  }
  .scroll {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .scroll::-webkit-scrollbar {
    display: none;
  }
  .sep {
    width: 22px;
    height: 1px;
    margin: 2px 0;
    background: var(--hairline);
  }
  button {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: var(--r-md);
    background: transparent;
    color: var(--text-3);
    transition:
      color var(--t-fast) var(--ease),
      background var(--t-fast) var(--ease),
      transform var(--t) var(--ease),
      box-shadow var(--t) var(--ease);
  }
  button:hover {
    color: var(--text);
  }
  button.active {
    color: var(--text);
    background: var(--hover);
  }
  .item.active {
    box-shadow: 0 0 0 2px rgba(var(--ink), 0.55);
  }
  .item.over,
  .files {
    color: var(--text);
    transform: scale(1.12);
    box-shadow: 0 0 0 2px rgba(var(--ink), 0.9);
  }
  .glyph {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 7px;
    background: rgba(var(--ink), 0.08);
    color: var(--text-2);
  }
  .opened .glyph {
    background: rgba(var(--ink), 0.14);
    color: var(--text);
  }
  .nested {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 4px 0;
    border-radius: var(--r-md);
    background: rgba(var(--ink), 0.04);
  }
  .small {
    width: 36px;
    height: 36px;
  }
  .small .glyph {
    width: 32px;
    height: 32px;
  }
  .add {
    width: 32px;
    height: 32px;
  }
  /* The profile picture sits above Settings at the bottom of the rail. */
  .me {
    margin-top: auto;
    border-radius: 50%;
  }
  .me + .settings {
    margin-top: 6px;
  }
  .settings {
    margin-top: auto;
    margin-bottom: 14px;
  }
</style>
