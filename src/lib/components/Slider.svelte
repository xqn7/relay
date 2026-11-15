<script lang="ts">
  // Pointer-driven slider. `oninput` fires while dragging, `onchange` once on release.
  let {
    value,
    label,
    oninput,
    onchange,
    thin = false,
    valueText,
  }: {
    value: number;
    label: string;
    oninput?: (v: number) => void;
    onchange?: (v: number) => void;
    thin?: boolean;
    valueText?: string;
  } = $props();

  let track = $state<HTMLDivElement>();
  let dragging = $state<number | null>(null);
  const shown = $derived(dragging ?? Math.min(1, Math.max(0, value || 0)));

  function at(e: PointerEvent): number {
    const r = track!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    track!.setPointerCapture(e.pointerId);
    dragging = at(e);
    oninput?.(dragging);
  }

  function move(e: PointerEvent) {
    if (dragging == null) return;
    dragging = at(e);
    oninput?.(dragging);
  }

  function up(e: PointerEvent) {
    if (dragging == null) return;
    const v = at(e);
    dragging = null;
    onchange?.(v);
  }

  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 0.1 : 0.02;
    // Sliders that only listen while dragging still have to follow the arrow keys.
    const set = onchange ?? oninput;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") set?.(Math.min(1, shown + step));
    else if (e.key === "ArrowLeft" || e.key === "ArrowDown") set?.(Math.max(0, shown - step));
    else return;
    e.preventDefault();
  }
</script>

<div
  class="slider"
  class:thin
  class:dragging={dragging != null}
  bind:this={track}
  role="slider"
  tabindex="0"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={Math.round(shown * 100)}
  aria-valuetext={valueText}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={() => (dragging = null)}
  onkeydown={key}>
  <div class="rail"><div class="fill" style:width="{shown * 100}%"></div></div>
  <div class="thumb" style:left="{shown * 100}%"></div>
</div>

<style>
  .slider {
    position: relative;
    height: 18px;
    display: flex;
    align-items: center;
    touch-action: none;
    outline: none;
  }
  .rail {
    position: relative;
    width: 100%;
    height: 4px;
    border-radius: 4px;
    background: rgba(var(--ink), 0.16);
    overflow: hidden;
    transition: height var(--t-fast) var(--ease);
  }
  .fill {
    height: 100%;
    background: rgba(var(--ink), 0.9);
  }
  .thumb {
    position: absolute;
    top: 50%;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--solid);
    transform: translate(-50%, -50%) scale(0);
    transition: transform var(--t-fast) var(--ease);
  }
  .slider:hover .thumb,
  .slider:focus-visible .thumb,
  .dragging .thumb {
    transform: translate(-50%, -50%) scale(1);
  }
  .thin .rail {
    height: 3px;
  }
  .thin:hover .rail,
  .thin.dragging .rail {
    height: 5px;
  }
</style>
