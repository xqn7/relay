<!-- Shown once after the welcome setup: downloads the helpers Relay needs (yt-dlp for YouTube
     playback, ffmpeg for video files, MP3 export and waveforms) with their progress, so no
     feature has to wait for them later. Closes by itself when both are ready; it can't be
     skipped, only left after a download failed (no internet). -->
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { api, type ToolProgress } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import { i18n, t } from "../../lib/i18n/index.svelte";

  let { onDone }: { onDone: () => void } = $props();

  const CLOSE_AFTER_READY_MS = 900;
  /** Set while the helpers aren't all there, so the screen comes back on the next start. */
  const PENDING_KEY = "relay.helpersPending";

  function remember(pending: boolean) {
    try {
      if (pending) localStorage.setItem(PENDING_KEY, "1");
      else localStorage.removeItem(PENDING_KEY);
    } catch {
      // Without storage the screen only shows after the welcome setup.
    }
  }
  const NAMES = { "yt-dlp": "prepare.ytdlp", ffmpeg: "prepare.ffmpeg" } as const;

  let tools = $state<ToolProgress[]>([]);
  // Progress that arrived before the first answer, so a "ready" in that gap isn't lost.
  const early = new Map<string, ToolProgress>();
  const allReady = $derived(tools.length > 0 && tools.every((x) => x.phase === "ready"));
  const anyFailed = $derived(tools.some((x) => x.phase === "failed"));

  const mb = (bytes: number) => (bytes / 1024 / 1024).toLocaleString(i18n.lang, { maximumFractionDigits: 1, minimumFractionDigits: 1 });
  const share = (x: ToolProgress) => (x.phase === "ready" || x.phase === "unpacking" ? 1 : x.total ? Math.min(1, x.done / x.total) : 0);

  function detail(x: ToolProgress): string {
    if (x.phase === "ready") return t("prepare.ready");
    if (x.phase === "failed") return t("prepare.failed");
    if (x.phase === "unpacking") return t("prepare.unpacking");
    if (x.phase === "downloading" && x.total) return t("prepare.of", { done: mb(x.done), total: mb(x.total) });
    return t("prepare.waiting");
  }

  function prepare() {
    api
      .toolsPrepare()
      .then((list) => {
        const first = tools.length === 0;
        tools = list.map((x) => early.get(x.tool) ?? x);
        early.clear();
        // Everything already there (a second account on this PC): nothing to show.
        if (first && tools.every((x) => x.phase === "ready")) {
          remember(false);
          onDone();
        }
      })
      .catch((e) => {
        console.error("helper downloads failed to start", e);
        // Nothing to show progress for; the helpers still download when a feature needs them.
        if (!tools.length) onDone();
      });
  }

  onMount(() => {
    remember(true);
    prepare();
    const off = listen<ToolProgress>("tool-progress", ({ payload }) => {
      if (tools.length) tools = tools.map((x) => (x.tool === payload.tool ? payload : x));
      else early.set(payload.tool, payload);
    });
    return () => {
      off.then((f) => f());
    };
  });

  $effect(() => {
    if (!allReady) return;
    remember(false);
    const id = setTimeout(onDone, CLOSE_AFTER_READY_MS);
    return () => clearTimeout(id);
  });
</script>

{#if tools.length}
<div class="prepare" role="dialog" aria-modal="true" aria-label={t("prepare.title")} transition:fade={{ duration: 220 }}>
  <div class="panel glass" in:fly={{ y: 14, duration: 260 }}>
    <div class="mark">
      {#if allReady}<span class="done"><Icon name="check" size={26} /></span>{:else}<Spinner size={30} />{/if}
    </div>
    <h1>{allReady ? t("prepare.allSet") : t("prepare.title")}</h1>
    <p class="sub">{t("prepare.sub")}</p>

    <div class="tools">
      {#each tools as x (x.tool)}
        <div class="tool">
          <div class="line">
            <span class="name">{t(NAMES[x.tool])}</span>
            <span class="detail">{detail(x)}</span>
          </div>
          <div class="bar" class:failed={x.phase === "failed"}><span style:width="{share(x) * 100}%"></span></div>
        </div>
      {/each}
    </div>

    <footer>
      {#if anyFailed}
        <Button variant="secondary" icon="refresh" onclick={prepare}>{t("prepare.retry")}</Button>
      {/if}
      {#if allReady}
        <Button variant="primary" onclick={onDone}>{t("welcome.finish")}</Button>
      {:else if anyFailed}
        <!-- Only without internet (a failed download) can Relay be used before the helpers are
             there; they download when a feature first needs them. -->
        <Button variant="ghost" onclick={onDone}>{t("prepare.background")}</Button>
      {/if}
    </footer>
  </div>
</div>
{/if}

<style>
  .prepare {
    position: fixed;
    inset: 0;
    z-index: 65;
    display: grid;
    place-items: center;
    background: rgba(var(--scrim), 0.55);
    backdrop-filter: blur(18px);
  }
  .panel {
    width: min(460px, calc(100vw - 48px));
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 30px 28px 22px;
    border-radius: 16px;
    background: rgba(var(--panel), 0.82);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.4);
    text-align: center;
  }
  .mark {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    margin-bottom: 4px;
    color: var(--text);
  }
  .done {
    display: grid;
    place-items: center;
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background: var(--glass-raised);
  }
  h1 {
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .sub {
    margin: 0 0 8px;
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--text-2);
  }
  .tools {
    display: grid;
    gap: 14px;
    width: 100%;
    text-align: left;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 6px;
    font-size: var(--fs-sm);
  }
  .name {
    font-weight: 600;
  }
  .detail {
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .bar {
    height: 4px;
    overflow: hidden;
    border-radius: 2px;
    background: rgba(var(--ink), 0.12);
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--solid);
    transition: width var(--t) var(--ease);
  }
  .bar.failed span {
    background: rgba(var(--ink), 0.35);
  }
  footer {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }
</style>
