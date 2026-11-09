<script lang="ts">
  import { api, type ArtistPage, type Collection, type SearchSource } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Cover from "../lib/components/Cover.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import Tile from "../lib/components/Tile.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { library } from "../lib/stores/library.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import { errorText } from "../shell/dialogs/importText";
  import EmptyState from "./EmptyState.svelte";

  let { source, id, name }: { source: SearchSource; id: string; name: string } = $props();
  let page = $state<ArtistPage | null>(null);
  let error = $state("");
  let allTop = $state(false);
  let aboutOpen = $state(false);

  async function load() {
    error = "";
    try {
      page = await api.artistPage(source, id);
      ui.heroCover = page.avatar;
    } catch (e) {
      error = errorText(e);
    }
  }

  $effect(() => {
    void id;
    page = null;
    allTop = aboutOpen = false;
    load();
    return () => (ui.heroCover = null);
  });

  const following = $derived(library.isFollowing(source, id));
  const top = $derived(page ? page.topTracks.slice(0, allTop ? 10 : 5) : []);

  function toggleFollow() {
    if (!page) return;
    library.setFollow({ source, artistId: id, name: page.name, avatar: page.avatar }, !following);
  }

  async function openRelease(c: Collection) {
    if (c.kind === "track") {
      // SoundCloud "singles" are single songs: play them.
      try {
        player.playTracks([await api.soundcloudTrack(c.id)]);
      } catch {
        ui.toast(t("toast.collectionFailed"), "alert");
      }
    } else ui.open({ name: "album", source: c.source, id: c.id });
  }

  function releaseSub(c: Collection): string {
    return c.year ? String(c.year) : c.subtitle;
  }
</script>

{#if error}
  <h1 class="page-title">{name}</h1>
  <EmptyState icon="alert" title={error}>
    <Button variant="secondary" icon="refresh" onclick={load}>{t("dialog.retry")}</Button>
  </EmptyState>
{:else if !page}
  <div class="center"><Spinner size={22} /></div>
{:else}
  <header class="hero">
    <Cover src={page.avatar} size={176} round fallback="user" />
    <div class="info">
      <span class="kind">{t("artist.kind")}</span>
      <h1>{page.name}</h1>
      <div class="actions">
        <Button variant="primary" icon="play" disabled={!page.topTracks.length} onclick={() => player.playTracks(page!.topTracks)}>{t("artist.play")}</Button>
        {#if page.topTracks[0]}
          <IconButton icon="radio" label={t("menu.startRadio")} variant="solid" onclick={() => player.startRadio(page!.topTracks[0])} />
        {/if}
        <Button variant="secondary" icon={following ? "userCheck" : "userPlus"} onclick={toggleFollow}>{following ? t("artist.following") : t("artist.follow")}</Button>
        {#if page.link}<IconButton icon="external" label={t("album.openOnline")} variant="solid" onclick={() => api.openExternal(page!.link!)} />{/if}
      </div>
    </div>
  </header>

  {#if page.topTracks.length}
    <section>
      <h2>{t("artist.topSongs")}</h2>
      <TrackList tracks={top} />
      {#if page.topTracks.length > 5}
        <button class="more" onclick={() => (allTop = !allTop)}>{allTop ? t("artist.showLess") : t("artist.showMore")}</button>
      {/if}
    </section>
  {/if}

  {#each [{ list: page.albums, label: t("artist.albums") }, { list: page.singles, label: t("artist.singles") }] as group (group.label)}
    {#if group.list.length}
      <section>
        <h2>{group.label}</h2>
        <div class="grid">
          {#each group.list as c (c.kind + c.id)}
            <Tile title={c.title} subtitle={releaseSub(c)} cover={c.cover} icon="disc" onclick={() => openRelease(c)} />
          {/each}
        </div>
      </section>
    {/if}
  {/each}

  {#if page.about}
    <section>
      <h2>{t("artist.about")}</h2>
      <p class="about" class:open={aboutOpen}>{page.about}</p>
      {#if page.about.length > 320}
        <button class="more" onclick={() => (aboutOpen = !aboutOpen)}>{aboutOpen ? t("artist.showLess") : t("artist.readMore")}</button>
      {/if}
    </section>
  {/if}

  {#if page.similar.length}
    <section>
      <h2>{t("artist.similar")}</h2>
      <div class="grid">
        {#each page.similar.slice(0, 12) as a (a.id)}
          <Tile title={a.name} cover={a.avatar} round icon="user" onclick={() => ui.open({ name: "artist", source: a.source, id: a.id, artistName: a.name })} />
        {/each}
      </div>
    </section>
  {/if}
{/if}

<style>
  .center {
    display: grid;
    place-items: center;
    height: 50vh;
  }
  .hero {
    display: flex;
    align-items: flex-end;
    gap: var(--s5);
    margin: 8px 0 var(--s6);
  }
  .info {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .kind {
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }
  h1 {
    margin: 0 0 10px;
    font-size: 48px;
    font-weight: 800;
    letter-spacing: -0.03em;
    line-height: 1.05;
  }
  .actions {
    display: flex;
    gap: var(--s2);
  }
  section {
    margin-bottom: var(--s6);
  }
  h2 {
    margin: 0 0 var(--s3);
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 4px;
    margin: 0 -8px;
  }
  .more {
    margin: 6px 8px 0;
    padding: 0;
    border: 0;
    background: none;
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-2);
  }
  .more:hover {
    color: var(--text);
  }
  .about {
    max-width: 760px;
    margin: 0;
    color: var(--text-2);
    line-height: 1.6;
    white-space: pre-line;
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .about.open {
    display: block;
  }
</style>
