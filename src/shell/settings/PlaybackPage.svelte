<script lang="ts">
  import DevicePicker from "../../lib/components/DevicePicker.svelte";
  import EqEditor from "../../lib/components/EqEditor.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import SpeedPitch from "../../lib/components/SpeedPitch.svelte";
  import Toggle from "../../lib/components/Toggle.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { settings } from "../../lib/stores/settings.svelte";
  import HiddenList from "./HiddenList.svelte";
  import SettingRow from "./SettingRow.svelte";

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();
  const v = $derived(settings.values);
  const crossfadeOptions = $derived([0, 3, 6, 9, 12].map((n) => ({ value: n, label: n === 0 ? t("settings.off") : t("settings.seconds", { n }) })));
</script>

{#if visible("settings.autoplay", "settings.autoplayDesc") || visible("settings.doubleClick", "settings.doubleClickDesc") || visible("settings.startup", "settings.startupDesc") || visible("settings.pauseOnDevice", "settings.pauseOnDeviceDesc")}
  <h3>{t("settings.section.queue")}</h3>
  <div class="card">
    {#if visible("settings.autoplay", "settings.autoplayDesc")}
      <SettingRow label={t("settings.autoplay")} desc={t("settings.autoplayDesc")}>
        <Toggle label={t("settings.autoplay")} checked={v.autoplay} onchange={(x) => settings.set("autoplay", x)} />
      </SettingRow>
    {/if}
    {#if visible("settings.doubleClick", "settings.doubleClickDesc")}
      <SettingRow label={t("settings.doubleClick")} desc={t("settings.doubleClickDesc")}>
        <Segmented
          label={t("settings.doubleClick")}
          value={v.doubleClick}
          options={[
            { value: "play", label: t("settings.playInstantly") },
            { value: "queue", label: t("settings.addToQueue") },
          ]}
          onchange={(x) => settings.set("doubleClick", x)} />
      </SettingRow>
    {/if}
    {#if visible("settings.startup", "settings.startupDesc")}
      <SettingRow label={t("settings.startup")} desc={t("settings.startupDesc")}>
        <Segmented
          label={t("settings.startup")}
          value={v.startup}
          options={[
            { value: "resume", label: t("settings.resumeQueue") },
            { value: "empty", label: t("settings.startEmpty") },
          ]}
          onchange={(x) => settings.set("startup", x)} />
      </SettingRow>
    {/if}
    {#if visible("settings.pauseOnDevice", "settings.pauseOnDeviceDesc")}
      <SettingRow label={t("settings.pauseOnDevice")} desc={t("settings.pauseOnDeviceDesc")}>
        <Toggle label={t("settings.pauseOnDevice")} checked={v.pauseOnDeviceChange} onchange={(x) => settings.set("pauseOnDeviceChange", x)} />
      </SettingRow>
    {/if}
  </div>
{/if}

{#if visible("settings.crossfade", "settings.crossfadeDesc") || visible("settings.normalize", "settings.normalizeDesc") || visible("settings.quality", "settings.qualityDesc")}
  <h3>{t("settings.section.sound")}</h3>
  <div class="card">
    {#if visible("settings.crossfade", "settings.crossfadeDesc")}
      <SettingRow label={t("settings.crossfade")} desc={t("settings.crossfadeDesc")}>
        <Segmented
          label={t("settings.crossfade")}
          value={v.crossfade}
          options={crossfadeOptions}
          onchange={(x) => {
            settings.set("crossfade", x);
            player.applySettings();
          }} />
      </SettingRow>
    {/if}
    {#if visible("settings.normalize", "settings.normalizeDesc")}
      <SettingRow label={t("settings.normalize")} desc={t("settings.normalizeDesc")}>
        <Toggle
          label={t("settings.normalize")}
          checked={v.normalize}
          onchange={(x) => {
            settings.set("normalize", x);
            player.applySettings();
          }} />
      </SettingRow>
    {/if}
    {#if visible("settings.quality", "settings.qualityDesc")}
      <SettingRow label={t("settings.quality")} desc={t("settings.qualityDesc")}>
        <Segmented
          label={t("settings.quality")}
          value={v.quality}
          options={[
            { value: "best", label: t("settings.best") },
            { value: "dataSaver", label: t("settings.dataSaver") },
          ]}
          onchange={(x) => settings.set("quality", x)} />
      </SettingRow>
    {/if}
  </div>
{/if}

{#if visible("sound.output", "sound.outputDesc") || visible("eq.title", "eq.desc") || visible("sound.speedPitch", "sound.speedPitchDesc") || visible("settings.sponsorBlock", "settings.sponsorBlockDesc")}
  <h3>{t("settings.section.tools")}</h3>
  <div class="card">
    {#if visible("sound.output", "sound.outputDesc")}
      <SettingRow label={t("sound.output")} desc={t("sound.outputDesc")}>
        <div class="device"><DevicePicker /></div>
      </SettingRow>
    {/if}
    {#if visible("eq.title", "eq.desc")}
      <div class="block">
        <EqEditor />
        <p class="note">{t("eq.desc")}</p>
      </div>
    {/if}
    {#if visible("sound.speedPitch", "sound.speedPitchDesc")}
      <div class="block">
        <SpeedPitch />
        <p class="note">{t("sound.speedPitchDesc")}</p>
      </div>
    {/if}
    {#if visible("settings.sponsorBlock", "settings.sponsorBlockDesc")}
      <SettingRow label={t("settings.sponsorBlock")} desc={t("settings.sponsorBlockDesc")}>
        <Toggle
          label={t("settings.sponsorBlock")}
          checked={v.sponsorBlock}
          onchange={(x) => {
            settings.set("sponsorBlock", x);
            player.applySkipSetting();
          }} />
      </SettingRow>
    {/if}
  </div>
{/if}

{#if visible("settings.hidden", "settings.hiddenDesc")}
  <h3>{t("settings.hidden")}</h3>
  <p class="desc">{t("settings.hiddenDesc")}</p>
  <div class="card"><HiddenList /></div>
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
  .desc {
    margin: -4px 2px 10px;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  .card {
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.05);
  }
  .block {
    padding: 14px 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.04);
  }
  .block + :global(.row) {
    border-top: 1px solid rgba(255, 255, 255, 0.04);
  }
  .note {
    margin: 10px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .device {
    max-width: 300px;
    display: flex;
  }
</style>
