<!-- Welcome setup, shown once right after an account is created: import music, pick the
     background, choose visual quality. Every choice saves at once and stays changeable in
     Settings, so Skip and closing the app lose nothing. -->
<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import BackgroundImage from "../../lib/backdrop/BackgroundImage.svelte";
  import logo from "../../lib/assets/logo.svg";
  import Button from "../../lib/components/Button.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import type { VisualQuality } from "../../lib/api";
  import { t } from "../../lib/i18n/index.svelte";
  import { pickImage } from "../../lib/profileActions";
  import { backgrounds } from "../../lib/stores/backgrounds.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { library } from "../../lib/stores/library.svelte";
  import { settings, type BackgroundMode } from "../../lib/stores/settings.svelte";
  import { visual } from "../../lib/stores/visual.svelte";

  let { onDone }: { onDone: () => void } = $props();

  const STEPS = 3;
  const QUALITIES: VisualQuality[] = ["high", "medium", "low"];
  const MODES: { value: BackgroundMode; label: "bg.cover" | "bg.image" | "bg.off"; desc: "welcome.bg.coverDesc" | "welcome.bg.imageDesc" | "welcome.bg.offDesc" }[] = [
    { value: "cover", label: "bg.cover", desc: "welcome.bg.coverDesc" },
    { value: "image", label: "bg.image", desc: "welcome.bg.imageDesc" },
    { value: "off", label: "bg.off", desc: "welcome.bg.offDesc" },
  ];

  let step = $state(0);
  let direction = $state(1);
  let link = $state("");

  const imported = $derived(library.overview.playlists.length);
  const mode = $derived(settings.values.background);
  const image = $derived(backgrounds.active);

  function go(next: number) {
    direction = next > step ? 1 : -1;
    step = next;
  }

  function importLink(e: SubmitEvent) {
    e.preventDefault();
    dialogs.open({ kind: "import", url: link.trim() || undefined });
    link = "";
  }

  function chooseMode(value: BackgroundMode) {
    // "Custom image" needs an image first; without one the picker opens and the mode
    // switches once a file is added.
    if (value === "image" && !backgrounds.list.length) return pickBackground();
    if (value === "image" && !image) return backgrounds.use(backgrounds.list[0].id);
    settings.set("background", value);
  }

  function pickBackground() {
    pickImage("image/jpeg,image/png,image/webp,image/gif", (file) => backgrounds.add(file));
  }

  function onKey(e: KeyboardEvent) {
    // Dialogs opened from here (import) handle their own keys.
    if (e.key === "Escape" && !dialogs.current) onDone();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="welcome" role="dialog" aria-modal="true" aria-label={t("welcome.title")} transition:fade={{ duration: 220 }}>
  <div class="panel glass" in:fly={{ y: 14, duration: 260 }}>
    <header>
      <div class="progress" role="progressbar" aria-valuemin={1} aria-valuemax={STEPS} aria-valuenow={step + 1} aria-label={t("welcome.step", { n: step + 1, total: STEPS })}>
        {#each { length: STEPS } as _, i (i)}
          <span class:on={i <= step}></span>
        {/each}
      </div>
      <span class="count">{t("welcome.step", { n: step + 1, total: STEPS })}</span>
      <Button variant="ghost" small onclick={onDone}>{t("welcome.skip")}</Button>
    </header>

    <div class="steps">
      {#key step}
        <section in:fly={{ x: 18 * direction, duration: 200, delay: 60 }} out:fade={{ duration: 80 }}>
          {#if step === 0}
            <img class="logo logo-mark" src={logo} alt="" width="40" height="40" />
            <h1>{t("welcome.import.title")}</h1>
            <p class="sub">{t("welcome.import.sub")}</p>
            <form class="paste" onsubmit={importLink}>
              <Icon name="link" size={16} />
              <input bind:value={link} placeholder={t("welcome.import.placeholder")} spellcheck="false" aria-label={t("welcome.import.placeholder")} />
              <Button type="submit" variant="primary" small disabled={!link.trim()}>{t("welcome.import.go")}</Button>
            </form>
            <div class="services">Spotify · Apple Music · Deezer · YouTube · SoundCloud</div>
            <button class="liked" onclick={() => dialogs.open({ kind: "importLiked" })}>
              <Icon name="heart" size={16} />
              <span class="text">
                <b>{t("liked.import")}</b>
                <small>{t("welcome.import.liked")}</small>
              </span>
              <Icon name="chevronDown" size={16} />
            </button>
            {#if imported}<p class="done"><Icon name="check" size={14} />{t("welcome.import.count", { n: imported })}</p>{/if}
          {:else if step === 1}
            <h1>{t("welcome.bg.title")}</h1>
            <p class="sub">{t("welcome.bg.sub")}</p>
            <div class="preview" aria-hidden="true">
              {#if mode === "image" && image}
                <BackgroundImage url={image.url} look={backgrounds.look(image)} />
              {:else if mode === "cover"}
                <div class="sample"><i></i><i></i><i></i></div>
              {/if}
              <div class="mock">
                <span class="cover"></span>
                <span class="lines"><i></i><i></i></span>
              </div>
            </div>
            <div class="choices three" role="radiogroup" aria-label={t("bg.mode")}>
              {#each MODES as m (m.value)}
                <button role="radio" aria-checked={mode === m.value} class:on={mode === m.value} onclick={() => chooseMode(m.value)}>
                  <b>{t(m.label)}</b>
                  <small>{t(m.desc)}</small>
                </button>
              {/each}
            </div>
            {#if mode === "image"}
              <Button variant="ghost" small icon="image" onclick={pickBackground}>{t("welcome.bg.another")}</Button>
            {/if}
          {:else}
            <h1>{t("welcome.quality.title")}</h1>
            <p class="sub">{t("quality.desc")}</p>
            <div class="choices" role="radiogroup" aria-label={t("quality.title")}>
              {#each QUALITIES as q (q)}
                <button class="row" role="radio" aria-checked={visual.chosen === q} class:on={visual.chosen === q} onclick={() => settings.set("visualQuality", q)}>
                  <span class="radio"></span>
                  <span class="text">
                    <b>{t(`quality.${q}`)}{#if visual.recommended === q}<em>{t("welcome.recommended")}</em>{/if}</b>
                    <small>{t(`quality.${q}Desc`)}</small>
                  </span>
                </button>
              {/each}
            </div>
            {#if visual.hardware}
              {@const hw = visual.hardware}
              <p class="reason">{t(`quality.reason.${hw.recommended.reason}`, { ram: hw.hardware.ramGb, cores: hw.hardware.cores, w: hw.hardware.width, h: hw.hardware.height })}</p>
            {/if}
          {/if}
        </section>
      {/key}
    </div>

    <footer>
      {#if step > 0}
        <Button variant="ghost" onclick={() => go(step - 1)}>{t("welcome.back")}</Button>
      {/if}
      <span class="hint">{t("welcome.later")}</span>
      {#if step < STEPS - 1}
        <Button variant="primary" onclick={() => go(step + 1)}>{t("welcome.next")}</Button>
      {:else}
        <Button variant="primary" onclick={onDone}>{t("welcome.finish")}</Button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .welcome {
    position: fixed;
    inset: 0;
    z-index: 65;
    display: grid;
    place-items: center;
    background: rgba(var(--scrim), 0.55);
    backdrop-filter: blur(18px);
  }
  .panel {
    width: min(560px, calc(100vw - 48px));
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    border-radius: 16px;
    background: rgba(var(--panel), 0.82);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.4);
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px 14px 0 24px;
  }
  .progress {
    display: flex;
    gap: 6px;
  }
  .progress span {
    width: 28px;
    height: 4px;
    border-radius: 2px;
    background: rgba(var(--ink), 0.14);
    transition: background var(--t) var(--ease);
  }
  .progress span.on {
    background: var(--solid);
  }
  .count {
    flex: 1;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  .steps {
    display: grid;
    min-height: 0;
    overflow-y: auto;
  }
  section {
    grid-area: 1 / 1;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    padding: 22px 24px 8px;
  }
  .logo {
    margin-bottom: -2px;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-2xl);
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .sub {
    margin: -6px 0 4px;
    font-size: var(--fs-md);
    line-height: 1.5;
    color: var(--text-2);
  }
  .paste {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 44px;
    padding: 0 6px 0 14px;
    border-radius: var(--r-md);
    background: var(--field);
    border: 1px solid var(--hairline);
    color: var(--text-3);
  }
  .paste:focus-within {
    border-color: rgba(var(--ink), 0.3);
  }
  .paste input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    font-size: var(--fs-md);
    color: var(--text);
  }
  .paste input::placeholder {
    color: var(--text-3);
  }
  .services {
    margin-top: -6px;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  .liked {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 12px 14px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--glass);
    text-align: left;
    color: var(--text-2);
  }
  .liked:hover {
    background: var(--glass-strong);
  }
  .liked > :global(svg:last-child) {
    rotate: -90deg;
    color: var(--text-3);
  }
  .text {
    flex: 1;
    display: grid;
    gap: 2px;
  }
  b {
    font-size: var(--fs-md);
    font-weight: 600;
    color: var(--text);
  }
  small {
    font-size: var(--fs-sm);
    line-height: 1.4;
    color: var(--text-2);
  }
  .done {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  /* A small window showing the chosen background, with a fake song row on top. */
  .preview {
    position: relative;
    width: 100%;
    height: 132px;
    overflow: hidden;
    border-radius: var(--r-lg);
    background: #0a0a0b;
  }
  .sample {
    position: absolute;
    inset: 0;
    filter: blur(18px);
  }
  .sample i {
    position: absolute;
    width: 70%;
    height: 200%;
    top: -120%;
    border-radius: 50%;
    animation: sway 14s ease-in-out infinite alternate;
  }
  /* Colors of an imaginary sunset cover, the kind of tint a real song brings. */
  .sample i:nth-child(1) {
    left: -14%;
    background: radial-gradient(closest-side, rgba(214, 112, 64, 0.85), transparent);
  }
  .sample i:nth-child(2) {
    left: 24%;
    background: radial-gradient(closest-side, rgba(196, 72, 88, 0.7), transparent);
    animation-duration: 18s;
  }
  .sample i:nth-child(3) {
    left: 58%;
    background: radial-gradient(closest-side, rgba(226, 170, 84, 0.6), transparent);
    animation-duration: 16s;
  }
  @keyframes sway {
    to {
      transform: translate(8%, 6%) scale(1.1);
    }
  }
  :global(html[data-quality="low"]) .sample i {
    animation: none;
  }
  .mock {
    position: absolute;
    left: 16px;
    bottom: 16px;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .mock .cover {
    width: 40px;
    height: 40px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.18);
  }
  .mock .lines {
    display: grid;
    gap: 6px;
  }
  .mock i {
    display: block;
    width: 110px;
    height: 7px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.75);
  }
  .mock i + i {
    width: 70px;
    background: rgba(255, 255, 255, 0.35);
  }
  .choices {
    display: grid;
    gap: 8px;
    width: 100%;
  }
  .choices.three {
    grid-template-columns: repeat(3, 1fr);
  }
  .choices button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 12px 14px;
    border: 1px solid transparent;
    border-radius: var(--r-md);
    background: var(--glass);
    text-align: left;
    transition: border-color var(--t-fast) var(--ease), background var(--t-fast) var(--ease);
  }
  .choices button:hover {
    background: var(--glass-strong);
  }
  .choices button.on {
    border-color: rgba(var(--ink), 0.5);
    background: var(--glass-strong);
  }
  .choices .row {
    flex-direction: row;
    align-items: center;
    gap: 12px;
  }
  .radio {
    flex: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1.5px solid rgba(var(--ink), 0.4);
    transition: border var(--t-fast) var(--ease);
  }
  .on .radio {
    border: 5px solid var(--solid);
  }
  em {
    margin-left: 8px;
    padding: 1px 7px;
    border-radius: var(--r-full);
    background: var(--glass-raised);
    font-size: var(--fs-xs);
    font-style: normal;
    font-weight: 600;
    vertical-align: 1px;
  }
  .reason {
    margin: 0;
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--text-3);
  }
  footer {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 18px 24px 20px;
  }
  .hint {
    flex: 1;
    font-size: var(--fs-xs);
    color: var(--text-3);
    text-align: right;
  }
</style>
