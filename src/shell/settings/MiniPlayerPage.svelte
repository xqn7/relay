<!-- Settings → Mini player: on/off, where it sits, its global shortcut, auto-show on minimize,
     always on top. -->
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import Toggle from "../../lib/components/Toggle.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { MINI_POSITIONS, settings, validShortcut } from "../../lib/stores/settings.svelte";
  import SettingRow from "./SettingRow.svelte";

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();
  const v = $derived(settings.values);

  let recording = $state(false);
  let shortcutOk = $state(true);
  let custom = $state(false);
  let invalid = $state(false);

  onMount(() => {
    api.desktopInfo().then((i) => {
      shortcutOk = i.shortcutOk;
      custom = i.miniCustom;
    });
    const offs = [listen<boolean>("shortcut-status", ({ payload }) => (shortcutOk = payload)), listen<string>("overlay-moved", ({ payload }) => payload === "mini" && (custom = true))];
    return () => offs.forEach((o) => o.then((f) => f()));
  });

  function setPosition(p: (typeof MINI_POSITIONS)[number]) {
    // Saved even when unchanged: picking a preset snaps a dragged player back to it.
    settings.set("miniPosition", p);
    custom = false;
  }

  /** Builds "Ctrl+Alt+K"-style text from a key press. */
  function record(e: KeyboardEvent) {
    e.preventDefault();
    if (e.key === "Escape") {
      recording = false;
      return;
    }
    const key = e.code.startsWith("Key") ? e.code.slice(3) : e.code.startsWith("Digit") ? e.code.slice(5) : /^F\d+$/.test(e.code) ? e.code : e.code === "Space" ? "Space" : null;
    if (!key) return;
    const parts = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Win", key].filter(Boolean) as string[];
    const text = parts.join("+");
    invalid = !validShortcut(text);
    if (invalid) return;
    recording = false;
    settings.set("miniShortcut", text);
  }
</script>

<svelte:window onkeydown={(e) => recording && record(e)} />

{#if visible("mini.enable", "mini.enableDesc") || visible("mini.position", "mini.positionDesc") || visible("mini.shortcut", "mini.shortcutDesc") || visible("mini.autoShow", "mini.autoShowDesc") || visible("mini.onTop", "mini.onTopDesc")}
  <h3>{t("settings.page.miniPlayer")}</h3>
  <div class="card">
    {#if visible("mini.enable", "mini.enableDesc")}
      <SettingRow label={t("mini.enable")} desc={t("mini.enableDesc")}>
        <Toggle label={t("mini.enable")} checked={v.miniPlayer} onchange={(x) => settings.set("miniPlayer", x)} />
      </SettingRow>
    {/if}
    {#if v.miniPlayer}
      {#if visible("mini.position", "mini.positionDesc")}
        <SettingRow label={t("mini.position")} desc={custom ? t("mini.positionCustom") : t("mini.positionDesc")}>
          <Segmented
            label={t("mini.position")}
            value={custom ? ("" as (typeof MINI_POSITIONS)[number]) : v.miniPosition}
            options={MINI_POSITIONS.map((p) => ({ value: p, label: t(`mini.pos.${p}`) }))}
            onchange={setPosition} />
        </SettingRow>
      {/if}
      {#if visible("mini.shortcut", "mini.shortcutDesc")}
        <SettingRow
          label={t("mini.shortcut")}
          desc={recording ? (invalid ? t("mini.shortcutInvalid") : t("mini.shortcutPress")) : shortcutOk ? t("mini.shortcutDesc") : t("mini.shortcutTaken")}>
          <Button variant="secondary" onclick={() => ((recording = !recording), (invalid = false))}>
            {recording ? t("mini.shortcutRecording") : v.miniShortcut}
          </Button>
        </SettingRow>
      {/if}
      {#if visible("mini.autoShow", "mini.autoShowDesc")}
        <SettingRow label={t("mini.autoShow")} desc={t("mini.autoShowDesc")}>
          <Toggle label={t("mini.autoShow")} checked={v.miniAutoShow} onchange={(x) => settings.set("miniAutoShow", x)} />
        </SettingRow>
      {/if}
      {#if visible("mini.onTop", "mini.onTopDesc")}
        <SettingRow label={t("mini.onTop")} desc={t("mini.onTopDesc")}>
          <Toggle label={t("mini.onTop")} checked={v.miniOnTop} onchange={(x) => settings.set("miniOnTop", x)} />
        </SettingRow>
      {/if}
      <SettingRow label={t("mini.try")} desc={t("mini.tryDesc", { keys: v.miniShortcut })}>
        <Button variant="secondary" onclick={() => api.miniToggle()}>{t("mini.toggle")}</Button>
      </SettingRow>
    {/if}
  </div>
{/if}
