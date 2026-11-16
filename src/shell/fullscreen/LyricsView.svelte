<!-- The lyrics column of the fullscreen player: synced lyrics that follow the song (click a line
     to jump there), plain lyrics, search, translation, animation style and the way into the
     editor. -->
<script lang="ts">
  import { tick } from "svelte";
  import Button from "../../lib/components/Button.svelte";
  import IconButton from "../../lib/components/IconButton.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { activeIndex, buildLines, foldText } from "../../lib/lyrics/lines";
  import { visual } from "../../lib/stores/visual.svelte";
  import { desktop } from "../../lib/stores/desktop.svelte";
  import LyricText from "../../lib/lyrics/LyricText.svelte";
  import { livePosition } from "../../lib/lyrics/position.svelte";
  import { lyrics } from "../../lib/stores/lyrics.svelte";
  import { menu } from "../../lib/stores/menu.svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { LYRICS_STYLES, settings } from "../../lib/stores/settings.svelte";

  let { onEdit }: { onEdit: () => void } = $props();

  /** After the user scrolls, the view stays where they put it for this long. */
  const FREE_SCROLL_MS = 4000;

  const pos = livePosition();
  const style = $derived(visual.lyricsStyle);
  const synced = $derived(lyrics.data?.synced ?? null);
  const lines = $derived(synced ? buildLines(synced, player.durationMs) : []);
  const plainLines = $derived(!synced && lyrics.data?.plain ? lyrics.data.plain.split("\n").map((l) => l.trim()) : []);
  const active = $derived(activeIndex(lines, pos.ms));
  const translation = $derived(lyrics.translation);

  let scroller = $state<HTMLElement>();
  let lineEls = $state<HTMLElement[]>([]);
  let freeUntil = 0;
  let placed = false;

  let searching = $state(false);
  let query = $state("");
  let hitAt = $state(0);
  let searchInput = $state<HTMLInputElement>();
  const texts = $derived(synced ? lines.map((l) => l.text) : plainLines);
  const hits = $derived.by(() => {
    const q = foldText(query.trim());
    if (!q) return [];
    return texts.flatMap((text, i) => (foldText(text).includes(q) ? [i] : []));
  });

  function scrollTo(i: number, smooth: boolean) {
    const el = lineEls[i];
    if (!el || !scroller) return;
    const top = el.offsetTop - scroller.clientHeight * 0.36 + el.offsetHeight / 2;
    scroller.scrollTo({ top, behavior: smooth ? "smooth" : "instant" });
  }

  // Keep the sung line in view, unless the user is looking around.
  $effect(() => {
    const i = active;
    if (i < 0 || Date.now() < freeUntil) return;
    tick().then(() => {
      scrollTo(i, placed);
      placed = true;
    });
  });

  function lookAround() {
    freeUntil = Date.now() + FREE_SCROLL_MS;
  }

  function jump(i: number) {
    const line = lines[i];
    if (!line) return;
    freeUntil = 0;
    player.seek(line.timeMs);
    if (!player.playing) player.play();
  }

  async function openSearch() {
    searching = true;
    await tick();
    searchInput?.focus();
    searchInput?.select();
  }

  function closeSearch() {
    searching = false;
    query = "";
    freeUntil = 0;
  }

  function showHit(step: number) {
    if (!hits.length) return;
    hitAt = (hitAt + step + hits.length) % hits.length;
    freeUntil = Date.now() + FREE_SCROLL_MS * 2;
    scrollTo(hits[hitAt], true);
  }

  $effect(() => {
    // A new search starts at its first hit.
    void query;
    hitAt = 0;
    if (hits.length) {
      freeUntil = Date.now() + FREE_SCROLL_MS * 2;
      tick().then(() => scrollTo(hits[0], true));
    }
  });

  function searchKey(e: KeyboardEvent) {
    if (e.key === "Enter") showHit(e.shiftKey ? -1 : 1);
    else if (e.key === "Escape") {
      e.stopPropagation();
      closeSearch();
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.ctrlKey && e.key.toLowerCase() === "f" && lyrics.status === "ready") {
      e.preventDefault();
      openSearch();
    }
  }

  function pickStyle(e: MouseEvent) {
    menu.show(
      e,
      LYRICS_STYLES.map((s) => ({
        label: t(`lyrics.style.${s}`),
        icon: s === settings.values.lyricsStyle ? "check" : "empty",
        action: () => settings.set("lyricsStyle", s),
      })),
    );
  }

  function desktopMenu(e: MouseEvent) {
    const on = settings.values.desktopLyrics;
    menu.show(e, [
      { label: t("desktopLyrics.show"), icon: on ? "check" : "empty", action: () => settings.set("desktopLyrics", !on) },
      {
        label: desktop.lyricsUnlocked ? t("desktopLyrics.lock") : t("desktopLyrics.unlock"),
        icon: desktop.lyricsUnlocked ? "lock" : "unlock",
        disabled: !on,
        action: () => desktop.setLyricsUnlocked(!desktop.lyricsUnlocked),
      },
    ]);
  }

  const credit = $derived.by(() => {
    const d = lyrics.data;
    if (!d) return "";
    if (d.source === "user") return t("lyrics.yours");
    if (d.source === "youtube") return d.credit ?? t("lyrics.fromYoutube");
    if (d.source === "genius") return t("lyrics.fromGenius");
    return t("lyrics.fromLrclib");
  });

  // A song change resets the view to the top of the new lyrics.
  $effect(() => {
    void lyrics.track?.id;
    placed = false;
    freeUntil = 0;
    closeSearch();
    scroller?.scrollTo({ top: 0 });
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="lyrics">
  <div class="tools">
    {#if searching}
      <label class="search glass">
        <input bind:this={searchInput} bind:value={query} placeholder={t("lyrics.search")} spellcheck="false" onkeydown={searchKey} />
        {#if query.trim()}<span class="count">{hits.length ? t("lyrics.hit", { at: hitAt + 1, n: hits.length }) : t("lyrics.noHits")}</span>{/if}
        <IconButton icon="chevronDown" label={t("lyrics.nextHit")} size={16} disabled={!hits.length} onclick={() => showHit(1)} />
        <IconButton icon="close" label={t("lyrics.closeSearch")} size={16} onclick={closeSearch} />
      </label>
    {:else}
      <IconButton icon="search" label={t("lyrics.searchTip")} disabled={lyrics.status !== "ready" || !!lyrics.data?.instrumental} onclick={openSearch} />
    {/if}
    <IconButton
      icon="languages"
      label={settings.values.translateLyrics ? t("lyrics.hideTranslation") : t("lyrics.translate")}
      active={settings.values.translateLyrics}
      disabled={lyrics.status !== "ready" || !!lyrics.data?.instrumental}
      onclick={() => lyrics.setTranslate(!settings.values.translateLyrics)} />
    <IconButton icon="type" label={t("lyrics.styleTip")} onclick={pickStyle} />
    <IconButton icon="overlay" label={t("desktopLyrics.title")} active={settings.values.desktopLyrics} onclick={desktopMenu} />
    <IconButton icon="pencil" label={t("lyrics.edit")} disabled={!player.current} onclick={onEdit} />
  </div>

  {#if lyrics.status === "loading"}
    <div class="state"><Spinner size={22} /></div>
  {:else if lyrics.status === "ready" && lyrics.data?.instrumental}
    <div class="state"><span class="big-note">♪</span><p>{t("lyrics.instrumental")}</p></div>
  {:else if lyrics.status === "ready"}
    <div class="scroller" class:plain={!synced} bind:this={scroller} onwheel={lookAround}>
      <div class="pad-top"></div>
      {#if lyrics.translating}<p class="note">{t("lyrics.translating")}</p>{/if}
      {#if lyrics.sameLanguage}<p class="note">{t("lyrics.sameLanguage")}</p>{/if}
      {#if synced}
        {#each lines as line, i (i)}
          {@const distance = active < 0 ? i + 1 : Math.abs(i - active)}
          <button
            class="line {style}"
            class:on={i === active}
            class:past={i < active}
            class:hit={hits[hitAt] === i && !!query.trim()}
            style:--d={Math.min(distance, 4)}
            bind:this={lineEls[i]}
            onclick={() => jump(i)}>
            <LyricText {line} pos={pos.ms} active={i === active} still={!player.playing} {style} {query} />
            {#if translation && line.index >= 0 && translation[line.index]}<span class="tr">{translation[line.index]}</span>{/if}
          </button>
        {/each}
      {:else}
        <p class="note">{t("lyrics.notSynced")}</p>
        {#each plainLines as text, i (i)}
          <p class="pline" class:hit={hits[hitAt] === i && !!query.trim()} bind:this={lineEls[i]}>
            <LyricText line={{ index: i, timeMs: 0, endMs: 0, text, words: [], gap: false }} pos={0} active={false} {style} {query} />
            {#if translation?.[i]}<span class="tr">{translation[i]}</span>{/if}
          </p>
        {/each}
      {/if}
      <p class="credit">{credit}</p>
      <div class="pad-bottom"></div>
    </div>
  {:else if lyrics.status === "none"}
    <div class="state">
      <p>{t("lyrics.none")}</p>
      <Button variant="secondary" icon="pencil" disabled={!player.current} onclick={onEdit}>{t("lyrics.add")}</Button>
    </div>
  {:else if lyrics.status === "offline"}
    <div class="state"><p>{t("lyrics.offline")}</p></div>
  {:else if lyrics.status === "error"}
    <div class="state">
      <p>{t("lyrics.failed")}</p>
      <Button variant="secondary" icon="refresh" onclick={() => lyrics.track && lyrics.load(lyrics.track)}>{t("lyrics.retry")}</Button>
    </div>
  {:else}
    <div class="state"><p>{t("lyrics.nothingPlaying")}</p></div>
  {/if}
</div>

<style>
  .lyrics {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .tools {
    position: absolute;
    top: 0;
    right: 0;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 34px;
    padding: 0 2px 0 12px;
    border-radius: var(--r-md);
    background: rgba(var(--panel), 0.5);
  }
  .search input {
    width: 180px;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font-size: var(--fs-sm);
  }
  .search input::placeholder {
    color: var(--text-3);
  }
  .count {
    padding: 0 6px;
    font-size: var(--fs-xs);
    color: var(--text-3);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .scroller {
    position: relative;
    flex: 1;
    overflow-y: auto;
    scrollbar-width: none;
    /* Lines melt away at the top and bottom edge instead of being cut off. */
    mask-image: linear-gradient(to bottom, transparent 0, #000 14%, #000 82%, transparent 100%);
  }
  .pad-top {
    height: 34vh;
  }
  .pad-bottom {
    height: 46vh;
  }
  .plain .pad-top {
    height: 56px;
  }
  .plain .pad-bottom {
    height: 80px;
  }
  .line {
    display: block;
    width: 100%;
    margin: 0;
    padding: 10px 14px;
    border: 0;
    border-radius: var(--r-md);
    background: transparent;
    color: var(--text);
    font-size: clamp(24px, 2.6vw, 34px);
    font-weight: 700;
    line-height: 1.25;
    letter-spacing: -0.01em;
    text-align: left;
    opacity: 0.32;
    filter: blur(calc((var(--d) - 1) * 0.5px));
    transition:
      opacity 400ms var(--ease),
      filter 400ms var(--ease),
      transform 400ms var(--ease);
    transform-origin: left center;
  }
  .line:hover {
    opacity: 0.7;
    filter: none;
  }
  .line.on {
    opacity: 1;
    filter: none;
  }
  /* Fade: whole lines fade in, slightly larger, with no word effects. */
  .line.fade.on {
    transform: scale(1.02);
  }
  .line.hit,
  .pline.hit {
    outline: 1px solid rgba(var(--ink), 0.25);
    outline-offset: -1px;
  }
  .tr {
    display: block;
    margin-top: 4px;
    font-size: 0.55em;
    font-weight: 500;
    color: var(--text-2);
    letter-spacing: 0;
  }
  .pline {
    margin: 0;
    padding: 4px 14px;
    border-radius: var(--r-sm);
    min-height: 1em;
    font-size: clamp(18px, 1.8vw, 24px);
    font-weight: 600;
    line-height: 1.4;
    color: rgba(var(--ink), 0.86);
  }
  .note {
    margin: 0 14px 12px;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
  .credit {
    margin: 28px 14px 0;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  .state {
    flex: 1;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 14px;
    color: var(--text-2);
    text-align: center;
  }
  .state p {
    margin: 0;
    font-size: var(--fs-lg);
  }
  .big-note {
    font-size: 48px;
    color: var(--text-3);
  }
  /* Blurred lyric edges are part of High quality only. */
  :global(html:not([data-quality="high"])) .line {
    filter: none;
  }
</style>
