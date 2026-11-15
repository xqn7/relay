<!-- The idle screen: clock and date, the current lyric line filling in word by word with the
     next one faint below, and the song in the corner, all on the moving cover colors. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { fade, scale } from "svelte/transition";
  import { visual } from "../../lib/stores/visual.svelte";
  import { artistLine, coverAt, proxiedCover } from "../../lib/api";
  import Backdrop from "../../lib/backdrop/Backdrop.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import { i18n, t } from "../../lib/i18n/index.svelte";
  import { activeIndex, buildLines } from "../../lib/lyrics/lines";
  import LyricText from "../../lib/lyrics/LyricText.svelte";
  import { livePosition } from "../../lib/lyrics/position.svelte";
  import { lyrics } from "../../lib/stores/lyrics.svelte";
  import { player } from "../../lib/stores/player.svelte";

  const pos = livePosition();
  const track = $derived(player.current?.track ?? null);
  const cover = $derived(track?.cover ?? player.via?.cover ?? null);
  const lines = $derived(lyrics.status === "ready" && lyrics.data?.synced ? buildLines(lyrics.data.synced, player.durationMs) : []);
  const active = $derived(activeIndex(lines, pos.ms));
  const line = $derived(lines[active] ?? null);
  const nextLine = $derived(lines[active + 1] ?? null);
  const progress = $derived(player.durationMs > 0 ? Math.min(1, player.positionMs / player.durationMs) : 0);

  let now = $state(new Date());
  const time = $derived(new Intl.DateTimeFormat(i18n.lang, { hour: "numeric", minute: "2-digit" }).format(now));
  const date = $derived(new Intl.DateTimeFormat(i18n.lang, { weekday: "long", day: "numeric", month: "long" }).format(now));

  // The clock wakes once a minute, right when the minute changes.
  onMount(() => {
    let timer = 0;
    const schedule = () => {
      now = new Date();
      timer = window.setTimeout(schedule, 60_000 - (Date.now() % 60_000) + 50);
    };
    schedule();
    return () => clearTimeout(timer);
  });
</script>

<div class="idle theme-dark" in:fade={{ duration: 700 }} out:fade={{ duration: 260 }}>
  <Backdrop {cover} place="idle" />
  <div class="clock">
    <div class="time">{time}</div>
    <div class="date">{date}</div>
  </div>

  <div class="lyric">
    {#if line}
      {#key active}
        <div class="lines" in:fade={{ duration: 360 }}>
          <p class="current"><LyricText {line} pos={pos.ms} active style={visual.lyricsStyle} /></p>
          {#if nextLine}<p class="next">{nextLine.gap ? "♪" : nextLine.text}</p>{/if}
        </div>
      {/key}
    {/if}
  </div>

  {#if track}
    <div class="song">
      <div class="art">
        {#each [track] as tr (tr.id)}
          {@const art = tr.cover ?? player.via?.cover ?? null}
          <div class="art-cell" in:scale={visual.quality === "low" ? { start: 1, duration: 140, opacity: 0 } : { start: 0.94, duration: 520, opacity: 0 }} out:fade={{ duration: visual.quality === "low" ? 100 : 360 }}>
            {#if art}<img src={proxiedCover(coverAt(art, 400) ?? art)} alt="" />{:else}<span class="art-empty"><Icon name="music" size={36} /></span>{/if}
          </div>
        {/each}
      </div>
      <div class="meta">
        <div class="title truncate">{track.title}</div>
        <div class="artist truncate">{artistLine(track)}</div>
        <div class="bar"><span style:width="{progress * 100}%"></span></div>
      </div>
    </div>
  {/if}

  <p class="hint">{t("idle.hint")}</p>
</div>

<style>
  .idle {
    position: fixed;
    inset: 0;
    z-index: 85;
    overflow: hidden;
    background: var(--bg);
    cursor: none;
  }
  .clock {
    position: absolute;
    top: 7vh;
    left: 6vw;
  }
  .time {
    font-size: clamp(64px, 9vw, 128px);
    font-weight: 700;
    line-height: 1;
    letter-spacing: -0.03em;
    font-variant-numeric: tabular-nums;
  }
  .date {
    margin-top: 10px;
    font-size: clamp(16px, 1.6vw, 22px);
    font-weight: 500;
    color: var(--text-2);
  }
  .lyric {
    position: absolute;
    left: 6vw;
    right: 40vw;
    top: 44vh;
    display: grid;
  }
  .lines {
    grid-area: 1 / 1;
  }
  .current {
    margin: 0;
    font-size: clamp(30px, 3.6vw, 52px);
    font-weight: 700;
    line-height: 1.2;
    letter-spacing: -0.015em;
  }
  .next {
    margin: 14px 0 0;
    font-size: clamp(20px, 2.2vw, 30px);
    font-weight: 600;
    color: rgba(var(--ink), 0.32);
  }
  .song {
    position: absolute;
    right: 5vw;
    bottom: 9vh;
    display: flex;
    align-items: flex-end;
    gap: 18px;
    width: min(440px, 38vw);
  }
  .art {
    display: grid;
    flex: none;
    width: 112px;
    height: 112px;
  }
  .art-cell {
    grid-area: 1 / 1;
    overflow: hidden;
    border-radius: 10px;
    background: rgba(var(--ink), 0.06);
    box-shadow: 0 18px 44px rgba(0, 0, 0, 0.4);
  }
  .art-empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--text-3);
  }
  .art-cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .meta {
    flex: 1;
    min-width: 0;
    padding-bottom: 4px;
  }
  .title {
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .artist {
    margin-top: 2px;
    font-size: var(--fs-md);
    color: var(--text-2);
  }
  .bar {
    height: 3px;
    margin-top: 14px;
    border-radius: 2px;
    background: rgba(var(--ink), 0.16);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: rgba(var(--ink), 0.85);
    transition: width 300ms linear;
  }
  .hint {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 3.5vh;
    margin: 0;
    text-align: center;
    font-size: var(--fs-xs);
    color: rgba(var(--ink), 0.28);
  }
</style>
