<!-- The Sound popover from the player bar: output device, equalizer, speed and pitch. -->
<script lang="ts">
  import { fly } from "svelte/transition";
  import DevicePicker from "../lib/components/DevicePicker.svelte";
  import EqEditor from "../lib/components/EqEditor.svelte";
  import SpeedPitch from "../lib/components/SpeedPitch.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { menu } from "../lib/stores/menu.svelte";

  let { onClose }: { onClose: () => void } = $props();
  let el = $state<HTMLDivElement>();

  // Closes on a click outside, but not on clicks in (or around) one of its own menus: the
  // menu closes itself on that same click, before this sees it.
  function outside(e: PointerEvent) {
    if (menu.open || !el || el.contains(e.target as Node)) return;
    if ((e.target as HTMLElement).closest("[data-sound-toggle], .menu, .catcher")) return;
    onClose();
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === "Escape" && !menu.open && onClose()} />

<div class="sound-panel glass" bind:this={el} role="dialog" aria-label={t("sound.title")} transition:fly={{ y: 8, duration: 160 }}>
  <div class="device">
    <span class="label">{t("sound.output")}</span>
    <DevicePicker />
  </div>
  <div class="part"><EqEditor compact /></div>
  <div class="part"><SpeedPitch /></div>
</div>

<style>
  .sound-panel {
    position: fixed;
    right: 10px;
    bottom: calc(var(--player-h) + 20px);
    z-index: 41;
    width: 400px;
    padding: var(--s4);
    border-radius: var(--r-lg);
  }
  .device {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s3);
    min-width: 0;
  }
  .label {
    flex: none;
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .part {
    margin-top: var(--s4);
    padding-top: var(--s4);
    border-top: 1px solid var(--hairline);
  }
</style>
