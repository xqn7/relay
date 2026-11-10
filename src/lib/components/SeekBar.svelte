<!-- The song's progress bar: a plain line or the song's waveform, the trimmed-off parts
     dimmed, and the A–B markers with the part between them highlighted. Drag anywhere to
     seek, drag a marker to move it. -->
<script lang="ts">
  import { formatTime } from "../api";
  import { t } from "../i18n/index.svelte";
  import { library } from "../stores/library.svelte";
  import { player } from "../stores/player.svelte";
  import { settings } from "../stores/settings.svelte";
  import { waveform } from "../stores/waveform.svelte";

  /** `tall`: the fullscreen player's bigger bar. */
  let { tall = false }: { tall?: boolean } = $props();

  let el = $state<HTMLDivElement>();
  let width = $state(0);
  let seeking = $state<number | null>(null);
  let dragging = $state<"a" | "b" | null>(null);

  const dur = $derived(player.durationMs);
  const frac = (ms: number) => (dur > 0 ? Math.min(1, Math.max(0, ms / dur)) : 0);
  const progress = $derived(seeking ?? frac(player.positionMs));
  const track = $derived(player.current?.track ?? null);
  const trim = $derived(track ? library.trims[track.id] : undefined);
  const marks = $derived(player.markers);
  const bars = $derived(settings.values.waveform && track && waveform.trackId === track.id ? waveform.bars : null);
  const height = $derived(tall ? 40 : 22);

  /** The stored bars merged down to what fits: one bar per 3 px. */
  const shape = $derived.by(() => {
    if (!bars || width <= 0) return null;
    const count = Math.max(20, Math.min(bars.length, Math.floor(width / 3)));
    const per = bars.length / count;
    let d = "";
    for (let i = 0; i < count; i++) {
      let peak = 0;
      for (let j = Math.floor(i * per); j < Math.floor((i + 1) * per); j++) peak = Math.max(peak, bars[j]);
      // A floor keeps quiet parts visible; mirrored around the middle line.
      const h = Math.max(6, (peak / 255) * 100);
      d += `M${i + 0.18} ${50 - h / 2}h0.64v${h}h-0.64z`;
    }
    return { d, count };
  });

  function at(e: PointerEvent): number {
    const r = el!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }

  function down(e: PointerEvent) {
    if (e.button !== 0 || !track || dur <= 0) return;
    el!.setPointerCapture(e.pointerId);
    const handle = (e.target as HTMLElement).closest<HTMLElement>("[data-marker]")?.dataset.marker;
    if (handle === "a" || handle === "b") dragging = handle;
    else seeking = at(e);
  }

  function move(e: PointerEvent) {
    if (dragging && marks) {
      const ms = at(e) * dur;
      if (dragging === "a") player.setMarkers(ms, marks.bMs);
      else player.setMarkers(marks.aMs, ms);
    } else if (seeking != null) seeking = at(e);
  }

  function up(e: PointerEvent) {
    if (seeking != null) player.seek(at(e) * dur);
    seeking = null;
    dragging = null;
  }

  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 15_000 : 5_000;
    if (e.key === "ArrowRight") player.seek(player.positionMs + step);
    else if (e.key === "ArrowLeft") player.seek(player.positionMs - step);
    else return;
    e.preventDefault();
  }
</script>

