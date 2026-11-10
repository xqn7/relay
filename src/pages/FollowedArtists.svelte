<!-- Library → Artists: followed artists, each with their newest releases. -->
<script lang="ts">
  import { api, type Collection, type Releases } from "../lib/api";
  import Cover from "../lib/components/Cover.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import Tile from "../lib/components/Tile.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { artistMenu, collectionMenu } from "../lib/pins";
  import { library } from "../lib/stores/library.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import EmptyState from "./EmptyState.svelte";

  let rows = $state<Releases[] | null>(null);
  let failed = $state(false);

  $effect(() => {
    void library.follows.length;
    failed = false;
    api
      .followedReleases()
      .then((r) => (rows = r))
      .catch(() => (failed = true));
  });

  async function open(c: Collection) {
    if (c.kind === "track") {
      try {
        player.playTracks([await api.soundcloudTrack(c.id)]);
      } catch {
        ui.toast(t("toast.collectionFailed"), "alert");
      }
    } else ui.open({ name: "album", source: c.source, id: c.id });
  }
</script>

{#if library.follows.length === 0}
  <EmptyState icon="user" title={t("follows.emptyTitle")} body={t("follows.emptyBody")} />
{:else if failed}
  <EmptyState icon="alert" title={t("error.offline")} />
{:else if !rows}
  <div class="center"><Spinner size={20} /></div>
{:else}
  {#each rows as r (r.artist.source + r.artist.artistId)}
    <section>
      <button
        class="artist"
        onclick={() => ui.open({ name: "artist", source: r.artist.source, id: r.artist.artistId, artistName: r.artist.name })}
        oncontextmenu={(e) => artistMenu(e, { source: r.artist.source, id: r.artist.artistId, name: r.artist.name, avatar: r.artist.avatar })}>
        <Cover src={r.artist.avatar} size={40} round fallback="user" />
        <span class="name">{r.artist.name}</span>
        <Icon name="forward" size={16} />
      </button>
      {#if r.failed}
        <p class="note">{t("follows.failed")}</p>
      {:else if r.releases.length === 0}
        <p class="note">{t("follows.noReleases")}</p>
      {:else}
        <div class="grid">
          {#each r.releases as c (c.kind + c.id)}
            <Tile title={c.title} subtitle={c.year ? String(c.year) : ""} cover={c.cover} icon="disc" onclick={() => open(c)} onmenu={(e) => collectionMenu(e, c)} />
          {/each}
        </div>
      {/if}
    </section>
  {/each}
{/if}

<style>
  .center {
    display: grid;
    place-items: center;
    height: 30vh;
  }
  section {
    margin-bottom: var(--s5);
  }
  .artist {
    display: inline-flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 8px;
    padding: 4px 10px 4px 4px;
    border: 0;
    border-radius: var(--r-full);
    background: transparent;
    color: var(--text-2);
  }
  .artist:hover {
    background: var(--hover);
    color: var(--text);
  }
  .name {
    font-size: var(--fs-lg);
    font-weight: 700;
    color: var(--text);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 4px;
    margin: 0 -8px;
  }
  .note {
    margin: 0 0 0 4px;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
</style>
