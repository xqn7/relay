<!-- Settings → Appearance → Background, "Custom image": saved images, a live preview (drag it
     to choose which part shows), the look sliders, cover-color mixing and where it's used. -->
<script lang="ts">
  import BackgroundImage from "../../lib/backdrop/BackgroundImage.svelte";
  import { LOOK_RANGES, type Look, type LookNumber } from "../../lib/backdrop/look";
  import Button from "../../lib/components/Button.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import Slider from "../../lib/components/Slider.svelte";
  import Toggle from "../../lib/components/Toggle.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { pickImage } from "../../lib/profileActions";
  import { backgrounds } from "../../lib/stores/backgrounds.svelte";
  import { BACKGROUND_PLACES, settings, type BackgroundPlace } from "../../lib/stores/settings.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import SettingRow from "./SettingRow.svelte";

  const active = $derived(backgrounds.active);
  const look = $derived(active ? backgrounds.look(active) : null);
  let preview = $state<HTMLDivElement>();
  let drag: { x: number; y: number; lookX: number; lookY: number } | null = null;

  const sliders: { key: LookNumber; label: Key; text: (v: number) => string }[] = [
    { key: "blur", label: "bg.blur", text: (v) => `${Math.round(v)} px` },
    { key: "dim", label: "bg.dim", text: (v) => `${Math.round(v * 100)} %` },
    { key: "brightness", label: "bg.brightness", text: (v) => `${Math.round(v * 100)} %` },
    { key: "saturation", label: "bg.saturation", text: (v) => `${Math.round(v * 100)} %` },
    { key: "zoom", label: "bg.zoom", text: (v) => `${v.toFixed(1)}×` },
  ];
  const placeLabel: Record<BackgroundPlace, Key> = { app: "bg.place.app", fullscreen: "bg.place.fullscreen", idle: "bg.place.idle", mini: "bg.place.mini" };

  const toUnit = (key: LookNumber, v: number) => (v - LOOK_RANGES[key][0]) / (LOOK_RANGES[key][1] - LOOK_RANGES[key][0]);
  const fromUnit = (key: LookNumber, u: number) => LOOK_RANGES[key][0] + u * (LOOK_RANGES[key][1] - LOOK_RANGES[key][0]);

  function change(patch: Partial<Look>) {
    if (active && look) backgrounds.setLook(active.id, { ...look, ...patch });
  }

  function add() {
    pickImage("image/jpeg,image/png,image/webp,image/gif", (file) => backgrounds.add(file));
  }

  function togglePlace(place: BackgroundPlace, on: boolean) {
    const now = settings.values.backgroundWhere.filter((p) => p !== place);
    settings.set("backgroundWhere", on ? [...now, place] : now);
  }

  // Dragging the preview moves the visible part, like sliding the image under a window.
  function down(e: PointerEvent) {
    if (e.button !== 0 || !look) return;
    preview!.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, lookX: look.x, lookY: look.y };
  }

  function move(e: PointerEvent) {
    if (!drag || !preview) return;
    const r = preview.getBoundingClientRect();
    const clamp = (v: number) => Math.min(100, Math.max(0, v));
    change({ x: clamp(drag.lookX - ((e.clientX - drag.x) / r.width) * 100), y: clamp(drag.lookY - ((e.clientY - drag.y) / r.height) * 100) });
  }
</script>

<div class="gallery">
  {#each backgrounds.list as b (b.id)}
    <div class="thumb" class:on={active?.id === b.id}>
      <button class="pick" aria-label={t("bg.use")} onclick={() => backgrounds.use(b.id)}><img src={b.url} alt="" draggable="false" /></button>
      <button class="remove" aria-label={t("bg.remove")} title={t("bg.remove")} onclick={() => backgrounds.remove(b.id)}><Icon name="close" size={12} /></button>
    </div>
  {/each}
  <button class="thumb add" aria-label={t("bg.add")} title={t("bg.add")} onclick={add}><Icon name="plus" size={20} /></button>
</div>

{#if active && look}
  <div
    class="preview"
    bind:this={preview}
    role="presentation"
    title={t("bg.dragHint")}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={() => (drag = null)}>
    <BackgroundImage url={active.url} {look} colors={ui.coverColors} />
    <span class="sample">{t("bg.sample")}</span>
  </div>

  {#each sliders as s (s.key)}
    <div class="slider-row">
      <span class="label">{t(s.label)}</span>
      <Slider label={t(s.label)} value={toUnit(s.key, look[s.key])} oninput={(u) => change({ [s.key]: fromUnit(s.key, u) })} />
      <span class="value">{s.text(look[s.key])}</span>
    </div>
  {/each}

  <SettingRow label={t("bg.mix")} desc={t("bg.mixDesc")}>
    <Toggle label={t("bg.mix")} checked={look.mix} onchange={(v) => change({ mix: v })} />
  </SettingRow>
  <div class="where">{t("bg.where")}</div>
  {#each BACKGROUND_PLACES as place (place)}
    <SettingRow label={t(placeLabel[place])} desc={place === "mini" ? t("bg.place.miniDesc") : undefined}>
      <Toggle label={t(placeLabel[place])} checked={settings.values.backgroundWhere.includes(place)} onchange={(on) => togglePlace(place, on)} />
    </SettingRow>
  {/each}
  <div class="reset">
    <Button variant="secondary" icon="undo" onclick={() => settings.set("background", "cover")}>{t("bg.reset")}</Button>
  </div>
{:else}
  <p class="hint">{t("bg.none")}</p>
{/if}

<style>
  .gallery {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 4px 16px 14px;
  }
  .thumb {
    position: relative;
    width: 96px;
    height: 60px;
    border-radius: var(--r-md);
    overflow: hidden;
    outline: 2px solid transparent;
    outline-offset: 2px;
    transition: outline-color var(--t-fast) var(--ease);
  }
  .thumb.on {
    outline-color: rgba(255, 255, 255, 0.85);
  }
  .pick {
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    background: rgba(255, 255, 255, 0.06);
  }
  .pick img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .remove {
    position: absolute;
    top: 4px;
    right: 4px;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: rgba(10, 10, 11, 0.7);
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease);
  }
  .thumb:hover .remove,
  .remove:focus-visible {
    opacity: 1;
  }
  .add {
    display: grid;
    place-items: center;
    border: 1px dashed rgba(255, 255, 255, 0.2);
    background: transparent;
    color: var(--text-2);
  }
  .add:hover {
    color: var(--text);
    background: var(--hover);
  }
  .preview {
    position: relative;
    height: 180px;
    margin: 0 16px 10px;
    overflow: hidden;
    border-radius: var(--r-md);
    background: var(--bg);
    cursor: grab;
    touch-action: none;
  }
  .preview:active {
    cursor: grabbing;
  }
  .sample {
    position: absolute;
    left: 16px;
    bottom: 14px;
    font-size: var(--fs-xl);
    font-weight: 700;
    pointer-events: none;
  }
  .slider-row {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr) 56px;
    align-items: center;
    gap: var(--s3);
    padding: 6px 16px;
  }
  .label {
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .value {
    text-align: right;
    font-size: var(--fs-sm);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .where {
    padding: 10px 16px 0;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-3);
  }
  .reset {
    padding: 8px 16px 14px;
  }
  .hint {
    margin: 0;
    padding: 0 16px 14px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
</style>
