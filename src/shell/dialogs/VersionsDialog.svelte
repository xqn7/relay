<!-- Other versions of a song (slowed, sped up, remix, live …) found on YouTube and SoundCloud.
     Clicking one plays it instead; inside a playlist it can also replace the song there. -->
<script lang="ts">
  import { api, artistLine, formatTime, isAppError, type Track, type Version } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Cover from "../../lib/components/Cover.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import SourceBadge from "../../lib/components/SourceBadge.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { fail, library } from "../../lib/stores/library.svelte";
  import { offline } from "../../lib/stores/offline.svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  let { track, playlist }: { track: Track; playlist?: { id: string; itemId: string; onReplaced: () => void } } = $props();

  let versions = $state<Version[]>([]);
  let loading = $state(true);
  let error = $state("");

  /** Results per song, for this session: opening the dialog again is instant. */
  const cache = new Map<string, Version[]>();

  $effect(() => {
    const hit = cache.get(track.id);
    if (hit) {
      versions = hit;
      loading = false;
      return;
    }
    if (!offline.online) {
      error = t("error.offline");
      loading = false;
      return;
    }
    api
      .otherVersions(track)
      .then((found) => {
        cache.set(track.id, found);
        versions = found;
      })
      .catch((e) => (error = isAppError(e) && (e.code === "offline" || e.code === "network") ? t("error.offline") : t("versions.failed")))
      .finally(() => (loading = false));
  });

  const groups = $derived.by(() => {
    const out: { kind: Version["kind"]; tracks: Track[] }[] = [];
    for (const v of versions) {
      const last = out[out.length - 1];
      if (last?.kind === v.kind) last.tracks.push(v.track);
      else out.push({ kind: v.kind, tracks: [v.track] });
    }
    return out;
  });

  function play(v: Track) {
    player.switchVersion(track, v);
    dialogs.close();
  }

  async function replace(v: Track) {
    if (!playlist) return;
    try {
      await api.playlistReplace(playlist.id, playlist.itemId, v);
      playlist.onReplaced();
      library.refresh().catch(() => {});
      ui.toast(t("versions.replaced", { title: v.title }), "check");
      dialogs.close();
    } catch (e) {
      fail(e);
    }
  }
</script>

<Dialog title={t("versions.title")} sub={`${track.title} · ${artistLine(track)}`} width={600} onclose={() => dialogs.close()}>
  {#if loading}
    <div class="status"><Spinner size={16} />{t("versions.searching")}</div>
  {:else if error}
    <div class="status"><Icon name="alert" size={16} />{error}</div>
  {:else if groups.length === 0}
    <div class="status">{t("versions.none")}</div>
  {:else}
    <div class="list">
      {#each groups as g (g.kind)}
        <h3>{t(`versions.kind.${g.kind}`)}</h3>
        {#each g.tracks as v (v.id)}
          <div class="row">
            <button class="main" title={t("versions.play")} onclick={() => play(v)}>
              <Cover src={v.cover} size={40} />
              <div class="text">
                <div class="name truncate">{v.title}</div>
                <div class="sub"><SourceBadge source={v.source} /><span class="truncate">{artistLine(v)}</span></div>
              </div>
              <span class="time">{formatTime(v.durationMs)}</span>
            </button>
            {#if playlist}
              <Button small variant="secondary" onclick={() => replace(v)}>{t("versions.replace")}</Button>
            {/if}
          </div>
        {/each}
      {/each}
    </div>
  {/if}
</Dialog>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 2px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .list {
    max-height: 56vh;
    overflow-y: auto;
    margin: 0 -6px;
    padding: 0 6px;
  }
  h3 {
    margin: 14px 2px 6px;
    font-size: var(--fs-sm);
    font-weight: 700;
  }
  h3:first-child {
    margin-top: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    border-radius: var(--r-md);
  }
  .row:hover {
    background: var(--hover);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
  .time {
    padding-right: 4px;
    font-size: var(--fs-xs);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .row :global(.btn) {
    margin-right: 6px;
  }
</style>
