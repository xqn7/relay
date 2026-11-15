<!-- The 10-band equalizer: on/off, preset menu, one vertical slider per band, and saving
     the current curve as a named preset. Used in the Sound panel and in Settings. -->
<script lang="ts">
  import { BANDS, BUILT_IN_IDS, bandLabel, MAX_DB } from "../audio/eq";
  import { t } from "../i18n/index.svelte";
  import { dialogs } from "../stores/dialogs.svelte";
  import { menu, type MenuItem } from "../stores/menu.svelte";
  import { settings } from "../stores/settings.svelte";
  import { sound } from "../stores/sound.svelte";
  import Icon from "./Icon.svelte";
  import Toggle from "./Toggle.svelte";

  /** `compact`: shorter sliders for the popover. */
  let { compact = false }: { compact?: boolean } = $props();

  const eq = $derived(sound.eq);
  let dragging = $state<number | null>(null);

  function presetMenu(e: MouseEvent) {
    const mark = (id: string) => (eq.preset === id ? "check" : "empty");
    const items: MenuItem[] = BUILT_IN_IDS.map((id) => ({ label: sound.presetLabel(id), icon: mark(id), action: () => sound.choosePreset(id) }));
    settings.values.eqPresets.forEach((p, i) =>
      items.push({ label: p.name, icon: mark(`user:${p.name}`), divider: i === 0, action: () => sound.choosePreset(`user:${p.name}`) }),
    );
    items.push({ label: t("eq.savePreset"), icon: "plus", divider: true, action: savePrompt });
    if (eq.preset.startsWith("user:")) {
      const name = eq.preset.slice(5);
      items.push({ label: t("eq.deletePreset", { name }), icon: "trash", action: () => sound.deletePreset(name) });
    }
    // Opened under the button rather than at the pointer; the click itself must not reach the
    // window, where it would close the menu again.
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu.show(new MouseEvent("contextmenu", { clientX: r.left, clientY: r.bottom + 4 }), items);
  }

  function savePrompt() {
    dialogs.open({
      kind: "prompt",
      title: t("eq.savePreset"),
      label: t("eq.presetName"),
      value: "",
      confirm: t("settings.save"),
      onConfirm: (name) => sound.savePreset(name),
    });
  }

  function dbAt(e: PointerEvent, el: HTMLElement): number {
    const r = el.getBoundingClientRect();
    const f = 1 - Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    return (f * 2 - 1) * MAX_DB;
  }

  function down(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    dragging = i;
    sound.setBand(i, dbAt(e, el));
  }

  function move(e: PointerEvent, i: number) {
    if (dragging === i) sound.setBand(i, dbAt(e, e.currentTarget as HTMLElement));
  }

  function key(e: KeyboardEvent, i: number) {
    const step = e.shiftKey ? 3 : 0.5;
    const g = eq.gains[i];
    if (e.key === "ArrowUp" || e.key === "ArrowRight") sound.setBand(i, Math.min(MAX_DB, g + step));
    else if (e.key === "ArrowDown" || e.key === "ArrowLeft") sound.setBand(i, Math.max(-MAX_DB, g - step));
    else if (e.key === "0" || e.key === "Delete") sound.setBand(i, 0);
    else return;
    e.preventDefault();
  }

  const pct = (db: number) => ((db + MAX_DB) / (2 * MAX_DB)) * 100;
</script>

<div class="eq" class:compact class:off={!eq.on}>
  <div class="head">
    <Toggle label={t("eq.title")} checked={eq.on} onchange={(on) => sound.setEqOn(on)} />
    <span class="name">{t("eq.title")}</span>
    <button class="preset" onclick={presetMenu}>
      {sound.presetLabel(eq.preset)}
      <Icon name="chevronDown" size={14} />
    </button>
  </div>
  <div class="bands">
    {#each BANDS as hz, i (hz)}
      {@const g = eq.gains[i]}
      <div class="band">
        <span class="value">{g > 0 ? "+" : ""}{g}</span>
        <div
          class="rail"
          class:active={dragging === i}
          role="slider"
          tabindex="0"
          aria-label={t("eq.band", { hz: bandLabel(hz) })}
          aria-orientation="vertical"
          aria-valuemin={-MAX_DB}
          aria-valuemax={MAX_DB}
          aria-valuenow={g}
          aria-valuetext="{g} dB"
          onpointerdown={(e) => down(e, i)}
          onpointermove={(e) => move(e, i)}
          onpointerup={() => (dragging = null)}
          onpointercancel={() => (dragging = null)}
          ondblclick={() => sound.setBand(i, 0)}
          onkeydown={(e) => key(e, i)}>
          <div class="line"></div>
          <div class="fill" style:bottom="{Math.min(50, pct(g))}%" style:top="{100 - Math.max(50, pct(g))}%"></div>
          <div class="knob" style:bottom="{pct(g)}%"></div>
        </div>
        <span class="hz">{bandLabel(hz)}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: var(--s3);
  }
  .name {
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .preset {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 5px 8px 5px 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: rgba(var(--ink), 0.07);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
    transition: background var(--t-fast) var(--ease);
  }
  .preset:hover {
    background: rgba(var(--ink), 0.12);
  }
  .bands {
    display: grid;
    grid-template-columns: repeat(10, 1fr);
    margin-top: var(--s4);
    transition: opacity var(--t-fast) var(--ease);
  }
  .off .bands {
    opacity: 0.45;
  }
  .band {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .value,
  .hz {
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .rail {
    position: relative;
    width: 24px;
    height: 120px;
    cursor: pointer;
    touch-action: none;
    outline: none;
  }
  .compact .rail {
    height: 92px;
  }
  .line {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    width: 3px;
    transform: translateX(-50%);
    border-radius: 2px;
    background: rgba(var(--ink), 0.14);
  }
  /* From the 0 dB middle to the value, so boosts and cuts read at a glance. */
  .fill {
    position: absolute;
    left: 50%;
    width: 3px;
    transform: translateX(-50%);
    border-radius: 2px;
    background: rgba(var(--ink), 0.85);
  }
  .knob {
    position: absolute;
    left: 50%;
    width: 12px;
    height: 12px;
    transform: translate(-50%, 50%);
    border-radius: 50%;
    background: var(--solid);
    transition: transform var(--t-fast) var(--ease);
  }
  .rail:hover .knob,
  .rail:focus-visible .knob,
  .rail.active .knob {
    transform: translate(-50%, 50%) scale(1.2);
  }
</style>
