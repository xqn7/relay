<!-- Lyrics editor: paste the text, then tap each line as it starts while the song plays, and
     fine-tune the times. Saved on this PC and used instead of the looked-up lyrics. -->
<script lang="ts">
  import { tick } from "svelte";
  import { api, type LyricLine } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import IconButton from "../../lib/components/IconButton.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { fail } from "../../lib/stores/library.svelte";
  import { lyrics } from "../../lib/stores/lyrics.svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  let { onClose }: { onClose: () => void } = $props();

  interface Row {
    text: string;
    timeMs: number | null;
  }

  const NUDGE_MS = 100;
  const PREVIEW_LEAD_MS = 2000;

  // The editor belongs to the song it was opened for.
  const track = player.current?.track ?? null;
  const start = lyrics.track?.id === track?.id ? lyrics.data : null;
  const startRows: Row[] = start?.synced ? start.synced.filter((l) => l.text.trim()).map((l) => ({ text: l.text.trim(), timeMs: l.timeMs })) : [];
  let rows = $state<Row[]>(startRows);
  let text = $state(start?.synced ? startRows.map((r) => r.text).join("\n") : (start?.plain ?? ""));
  let mode = $state<"text" | "sync">(start?.synced ? "sync" : "text");
  let at = $state(0);
  let error = $state("");
  let busy = $state(false);
  let list = $state<HTMLElement>();

  // When the song ends while editing (likely right after the last tap), the work stays; only
  // tapping and play-from wait until this song plays again.
  const sameSong = $derived(player.current?.track.id === track?.id);

  /** Rows from the text; lines that didn't change keep their times. */
  function rowsFromText(): Row[] {
    const lines = text.split("\n").map((l) => l.trim()).filter(Boolean);
    return lines.map((line, i) => ({ text: line, timeMs: rows[i]?.text === line ? rows[i].timeMs : null }));
  }

  function setMode(next: "text" | "sync") {
    error = "";
    if (next === "sync") {
      rows = rowsFromText();
      const firstOpen = rows.findIndex((r) => r.timeMs == null);
      at = firstOpen < 0 ? rows.length : firstOpen;
    }
    mode = next;
  }

  async function show(i: number) {
    await tick();
    list?.querySelectorAll(".row")[Math.min(i, rows.length - 1)]?.scrollIntoView({ block: "center", behavior: "smooth" });
  }

  function tap() {
    if (at >= rows.length || !sameSong) return;
    rows[at].timeMs = Math.round(player.livePositionMs());
    at += 1;
    error = "";
    show(at);
  }

  function undoTap() {
    if (at === 0) return;
    at -= 1;
    rows[at].timeMs = null;
    show(at);
  }

  function startOver() {
    for (const r of rows) r.timeMs = null;
    at = 0;
    player.seek(0);
    if (!player.playing) player.play();
    show(0);
  }

  function nudge(i: number, delta: number) {
    const time = rows[i].timeMs;
    if (time != null) rows[i].timeMs = Math.max(0, time + delta);
  }

  function playFrom(i: number) {
    const time = rows[i].timeMs;
    if (time == null || !sameSong) return;
    player.seek(Math.max(0, time - PREVIEW_LEAD_MS));
    if (!player.playing) player.play();
  }

  function fmt(ms: number | null): string {
    if (ms == null) return "–:––.––";
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}.${String(Math.floor((ms % 1000) / 10)).padStart(2, "0")}`;
  }

  /** Timed lines must run forward; returns the first line that doesn't, 1-based. */
  function outOfOrder(timed: Row[]): number {
    for (let i = 1; i < timed.length; i++) if ((timed[i].timeMs ?? 0) < (timed[i - 1].timeMs ?? 0)) return i + 1;
    return 0;
  }

  async function save() {
    if (!track) return;
    const current = mode === "text" ? rowsFromText() : rows;
    const timed = current.filter((r) => r.timeMs != null);
    error = "";
    if (current.length === 0) return (error = t("editor.empty"));
    if (timed.length > 0 && timed.length < current.length) return (error = t("editor.tapAll"));
    const wrong = outOfOrder(timed);
    if (wrong) return (error = t("editor.order", { n: wrong }));
    const lines: LyricLine[] = timed.map((r) => ({ timeMs: r.timeMs!, text: r.text }));
    busy = true;
    try {
      const saved = await api.lyricsSave(track, lines, lines.length ? "" : current.map((r) => r.text).join("\n"));
      if (lyrics.track?.id === track.id) lyrics.apply(saved);
      ui.toast(t("editor.saved"), "check");
      onClose();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  async function reset() {
    if (!track) return;
    busy = true;
    try {
      const found = await api.lyricsReset(track);
      if (lyrics.track?.id === track.id) lyrics.apply(found);
      onClose();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (mode !== "sync" || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLInputElement) return;
    if (e.key === "Enter") {
      e.preventDefault();
      tap();
    } else if (e.key === "Backspace" || e.key === "ArrowUp") {
      e.preventDefault();
      undoTap();
    } else if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="editor">
  <header>
    <h3>{t("editor.title")}</h3>
    <Segmented
      label={t("editor.title")}
      value={mode}
      options={[
        { value: "text", label: t("editor.text") },
        { value: "sync", label: t("editor.sync") },
      ]}
      onchange={setMode} />
    <span class="grow"></span>
    {#if start?.source === "user"}<Button variant="ghost" icon="undo" disabled={busy} onclick={reset}>{t("editor.reset")}</Button>{/if}
    <Button variant="ghost" onclick={onClose}>{t("editor.cancel")}</Button>
    <Button variant="primary" {busy} onclick={save}>{t("editor.save")}</Button>
  </header>

  {#if mode === "text"}
    <p class="hint">{t("editor.textHint")}</p>
    <textarea bind:value={text} spellcheck="false" placeholder={t("editor.placeholder")}></textarea>
  {:else if rows.length === 0}
    <p class="hint">{t("editor.noLines")}</p>
  {:else}
    <p class="hint">{sameSong ? t("editor.syncHint") : t("editor.otherSong", { title: track?.title ?? "" })}</p>
    <div class="rows" bind:this={list}>
      {#each rows as row, i (i)}
        <div class="row" class:next={i === at} class:timed={row.timeMs != null}>
          <button class="time" disabled={row.timeMs == null} title={t("editor.playFrom")} onclick={() => playFrom(i)}>{fmt(row.timeMs)}</button>
          <span class="nudge">
            <IconButton icon="minimize" size={14} label={t("editor.earlier")} disabled={row.timeMs == null} onclick={() => nudge(i, -NUDGE_MS)} />
            <IconButton icon="plus" size={14} label={t("editor.later")} disabled={row.timeMs == null} onclick={() => nudge(i, NUDGE_MS)} />
          </span>
          <button class="text" title={t("editor.tapFromHere")} onclick={() => (at = i)}>{row.text}</button>
        </div>
      {/each}
    </div>
    <footer>
      <Button variant="ghost" icon="refresh" onclick={startOver}>{t("editor.startOver")}</Button>
      <Button variant="ghost" icon="undo" disabled={at === 0} onclick={undoTap}>{t("editor.undo")}</Button>
      <span class="grow"></span>
      <span class="progress">{t("editor.progress", { done: Math.min(at, rows.length), n: rows.length })}</span>
      <Button variant="primary" disabled={at >= rows.length || !sameSong} onclick={tap}>{t("editor.tap")}</Button>
    </footer>
  {/if}
  {#if error}<p class="error"><Icon name="alert" size={14} />{error}</p>{/if}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
    min-height: 0;
  }
  header,
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  h3 {
    margin: 0 8px 0 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .grow {
    flex: 1;
  }
  .hint {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  textarea {
    flex: 1;
    min-height: 0;
    resize: none;
    padding: 14px 16px;
    border: 1px solid var(--hairline);
    border-radius: var(--r-lg);
    outline: none;
    background: rgba(0, 0, 0, 0.32);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-lg);
    line-height: 1.6;
  }
  textarea:focus {
    border-color: rgba(255, 255, 255, 0.24);
  }
  .rows {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px;
    border-radius: var(--r-lg);
    background: rgba(0, 0, 0, 0.28);
  }
  .row {
    display: grid;
    grid-template-columns: 76px auto 1fr;
    align-items: center;
    gap: 6px;
    padding: 2px 6px;
    border-radius: var(--r-md);
  }
  .row.next {
    background: var(--selected);
  }
  .time {
    padding: 4px 6px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-3);
    font: inherit;
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
    text-align: left;
  }
  .timed .time {
    color: var(--text);
  }
  .time:hover:not(:disabled) {
    background: var(--hover);
  }
  .nudge {
    display: flex;
  }
  .text {
    min-width: 0;
    padding: 8px 6px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-lg);
    font-weight: 600;
    text-align: left;
  }
  .timed .text,
  .next .text {
    color: var(--text);
  }
  .progress {
    font-size: var(--fs-sm);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text);
  }
</style>
