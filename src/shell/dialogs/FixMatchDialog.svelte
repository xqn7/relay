<script lang="ts">
  import { untrack } from "svelte";
  import { api, artistLine, formatTime, type Track } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Cover from "../../lib/components/Cover.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import SourceBadge from "../../lib/components/SourceBadge.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { fail, library } from "../../lib/stores/library.svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { errorText } from "./importText";

  let { track }: { track: Track } = $props();

  let query = $state(untrack(() => `${track.artists[0]?.name ?? ""} ${track.title}`.trim()));
  let current = $state<Track | null>(null);
  let candidates = $state<Track[]>([]);
  let loading = $state(false);
  let error = $state("");
  let seq = 0;

  $effect(() => {
    api.matchGet(track).then((m) => (current = m)).catch(() => {});
    search();
  });

  async function search() {
    const mine = ++seq;
    loading = true;
    error = "";
    try {
      const found = await api.matchCandidates(track, query);
      if (mine === seq) candidates = found;
    } catch (e) {
      if (mine === seq) error = errorText(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  async function choose(c: Track) {
    try {
      await api.matchSet(track, c);
      ui.toast(t("fix.saved", { title: track.title }), "check");
      dialogs.close();
      await library.refresh();
    } catch (e) {
      fail(e);
    }
  }
</script>

<Dialog title={t("fix.title")} sub={t("fix.sub")} width={580} onclose={() => dialogs.close()}>
  <div class="original">
    <Cover src={track.cover} size={48} />
    <div class="text">
      <div class="name truncate">{track.title}</div>
      <div class="sub truncate">{artistLine(track)} · {formatTime(track.durationMs)}</div>
    </div>
  </div>
  {#if current}
    <p class="current"><Icon name="check" size={14} />{t("fix.currently", { title: current.title, artist: artistLine(current) })}</p>
  {/if}

  <form
    class="search"
    onsubmit={(e) => {
      e.preventDefault();
      search();
    }}>
    <label class="field">
      <Icon name="search" size={15} />
      <input bind:value={query} spellcheck="false" />
    </label>
    <Button variant="secondary" type="submit">{t("fix.search")}</Button>
  </form>

  {#if loading}
    <div class="status"><Spinner size={16} />{t("fix.searching")}</div>
  {:else if error}
    <div class="status"><Icon name="alert" size={16} />{error}</div>
  {:else if candidates.length === 0}
    <div class="status">{t("fix.none")}</div>
  {:else}
    <div class="list">
      {#each candidates as c (c.id)}
        <div class="cand" class:chosen={current?.id === c.id}>
          <Cover src={c.cover} size={40} />
          <div class="text">
            <div class="name truncate">{c.title}</div>
            <div class="sub"><SourceBadge source={c.source} /><span class="truncate">{artistLine(c)}{c.isVideo ? ` · ${t("fix.video")}` : ""}</span></div>
          </div>
          <span class="time">{formatTime(c.durationMs)}</span>
          <button class="icon" title={t("fix.preview")} aria-label={t("fix.preview")} onclick={() => player.playTracks([c])}><Icon name="play" size={14} /></button>
          <Button variant={current?.id === c.id ? "ghost" : "secondary"} disabled={current?.id === c.id} onclick={() => choose(c)}>
            {current?.id === c.id ? t("fix.inUse") : t("fix.use")}
          </Button>
        </div>
      {/each}
    </div>
  {/if}
</Dialog>

<style>
  .original {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
  .current {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 0 0;
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
  .search {
    display: flex;
    gap: var(--s2);
    margin: 16px 0 8px;
  }
  .field {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px;
    border-radius: var(--r-md);
    background: var(--field);
    color: var(--text-3);
  }
  .field input {
    flex: 1;
    border: 0;
    outline: none;
    background: transparent;
    font-size: var(--fs-sm);
    color: var(--text);
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 2px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .cand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px;
    border-radius: var(--r-md);
  }
  .cand:hover,
  .chosen {
    background: var(--hover);
  }
  .cand .name {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .time {
    font-size: var(--fs-xs);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 50%;
    background: var(--glass-strong);
    color: var(--text);
  }
</style>
