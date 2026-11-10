<!-- Crop a picked image: drag to choose the part, zoom with the slider or the wheel. The
     result is drawn into a canvas and saved as a JPEG; the original file is never changed. -->
<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../../lib/components/Button.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Slider from "../../lib/components/Slider.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs, type Dialog as D } from "../../lib/stores/dialogs.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  let { d }: { d: Extract<D, { kind: "crop" }> } = $props();

  const FRAME_W = 440;
  const MAX_ZOOM = 4;
  const frameH = $derived(Math.round(FRAME_W / d.aspect));

  let img = $state<HTMLImageElement | null>(null);
  let url = "";
  let zoom = $state(1);
  /** Top-left of the image inside the frame, in frame pixels. */
  let x = $state(0);
  let y = $state(0);
  let drag: { px: number; py: number; x: number; y: number } | null = null;
  let busy = $state(false);

  // At zoom 1 the image just covers the frame.
  const base = $derived(img ? Math.max(FRAME_W / img.naturalWidth, frameH / img.naturalHeight) : 1);
  const w = $derived(img ? img.naturalWidth * base * zoom : 0);
  const h = $derived(img ? img.naturalHeight * base * zoom : 0);

  function clamp() {
    x = Math.min(0, Math.max(FRAME_W - w, x));
    y = Math.min(0, Math.max(frameH - h, y));
  }

  /** Zooms around the frame's center, so the chosen part stays in view. */
  function setZoom(z: number) {
    const next = Math.min(MAX_ZOOM, Math.max(1, z));
    const cx = (FRAME_W / 2 - x) / w;
    const cy = (frameH / 2 - y) / h;
    zoom = next;
    x = FRAME_W / 2 - cx * w;
    y = frameH / 2 - cy * h;
    clamp();
  }

  onMount(() => {
    url = URL.createObjectURL(d.file);
    const el = new Image();
    el.onload = () => {
      img = el;
      x = (FRAME_W - w) / 2;
      y = (frameH - h) / 2;
      clamp();
    };
    el.onerror = () => {
      dialogs.close();
      ui.toast(t("bg.badFile"), "alert");
    };
    el.src = url;
    return () => URL.revokeObjectURL(url);
  });

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { px: e.clientX, py: e.clientY, x, y };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    x = drag.x + e.clientX - drag.px;
    y = drag.y + e.clientY - drag.py;
    clamp();
  }

  async function save() {
    if (!img) return;
    busy = true;
    const scale = d.outWidth / FRAME_W;
    const canvas = document.createElement("canvas");
    canvas.width = d.outWidth;
    canvas.height = Math.round(frameH * scale);
    const ctx = canvas.getContext("2d")!;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(img, x * scale, y * scale, w * scale, h * scale);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/jpeg", 0.9));
    if (!blob) {
      busy = false;
      ui.toast(t("error.generic"), "alert");
      return;
    }
    const done = d.onDone;
    try {
      await done(new Uint8Array(await blob.arrayBuffer()));
      dialogs.close();
    } catch {
      ui.toast(t("error.generic"), "alert");
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title={d.title} sub={t("crop.hint")} width={FRAME_W + 48} onclose={() => dialogs.close()}>
  <div
    class="frame"
    class:round={d.round}
    style:width="{FRAME_W}px"
    style:height="{frameH}px"
    role="presentation"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={() => (drag = null)}
    onwheel={(e) => {
      e.preventDefault();
      setZoom(zoom * (e.deltaY < 0 ? 1.08 : 1 / 1.08));
    }}>
    {#if img}
      <img src={img.src} alt="" draggable="false" style:left="{x}px" style:top="{y}px" style:width="{w}px" style:height="{h}px" />
    {/if}
  </div>
  <div class="zoom">
    <Slider label={t("crop.zoom")} value={(zoom - 1) / (MAX_ZOOM - 1)} oninput={(v) => setZoom(1 + v * (MAX_ZOOM - 1))} />
  </div>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => dialogs.close()}>{t("dialog.cancel")}</Button>
    <Button variant="primary" busy={busy} disabled={!img} onclick={save}>{t("dialog.save")}</Button>
  {/snippet}
</Dialog>

<style>
  .frame {
    position: relative;
    overflow: hidden;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.05);
    cursor: grab;
    touch-action: none;
  }
  .frame:active {
    cursor: grabbing;
  }
  /* A round picture: the circle shows what will be visible, the corners stay faintly visible. */
  .frame.round::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: 50%;
    box-shadow: 0 0 0 999px rgba(10, 10, 11, 0.6);
    pointer-events: none;
  }
  img {
    position: absolute;
    max-width: none;
    pointer-events: none;
  }
  .zoom {
    margin-top: var(--s4);
  }
</style>
