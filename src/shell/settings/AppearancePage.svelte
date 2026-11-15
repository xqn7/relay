<!-- Settings → Appearance: the background, how lyrics look and translate, the up-next card and
     the idle screen. Language, theme and visual quality join this page in later phases. -->
<script lang="ts">
  import { api, isAppError } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Field from "../../lib/components/Field.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import Toggle from "../../lib/components/Toggle.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { lyrics } from "../../lib/stores/lyrics.svelte";
  import { IDLE_MINUTES, LYRICS_STYLES, settings } from "../../lib/stores/settings.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { desktop } from "../../lib/stores/desktop.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { visual } from "../../lib/stores/visual.svelte";
  import SettingRow from "./SettingRow.svelte";

  const QUALITIES = ["high", "medium", "low"] as const;

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();
  const v = $derived(settings.values);

  let keySaved = $state(false);
  let key = $state("");
  let keyError = $state("");
  let keyBusy = $state(false);

  $effect(() => {
    api.deeplKeySaved().then((saved) => (keySaved = saved)).catch(() => {});
  });

  async function saveKey(value: string | null) {
    keyBusy = true;
    keyError = "";
    try {
      await api.deeplKeySet(value);
      keySaved = value != null;
      key = "";
      ui.toast(value ? t("lyrics.deeplSaved") : t("lyrics.deeplRemoved"), "check");
    } catch (e) {
      const code = isAppError(e) ? e.code : "";
      keyError = code === "deepl_key_invalid" ? t("lyrics.deeplInvalid") : code === "offline" || code === "network" ? t("error.offline") : t("error.generic");
    } finally {
      keyBusy = false;
    }
  }

  function setTranslateTo(lang: "en" | "de") {
    settings.set("translateTo", lang);
    if (v.translateLyrics) lyrics.translate();
  }
</script>

