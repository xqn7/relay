<!-- Settings → Discord: Rich Presence on/off, what it shows, and a live preview of the status
     built by the same code that sends it to Discord. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api, formatTime, type DesktopInfo, type DiscordActivity } from "../../lib/api";
  import Segmented from "../../lib/components/Segmented.svelte";
  import Toggle from "../../lib/components/Toggle.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { settings, type SettingKey } from "../../lib/stores/settings.svelte";
  import SettingRow from "./SettingRow.svelte";

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();
  const v = $derived(settings.values);

  let info = $state<DesktopInfo | null>(null);
  let preview = $state<DiscordActivity | null>(null);
  const unset = $derived(info?.discord === "unset");

  onMount(() => {
    api.desktopInfo().then((i) => (info = i)).catch(() => {});
  });

  // The preview follows the song and every option; settings save before it's asked again.
  $effect(() => {
    void [player.current, player.playing, v.discordFormat, v.discordTimeline, v.discordArtwork, v.discordArtistBadge, v.discordShowPaused];
    void [v.discordButtonYoutube, v.discordButtonSoundcloud, v.discordButtonSpotify, v.discord];
    const timer = setTimeout(() => api.discordPreview().then((p) => (preview = p)).catch(() => (preview = null)), 150);
    return () => clearTimeout(timer);
  });

  const status = $derived(
    !info ? "" : unset ? t("discord.unset") : !v.discord ? t("discord.statusOff") : info.discord === "connected" ? t("discord.connected") : t("discord.waiting"),
  );
  const length = $derived(preview?.timestamps ? preview.timestamps.end - preview.timestamps.start : 0);

  const toggles: [SettingKey, Key, Key][] = [
    ["discordTimeline", "discord.timeline", "discord.timelineDesc"],
    ["discordArtwork", "discord.artwork", "discord.artworkDesc"],
    ["discordArtistBadge", "discord.badge", "discord.badgeDesc"],
    ["discordShowPaused", "discord.paused", "discord.pausedDesc"],
  ];
  const buttons: [SettingKey, Key][] = [
    ["discordButtonYoutube", "discord.listenYoutube"],
    ["discordButtonSoundcloud", "discord.listenSoundcloud"],
    ["discordButtonSpotify", "discord.listenSpotify"],
  ];
</script>

<div class="preview card">
  <div class="kind">{t("discord.listeningTo")}</div>
  {#if preview}
    <div class="activity">
      {#if preview.assets?.large_image}
        <div class="art">
          <img src={preview.assets.large_image} alt="" />
          {#if preview.assets.small_image}<img class="badge" src={preview.assets.small_image} alt="" />{/if}
        </div>
      {/if}
      <div class="lines">
        <div class="details truncate">{preview.details}</div>
        <div class="state truncate">{preview.state}</div>
        {#if length > 0}
          <div class="timeline">
            <span>{formatTime(player.positionMs)}</span>
            <div class="track"><div class="fill" style:width="{Math.min(100, (player.positionMs / length) * 100)}%"></div></div>
            <span>{formatTime(length)}</span>
          </div>
        {/if}
      </div>
    </div>
    {#each preview.buttons ?? [] as b (b.label)}
      <div class="btn">{b.label}</div>
    {/each}
  {:else}
    <p class="empty">{player.current ? t("discord.previewHidden") : t("discord.previewNothing")}</p>
  {/if}
  <p class="status">{status}</p>
</div>

{#if visible("discord.enable", "discord.enableDesc") || visible("discord.format", "discord.formatDesc")}
  <h3>{t("discord.richPresence")}</h3>
  <div class="card">
    <SettingRow label={t("discord.enable")} desc={t("discord.enableDesc")}>
      <Toggle label={t("discord.enable")} checked={v.discord} onchange={(x) => settings.set("discord", x)} />
    </SettingRow>
    <SettingRow label={t("discord.format")} desc={t("discord.formatDesc")}>
      <Segmented
        label={t("discord.format")}
        value={v.discordFormat}
        options={[
          { value: "artist", label: t("discord.format.artist") },
          { value: "title", label: t("discord.format.title") },
          { value: "both", label: t("discord.format.both") },
        ]}
        onchange={(x) => settings.set("discordFormat", x)} />
    </SettingRow>
    {#each toggles as [key, label, desc] (key)}
      <SettingRow label={t(label)} desc={t(desc)}>
        <Toggle label={t(label)} checked={v[key] as boolean} onchange={(x) => settings.set(key, x)} />
      </SettingRow>
    {/each}
  </div>
{/if}

{#if visible("discord.buttons", "discord.buttonsDesc")}
  <h3>{t("discord.buttons")}</h3>
  <div class="card">
    <p class="note top">{t("discord.buttonsDesc")}</p>
    {#each buttons as [key, label] (key)}
      <SettingRow label={t(label)}>
        <Toggle label={t(label)} checked={v[key] as boolean} onchange={(x) => settings.set(key, x)} />
      </SettingRow>
    {/each}
  </div>
{/if}

<style>
  .preview {
    display: grid;
    gap: 10px;
    margin-bottom: 22px;
    padding: 14px 16px;
  }
  .kind {
    font-size: var(--fs-xs);
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--text-2);
  }
  .activity {
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .art {
    position: relative;
    flex: none;
    width: 72px;
    height: 72px;
  }
  .art img {
    width: 100%;
    height: 100%;
    border-radius: var(--r-md);
    object-fit: cover;
  }
  .art .badge {
    position: absolute;
    right: -4px;
    bottom: -4px;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2px solid #18181b;
  }
  .lines {
    display: grid;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .details {
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .state {
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
  .timeline {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .track {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.12);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--text);
  }
  .btn {
    padding: 7px;
    border-radius: var(--r-sm);
    background: rgba(255, 255, 255, 0.08);
    text-align: center;
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .empty,
  .status {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  .top {
    padding-top: 14px;
  }
</style>
