<!-- One lyrics line in the chosen animation style. Only the line being sung animates its words;
     search hits are marked instead. -->
<script lang="ts">
  import type { LyricsStyle } from "../stores/settings.svelte";
  import { splitMatches, wordProgress, type ShownLine } from "./lines";

  // `still`: the music is paused, so the gap note rests instead of breathing (an endless
  // animation would repaint the window, and the always-on-top desktop lyrics, every frame).
  let {
    line,
    pos,
    active,
    style,
    query = "",
    still = false,
  }: { line: ShownLine; pos: number; active: boolean; style: LyricsStyle; query?: string; still?: boolean } = $props();
</script>

{#if line.gap}
  <span class="note" class:on={active && !still}>♪</span>
{:else if query.trim()}
  {#each splitMatches(line.text, query) as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
{:else if active && style !== "fade"}
  {#each line.words as w, i (i)}<span
      class="w {style}"
      class:done={pos >= w.endMs}
      class:now={pos >= w.startMs && pos < w.endMs}
      style:--p={style === "karaoke" ? `${wordProgress(w, pos) * 100}%` : undefined}>{w.text}</span
    >{" "}{/each}
{:else}
  {line.text}
{/if}

<style>
  .w {
    --off: rgba(255, 255, 255, 0.42);
    color: var(--off);
  }
  /* Each word fills with white from left to right as it's sung. */
  .karaoke {
    background: linear-gradient(90deg, #fff var(--p), var(--off) var(--p));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .glow,
  .bounce {
    transition:
      color 200ms var(--ease),
      text-shadow 300ms var(--ease),
      transform 260ms cubic-bezier(0.3, 1.5, 0.5, 1);
  }
  .glow.done,
  .glow.now,
  .bounce.done,
  .bounce.now {
    color: #fff;
  }
  .glow.now {
    text-shadow: 0 0 0.5em rgba(255, 255, 255, 0.5);
  }
  .bounce {
    display: inline-block;
  }
  .bounce.now {
    transform: translateY(-0.12em);
  }
  mark {
    padding: 0 2px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.85);
    color: var(--on-primary);
  }
  .note {
    display: inline-block;
  }
  .note.on {
    animation: breathe 2.4s ease-in-out infinite;
  }
  @keyframes breathe {
    50% {
      opacity: 0.45;
      transform: scale(0.92);
    }
  }
</style>
