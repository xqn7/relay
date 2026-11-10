<!-- Playback speed (pitch kept) and pitch (speed kept), each with a reset. -->
<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import { PITCH_RANGE, settings, SPEED_MAX, SPEED_MIN } from "../stores/settings.svelte";
  import { sound } from "../stores/sound.svelte";
  import IconButton from "./IconButton.svelte";
  import Slider from "./Slider.svelte";

  const speed = $derived(settings.values.speed);
  const pitch = $derived(settings.values.pitch);
  const speedFrac = $derived((speed - SPEED_MIN) / (SPEED_MAX - SPEED_MIN));
  const pitchFrac = $derived((pitch + PITCH_RANGE) / (2 * PITCH_RANGE));
  const toSpeed = (f: number) => SPEED_MIN + f * (SPEED_MAX - SPEED_MIN);
  const toPitch = (f: number) => f * 2 * PITCH_RANGE - PITCH_RANGE;
  const speedText = $derived(`${Number(speed.toFixed(2))}×`);
  const pitchText = $derived(pitch === 0 ? "0" : `${pitch > 0 ? "+" : "−"}${Math.abs(pitch)}`);
</script>

<div class="rows">
  <div class="row">
    <span class="label">{t("sound.speed")}</span>
    <div class="slider"><Slider label={t("sound.speed")} value={speedFrac} valueText={speedText} oninput={(f) => sound.setSpeed(toSpeed(f))} onchange={(f) => sound.setSpeed(toSpeed(f))} /></div>
    <span class="value">{speedText}</span>
    <span class="reset" class:hidden={speed === 1}><IconButton icon="undo" size={14} label={t("sound.reset")} onclick={() => sound.setSpeed(1)} /></span>
  </div>
  <div class="row">
    <span class="label">{t("sound.pitch")}</span>
    <div class="slider"><Slider label={t("sound.pitch")} value={pitchFrac} valueText={t("sound.semitones", { n: pitchText })} oninput={(f) => sound.setPitch(toPitch(f))} onchange={(f) => sound.setPitch(toPitch(f))} /></div>
    <span class="value">{t("sound.semitones", { n: pitchText })}</span>
    <span class="reset" class:hidden={pitch === 0}><IconButton icon="undo" size={14} label={t("sound.reset")} onclick={() => sound.setPitch(0)} /></span>
  </div>
</div>

<style>
  .rows {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
  }
  .row {
    display: grid;
    grid-template-columns: 64px 1fr 56px 28px;
    align-items: center;
    gap: var(--s3);
  }
  .label {
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .value {
    font-size: var(--fs-sm);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .hidden {
    visibility: hidden;
  }
</style>