<div
  class="seekbar"
  class:tall
  class:wave={!!shape}
  class:active={seeking != null || dragging != null}
  style:--h="{shape ? height : 18}px"
  bind:this={el}
  bind:clientWidth={width}
  role="slider"
  tabindex="0"
  aria-label={t("player.seek")}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={Math.round(progress * 100)}
  aria-valuetext={formatTime(player.positionMs)}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={() => {
    seeking = null;
    dragging = null;
  }}
  onkeydown={key}>
  {#if shape}
    <svg viewBox="0 0 {shape.count} 100" preserveAspectRatio="none" aria-hidden="true">
      <defs>
        <clipPath id="played-{tall ? 't' : 's'}"><rect width={progress * shape.count} height="100" /></clipPath>
      </defs>
      <path class="rest" d={shape.d} />
      <path class="played" d={shape.d} clip-path="url(#played-{tall ? 't' : 's'})" />
    </svg>
  {:else}
    <div class="rail"><div class="fill" style:width="{progress * 100}%"></div></div>
    <div class="thumb" style:left="{progress * 100}%"></div>
  {/if}

  {#if trim && !marks}
    {#if trim.startMs > 0}<div class="cut" style:left="0" style:width="{frac(trim.startMs) * 100}%"></div>{/if}
    {#if trim.endMs != null}<div class="cut" style:left="{frac(trim.endMs) * 100}%" style:right="0"></div>{/if}
  {/if}

  {#if marks}
    <div class="range" class:looping={player.looping} style:left="{frac(marks.aMs) * 100}%" style:width="{(frac(marks.bMs) - frac(marks.aMs)) * 100}%"></div>
    <div class="marker" data-marker="a" style:left="{frac(marks.aMs) * 100}%" title={t("range.dragA")}><span>A</span></div>
    <div class="marker" data-marker="b" style:left="{frac(marks.bMs) * 100}%" title={t("range.dragB")}><span>B</span></div>
  {/if}
</div>

<style>
  .seekbar {
    position: relative;
    height: var(--h);
    display: flex;
    align-items: center;
    touch-action: none;
    outline: none;
    cursor: pointer;
  }
  svg {
    width: 100%;
    height: 100%;
    display: block;
  }
  .rest {
    fill: rgba(255, 255, 255, 0.24);
  }
  .played {
    fill: rgba(255, 255, 255, 0.92);
  }
  .rail {
    position: relative;
    width: 100%;
    height: 3px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.16);
    overflow: hidden;
    transition: height var(--t-fast) var(--ease);
  }
  .tall .rail {
    height: 4px;
  }
  .seekbar:hover .rail,
  .active .rail {
    height: 5px;
  }
  .fill {
    height: 100%;
    background: rgba(255, 255, 255, 0.9);
  }
  .thumb {
    position: absolute;
    top: 50%;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #fff;
    transform: translate(-50%, -50%) scale(0);
    transition: transform var(--t-fast) var(--ease);
    pointer-events: none;
  }
  .seekbar:hover .thumb,
  .seekbar:focus-visible .thumb,
  .active .thumb {
    transform: translate(-50%, -50%) scale(1);
  }
  /* Trimmed-off parts: still seekable, but clearly not part of the song any more. */
  .cut {
    position: absolute;
    top: 50%;
    height: 5px;
    transform: translateY(-50%);
    background: rgba(12, 12, 14, 0.7);
    pointer-events: none;
  }
  .wave .cut {
    top: 0;
    height: 100%;
    transform: none;
  }
  .range {
    position: absolute;
    top: 50%;
    height: max(14px, 70%);
    transform: translateY(-50%);
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.1);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.18);
    pointer-events: none;
    transition: background var(--t-fast) var(--ease);
  }
  .range.looping {
    background: rgba(255, 255, 255, 0.18);
  }
  .marker {
    position: absolute;
    top: 50%;
    width: 14px;
    height: calc(max(14px, 70%) + 10px);
    transform: translate(-50%, -50%);
    cursor: ew-resize;
    z-index: 1;
  }
  /* A thin white line, with its letter riding above it. */
  .marker::before {
    content: "";
    position: absolute;
    left: 6px;
    top: 0;
    bottom: 0;
    width: 2px;
    border-radius: 1px;
    background: #fff;
  }
  .marker span {
    position: absolute;
    left: 50%;
    bottom: 100%;
    transform: translateX(-50%);
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
    padding-bottom: 2px;
    color: var(--text);
  }
</style>
