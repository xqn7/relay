<!-- Wrapped: full-screen cards for a month or a year of listening. Arrows or the arrow keys
     move between cards; "Save image" writes the card as a PNG to share. -->
<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { api, type Stats } from "../../lib/api";
  import { neutralColors, type Rgb } from "../../lib/backdrop/colors";
  import { coverColors, rgba } from "../../lib/backdrop/extract";
  import Avatar from "../../lib/components/Avatar.svelte";
  import Button from "../../lib/components/Button.svelte";
  import Cover from "../../lib/components/Cover.svelte";
  import IconButton from "../../lib/components/IconButton.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { i18n, t } from "../../lib/i18n/index.svelte";
  import { session } from "../../lib/stores/session.svelte";
  import { ui, type WrappedPeriod } from "../../lib/stores/ui.svelte";
  import { buildCards, leadCover, type Card } from "../../lib/wrapped/cards";
  import { CARD_H, CARD_W, renderCard } from "../../lib/wrapped/render";

  let { period }: { period: WrappedPeriod } = $props();

  let stats = $state<Stats | null>(null);
  let failed = $state(false);
  let colors = $state.raw<Rgb[]>(neutralColors());
  let index = $state(0);
  let direction = $state(1);
  let saving = $state(false);
  let scale = $state(1);

  const tr = (key: string, vars?: Record<string, string | number>) => t(key as Key, vars);
  const cards = $derived<Card[]>(stats ? buildCards(stats, period, i18n.lang, tr) : []);
  const card = $derived(cards[index] ?? null);
  const empty = $derived(stats !== null && stats.plays === 0);
  const name = $derived(session.account?.displayName ?? "");
  const avatar = $derived(session.account?.avatar ?? null);

  $effect(() => {
    const p = period;
    stats = null;
    failed = false;
    index = 0;
    api.listeningStats(p.from, p.to).then(
      (s) => {
        stats = s;
        const lead = leadCover(s);
        if (lead) coverColors(lead).then((c) => (colors = c), () => {});
      },
      () => (failed = true),
    );
  });

  // The card is designed at 540 × 675 and scaled to fit the window.
  function fitScale() {
    scale = Math.min(1.2, (window.innerHeight - 170) / CARD_H, (window.innerWidth - 200) / CARD_W);
  }
  fitScale();

  function go(step: number) {
    const next = Math.min(cards.length - 1, Math.max(0, index + step));
    if (next === index) return;
    direction = step;
    index = next;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") ui.wrapped = null;
    else if (e.key === "ArrowRight") go(1);
    else if (e.key === "ArrowLeft") go(-1);
  }

  async function save() {
    if (!card) return;
    saving = true;
    try {
      const png = await renderCard(card, colors, { name, avatar });
      const saved = await api.imageSave(png, `Relay ${period.label} ${index + 1}`);
      if (saved) ui.toast(t("wrapped.saved"), "check");
    } catch (e) {
      console.error("saving the card failed", e);
      ui.toast(t("wrapped.saveFailed"), "alert");
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={onKey} onresize={fitScale} />

<div class="wrapped" role="dialog" aria-modal="true" aria-label={t("wrapped.title")} transition:fade={{ duration: 180 }}>
  <header>
    <span class="label">{t("wrapped.title")} · {period.label}</span>
    <IconButton icon="close" label={t("win.close")} onclick={() => (ui.wrapped = null)} />
  </header>

  {#if failed}
    <p class="state">{t("error.generic")}</p>
  {:else if !stats}
    <div class="state"><Spinner size={22} /></div>
  {:else if empty}
    <p class="state">{t("wrapped.empty")}</p>
  {:else if card}
    <div class="stage">
      <IconButton icon="back" label={t("wrapped.previous")} variant="solid" disabled={index === 0} onclick={() => go(-1)} />
      <div class="frame" style:width="{CARD_W * scale}px" style:height="{CARD_H * scale}px">
        {#key index}
          <div
            class="card"
            style:scale={scale}
            style:--bd-1={rgba(colors[0], 0.85)}
            style:--bd-2={rgba(colors[1] ?? colors[0], 0.85)}
            style:--bd-3={rgba(colors[2] ?? colors[0], 0.85)}
            in:fly={{ x: 24 * direction, duration: 200 }}
            out:fade={{ duration: 100 }}>
            <div class="glow"></div>
            <div class="kicker">{card.kicker}</div>
            {#if card.id === "intro"}
              <div class="intro">
                <Avatar src={avatar} {name} size={132} />
                <h2 class="intro-title">{card.title}</h2>
                <div class="who">{name}</div>
              </div>
            {:else if card.title}
              <h2>{card.title}</h2>
            {/if}
            {#if card.big}
              <div class="big">{card.big}</div>
              <div class="unit">{card.unit}</div>
            {/if}
            {#if card.rows}
              <ol>
                {#each card.rows as row, i (i)}
                  <li>
                    <Cover src={row.cover} size={60} radius={8} round={row.round} fallback={row.round ? "user" : "music"} />
                    <div class="row-text">
                      <div class="row-title truncate">{row.title}</div>
                      <div class="row-sub truncate">{row.sub}</div>
                    </div>
                    <span class="row-value">{row.value}</span>
                  </li>
                {/each}
              </ol>
            {/if}
            {#if card.note}<p class="note">{card.note}</p>{/if}
            <footer>
              <Avatar src={avatar} {name} size={28} />
              <span class="truncate">{name}</span>
              <b>Relay</b>
            </footer>
          </div>
        {/key}
      </div>
      <IconButton icon="forward" label={t("wrapped.next")} variant="solid" disabled={index === cards.length - 1} onclick={() => go(1)} />
    </div>
    <div class="bottom">
      <div class="dots" aria-hidden="true">
        {#each cards as c, i (c.id)}<span class:on={i === index}></span>{/each}
      </div>
      <Button variant="secondary" icon="download" busy={saving} onclick={save}>{t("wrapped.save")}</Button>
    </div>
  {/if}
</div>

<style>
  .wrapped {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    flex-direction: column;
    align-items: center;
    background: rgba(8, 8, 9, 0.86);
    backdrop-filter: blur(30px);
  }
  header {
    align-self: stretch;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 52px;
    padding: 0 12px 0 24px;
  }
  .label {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-2);
  }
  .state {
    margin: auto;
    color: var(--text-2);
  }
  .stage {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--s5);
  }
  .frame {
    position: relative;
    display: grid;
  }
  /* Laid out on the same 540 × 675 grid as the saved image. */
  .card {
    grid-area: 1 / 1;
    position: relative;
    width: 540px;
    height: 675px;
    overflow: hidden;
    padding: 56px 44px 44px;
    border-radius: 22px;
    background: var(--bg);
    transform-origin: top left;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.45);
  }
  .glow {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(340px 340px at 80px 40px, var(--bd-1), transparent),
      radial-gradient(320px 320px at 300px 0, var(--bd-2), transparent),
      radial-gradient(300px 300px at 500px 120px, var(--bd-3), transparent);
  }
  .glow::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, rgba(10, 10, 11, 0) 0%, rgba(10, 10, 11, 0.6) 45%, var(--bg) 80%);
  }
  .card > :not(.glow, footer) {
    position: relative;
  }
  .kicker {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-2);
  }
  h2 {
    margin: 16px 0 0;
    font-size: 34px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .intro {
    margin-top: 44px;
  }
  .intro-title {
    margin: 48px 0 0;
    font-size: 44px;
    line-height: 52px;
  }
  .who {
    margin-top: 12px;
    font-size: 20px;
    font-weight: 600;
    color: var(--text-2);
  }
  .big {
    margin-top: 150px;
    font-size: 84px;
    font-weight: 800;
    letter-spacing: -0.03em;
    line-height: 1;
  }
  .unit {
    margin-top: 12px;
    font-size: 22px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.8);
  }
  ol {
    display: grid;
    gap: 24px;
    margin: 26px 0 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .row-text {
    flex: 1;
    min-width: 0;
  }
  .row-title {
    font-size: 18px;
    font-weight: 600;
  }
  .row-sub,
  .row-value {
    margin-top: 2px;
    font-size: 15px;
    color: var(--text-2);
  }
  .note {
    margin: 30px 0 0;
    font-size: 18px;
    line-height: 26px;
    color: var(--text-2);
  }
  .card footer {
    position: absolute;
    left: 44px;
    right: 44px;
    bottom: 44px;
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 15px;
    font-weight: 600;
  }
  .card footer b {
    margin-left: auto;
    color: var(--text-2);
  }
  .bottom {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s3);
    padding: var(--s3) 0 var(--s5);
  }
  .dots {
    display: flex;
    gap: 6px;
  }
  .dots span {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.25);
    transition: background var(--t) var(--ease);
  }
  .dots span.on {
    background: #fff;
  }
</style>