{#if visible("quality.title", "quality.desc")}
  <h3>{t("quality.title")}</h3>
  <div class="card">
    <SettingRow label={t("quality.title")} desc={t(`quality.${visual.chosen}Desc`)}>
      <Segmented
        label={t("quality.title")}
        value={visual.chosen}
        options={QUALITIES.map((q) => ({ value: q, label: t(`quality.${q}`) }))}
        onchange={(x) => settings.set("visualQuality", x)} />
    </SettingRow>
    {#if visual.hardware}
      {@const hw = visual.hardware}
      <p class="note">
        <span class="badge">{t("quality.recommended", { level: t(`quality.${hw.recommended.quality}`) })}</span>
        {t(`quality.reason.${hw.recommended.reason}`, { ram: hw.hardware.ramGb, cores: hw.hardware.cores, w: hw.hardware.width, h: hw.hardware.height })}
        <button class="link" onclick={() => dialogs.open({ kind: "hardware", info: hw })}>{t("quality.details")}</button>
      </p>
    {/if}
    {#if visual.game}<p class="note">{t("quality.gameActive")}</p>{/if}
  </div>
{/if}

{#if visible("bg.mode", "bg.modeDesc") || visible("bg.mix", "bg.mixDesc")}
  <h3>{t("settings.section.background")}</h3>
  <div class="card">
    <SettingRow label={t("bg.mode")} desc={t("bg.modeDesc")}>
      <Segmented
        label={t("bg.mode")}
        value={v.background}
        options={[
          { value: "cover", label: t("bg.cover") },
          { value: "image", label: t("bg.image") },
          { value: "off", label: t("bg.off") },
        ]}
        onchange={(x) => settings.set("background", x)} />
    </SettingRow>
    {#if v.background === "image"}
      {#await import("./BackgroundEditor.svelte") then m}<m.default />{/await}
    {/if}
  </div>
{/if}

{#if visible("settings.lyricsStyle", "settings.lyricsStyleDesc") || visible("settings.translateTo", "settings.translateToDesc") || visible("settings.deepl", "settings.deeplDesc")}
  <h3>{t("settings.section.lyrics")}</h3>
  <div class="card">
    {#if visible("settings.lyricsStyle", "settings.lyricsStyleDesc")}
      <SettingRow label={t("settings.lyricsStyle")} desc={t("settings.lyricsStyleDesc")}>
        <Segmented
          label={t("settings.lyricsStyle")}
          value={v.lyricsStyle}
          options={LYRICS_STYLES.map((s) => ({ value: s, label: t(`lyrics.style.${s}`) }))}
          onchange={(x) => settings.set("lyricsStyle", x)} />
      </SettingRow>
    {/if}
    {#if visible("settings.translateTo", "settings.translateToDesc")}
      <SettingRow label={t("settings.translateTo")} desc={t("settings.translateToDesc")}>
        <Segmented
          label={t("settings.translateTo")}
          value={v.translateTo}
          options={[
            { value: "en", label: "English" },
            { value: "de", label: "Deutsch" },
          ]}
          onchange={setTranslateTo} />
      </SettingRow>
    {/if}
    {#if visible("settings.deepl", "settings.deeplDesc")}
      <SettingRow label={t("settings.deepl")} desc={keySaved ? t("settings.deeplSavedDesc") : t("settings.deeplDesc")}>
        {#if keySaved}<Button variant="secondary" busy={keyBusy} onclick={() => saveKey(null)}>{t("storage.remove")}</Button>{/if}
      </SettingRow>
      {#if !keySaved}
        <form
          class="key-row"
          onsubmit={(e) => {
            e.preventDefault();
            if (key.trim()) saveKey(key.trim());
          }}>
          <div class="key-field"><Field label={t("settings.deeplKey")} type="password" bind:value={key} placeholder="xxxxxxxx-xxxx-…:fx" error={keyError} /></div>
          <Button type="submit" variant="secondary" busy={keyBusy} disabled={!key.trim()}>{t("settings.save")}</Button>
        </form>
      {/if}
    {/if}
  </div>
{/if}

{#if visible("desktopLyrics.title", "desktopLyrics.desc") || visible("lockscreen.title", "lockscreen.desc")}
  <h3>{t("settings.section.desktop")}</h3>
  <div class="card">
    {#if visible("desktopLyrics.title", "desktopLyrics.desc")}
      <SettingRow label={t("desktopLyrics.title")} desc={t("desktopLyrics.desc")}>
        <Toggle label={t("desktopLyrics.title")} checked={v.desktopLyrics} onchange={(x) => settings.set("desktopLyrics", x)} />
      </SettingRow>
      {#if v.desktopLyrics}
        <SettingRow label={t("desktopLyrics.position")} desc={t("desktopLyrics.positionDesc")}>
          <Segmented
            label={t("desktopLyrics.position")}
            value={v.lyricsPosition}
            options={[
              { value: "bottom", label: t("desktopLyrics.bottom") },
              { value: "top", label: t("desktopLyrics.top") },
            ]}
            onchange={(x) => settings.set("lyricsPosition", x)} />
        </SettingRow>
        <SettingRow label={t("desktopLyrics.move")} desc={t("desktopLyrics.moveDesc")}>
          <Button variant="secondary" icon={desktop.lyricsUnlocked ? "lock" : "unlock"} onclick={() => desktop.setLyricsUnlocked(!desktop.lyricsUnlocked)}>
            {desktop.lyricsUnlocked ? t("desktopLyrics.lock") : t("desktopLyrics.unlock")}
          </Button>
        </SettingRow>
      {/if}
    {/if}
    {#if visible("lockscreen.title", "lockscreen.desc")}
      <SettingRow label={t("lockscreen.title")} desc={t("lockscreen.desc")}>
        <Toggle label={t("lockscreen.title")} checked={v.lockScreen} onchange={(x) => settings.set("lockScreen", x)} />
      </SettingRow>
    {/if}
  </div>
{/if}

{#if visible("settings.upNext", "settings.upNextDesc") || visible("settings.idle", "settings.idleDesc") || visible("settings.waveform", "settings.waveformDesc") || visible("settings.visualizer", "settings.visualizerDesc")}
  <h3>{t("settings.section.screen")}</h3>
  <div class="card">
    {#if visible("settings.waveform", "settings.waveformDesc")}
      <SettingRow label={t("settings.waveform")} desc={t("settings.waveformDesc")}>
        <Toggle label={t("settings.waveform")} checked={v.waveform} onchange={(x) => settings.set("waveform", x)} />
      </SettingRow>
    {/if}
    {#if visible("settings.visualizer", "settings.visualizerDesc")}
      <SettingRow label={t("settings.visualizer")} desc={t("settings.visualizerDesc")}>
        {#if v.visualizer}
          <Segmented
            label={t("settings.visualizerStyle")}
            value={v.visualizerStyle}
            options={[
              { value: "bars", label: t("viz.bars") },
              { value: "wave", label: t("viz.wave") },
            ]}
            onchange={(x) => settings.set("visualizerStyle", x)} />
        {/if}
        <Toggle label={t("settings.visualizer")} checked={v.visualizer} onchange={(x) => settings.set("visualizer", x)} />
      </SettingRow>
    {/if}
    {#if visible("settings.upNext", "settings.upNextDesc")}
      <SettingRow label={t("settings.upNext")} desc={t("settings.upNextDesc")}>
        <Toggle label={t("settings.upNext")} checked={v.upNext} onchange={(x) => settings.set("upNext", x)} />
      </SettingRow>
    {/if}
    {#if visible("settings.idle", "settings.idleDesc")}
      <SettingRow label={t("settings.idle")} desc={t("settings.idleDesc")}>
        <Segmented
          label={t("settings.idle")}
          value={v.idleMinutes}
          options={IDLE_MINUTES.map((n) => ({ value: n, label: t("settings.minutes", { n }) }))}
          onchange={(x) => settings.set("idleMinutes", x)} />
      </SettingRow>
    {/if}
  </div>
{/if}

<style>
  h3 {
    margin: 22px 2px 10px;
    font-size: var(--fs-md);
    font-weight: 700;
  }
  h3:first-child {
    margin-top: 0;
  }
  .card {
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.05);
  }
  .key-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 0 16px 14px;
  }
  .key-field {
    flex: 1;
  }
  .key-row :global(.btn) {
    margin-top: 22px;
  }
  .badge {
    display: inline-block;
    margin-right: 6px;
    padding: 1px 7px;
    border-radius: var(--r-full);
    background: rgba(255, 255, 255, 0.12);
    color: var(--text);
    font-weight: 600;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-2);
    font: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .link:hover {
    color: var(--text);
  }
</style>
