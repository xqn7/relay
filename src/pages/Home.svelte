<!-- Home: speed dial, recently played, today's mixes, most played and artists to discover.
     Everything comes from this account's own listening on this PC. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { artistLine, type Mix } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Shelf from "../lib/components/Shelf.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import Tile from "../lib/components/Tile.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { mixMenu } from "../lib/mixMenu";
  import { history } from "../lib/stores/history.svelte";
  import { home } from "../lib/stores/home.svelte";
  import { offline } from "../lib/stores/offline.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { speedDial } from "../lib/stores/speedDial.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import { openTrackMenu } from "../lib/trackMenu";
  import EmptyState from "./EmptyState.svelte";
  import SpeedDial from "./home/SpeedDial.svelte";

  const MOST_SHOWN = 10;
  let failed = $state(false);

  // The lists are read again after every new play, so Home stays current while it's open.
  $effect(() => {
    void history.version;
    home.loadLists().then(
      () => (failed = false),
      (e) => {
        console.error("home lists failed", e);
        failed = true;
      },
    );
  });

  onMount(() => {
    home.loadMixes();
    if (offline.online) home.loadDiscover();
  });

  const lists = $derived(home.lists);
  const recent = $derived(offline.playable(lists?.recent ?? [], (x) => x));
  const most = $derived(offline.playable(lists?.most ?? [], (x) => x.track).slice(0, MOST_SHOWN));
  const nothingYet = $derived(lists !== null && lists.recent.length === 0);

  function mixCovers(m: Mix): (string | null)[] {
    return [...new Set(m.tracks.map((x) => x.cover).filter(Boolean))].slice(0, 4);
  }
</script>

<h1 class="page-title">{t("home.title")}</h1>

<div class="body">
  <SpeedDial />

  {#if failed}
    <EmptyState icon="alert" title={t("error.generic")}>
      <Button variant="secondary" icon="refresh" onclick={() => home.loadLists().then(() => (failed = false), () => {})}>{t("dialog.retry")}</Button>
    </EmptyState>
  {:else if !lists}
    <div class="center"><Spinner size={22} /></div>
  {:else if nothingYet}
    <EmptyState icon="home" title={speedDial.pins.length ? t("home.noHistoryTitle") : t("home.emptyTitle")} body={t("home.emptyBody")}>
      <Button variant="primary" icon="search" onclick={() => (ui.searchOpen = true)}>{t("home.emptyCta")}</Button>
    </EmptyState>
  {:else}
    {#if recent.length}
      <Shelf title={t("home.recent")}>
        {#each recent as track, i (track.id)}
          <Tile
            title={track.title}
            subtitle={artistLine(track)}
            cover={track.cover}
            onclick={() => player.playTracks(recent, i)}
            onmenu={(e) => openTrackMenu(e, { tracks: recent, index: i })} />
        {/each}
      </Shelf>
    {/if}

    {#if home.mixes === null && !home.mixesFailed}
      <div class="making"><Spinner size={16} /><span>{t("mix.making")}</span></div>
    {:else if home.mixes?.length}
      <Shelf title={t("mix.shelf")} sub={t("mix.shelfSub")}>
        {#each home.mixes as m (m.number)}
          <Tile
            title={t("mix.title", { n: m.number })}
            subtitle={m.artists.join(", ")}
            covers={mixCovers(m)}
            icon="radio"
            onclick={() => ui.open({ name: "mix", number: m.number })}
            onmenu={(e) => mixMenu(e, m)} />
        {/each}
      </Shelf>
    {/if}

    {#if most.length}
      <section class="most">
        <h2>{t("home.most")}</h2>
        <p>{t("home.mostSub")}</p>
        <TrackList tracks={most.map((m) => m.track)} compact />
      </section>
    {/if}

    {#if offline.online && home.suggestions?.length}
      <Shelf title={t("discover.shelf")} action={{ label: t("home.showAll"), run: () => ui.open({ name: "discover" }) }}>
        {#each home.suggestions.slice(0, 10) as s (s.artist.source + s.artist.id)}
          <Tile
            title={s.artist.name}
            subtitle={t("discover.because", { name: s.because })}
            cover={s.artist.avatar}
            round
            icon="user"
            onclick={() => ui.open({ name: "artist", source: s.artist.source, id: s.artist.id, artistName: s.artist.name })} />
        {/each}
      </Shelf>
    {/if}
  {/if}
</div>

<style>
  .body {
    margin-top: var(--s5);
  }
  .center {
    display: grid;
    place-items: center;
    height: 40vh;
  }
  .making {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 var(--s6);
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .most {
    margin-bottom: var(--s6);
  }
  .most h2 {
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .most p {
    margin: 2px 0 var(--s2);
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
</style>
