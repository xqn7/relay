<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade, scale } from "svelte/transition";
  import { t } from "../i18n/index.svelte";
  import IconButton from "./IconButton.svelte";

  let {
    title,
    sub,
    width = 480,
    onclose,
    children,
    footer,
  }: { title: string; sub?: string; width?: number; onclose: () => void; children: Snippet; footer?: Snippet } = $props();

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="scrim" role="presentation" onpointerdown={onclose} transition:fade={{ duration: 150 }}></div>
<div class="dialog glass" role="dialog" aria-modal="true" aria-label={title} style:width="{width}px" transition:scale={{ start: 0.97, duration: 170 }}>
  <header>
    <div class="titles">
      <h2>{title}</h2>
      {#if sub}<p>{sub}</p>{/if}
    </div>
    <IconButton icon="close" label={t("win.close")} onclick={onclose} />
  </header>
  <div class="body">{@render children()}</div>
  {#if footer}<footer>{@render footer()}</footer>{/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 70;
    background: rgba(5, 5, 6, 0.4);
    backdrop-filter: blur(8px);
  }
  .dialog {
    position: fixed;
    left: 50%;
    top: 50%;
    z-index: 71;
    max-width: calc(100vw - 48px);
    max-height: calc(100vh - 96px);
    display: flex;
    flex-direction: column;
    translate: -50% -50%;
    border-radius: var(--r-lg);
    background: rgba(22, 22, 25, 0.82);
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.4);
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--s3);
    padding: 18px 12px 4px 22px;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 700;
  }
  p {
    margin: 4px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
    line-height: 1.45;
  }
  .body {
    min-height: 0;
    overflow-y: auto;
    padding: 12px 22px 18px;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--s2);
    padding: 12px 22px 18px;
    border-top: 1px solid var(--hairline);
  }
</style>
