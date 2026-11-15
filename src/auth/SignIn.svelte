<script lang="ts">
  import { fade } from "svelte/transition";
  import { api, isAppError, type Account } from "../lib/api";
  import logo from "../lib/assets/logo.svg";
  import Button from "../lib/components/Button.svelte";
  import Field from "../lib/components/Field.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Segmented from "../lib/components/Segmented.svelte";
  import Toggle from "../lib/components/Toggle.svelte";
  import type { Key } from "../lib/i18n/en";
  import { hasKey, i18n, langName, languages, t, type Lang } from "../lib/i18n/index.svelte";
  import WindowControls from "../shell/WindowControls.svelte";

  /** `created`: a new account, which gets the welcome setup. */
  let { onSignedIn }: { onSignedIn: (account: Account, created?: boolean) => void } = $props();

  type Mode = "signIn" | "create" | "recovery" | "reset";
  let mode = $state<Mode>("signIn");
  let email = $state("");
  let password = $state("");
  let confirm = $state("");
  let recoveryInput = $state("");
  let keep = $state(true);
  let busy = $state(false);
  let errors = $state<Partial<Record<"email" | "password" | "confirm" | "recovery" | "form", string>>>({});
  let created = $state<{ account: Account; recoveryKey: string } | null>(null);
  let copied = $state(false);
  let langOpen = $state(false);

  const looksLikeEmail = (v: string) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v.trim());

  function switchMode(m: Mode) {
    mode = m;
    errors = {};
    password = confirm = "";
  }

  /** Maps a backend error code to the field it belongs to. */
  function showError(e: unknown) {
    const code = isAppError(e) ? e.code : "";
    const key = `error.${code}`;
    const text = hasKey(key) ? t(key as Key) : t("error.generic");
    if (code === "invalid_email" || code === "email_taken" || code === "no_account") errors = { email: text };
    else if (code === "wrong_recovery_key") errors = { recovery: text };
    else if (code === "wrong_password" || code === "password_too_short") errors = { password: text };
    else errors = { form: text };
  }

  function validate(): boolean {
    const next: typeof errors = {};
    if (!looksLikeEmail(email)) next.email = t("error.invalid_email");
    if (mode !== "signIn" && password.length < 8) next.password = t("error.password_too_short");
    if (mode === "signIn" && !password) next.password = t("error.wrong_password");
    if (mode === "create" && password !== confirm) next.confirm = t("error.mismatch");
    if (mode === "reset" && !recoveryInput.trim()) next.recovery = t("error.wrong_recovery_key");
    errors = next;
    return Object.keys(next).length === 0;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy || !validate()) return;
    busy = true;
    try {
      if (mode === "signIn") onSignedIn(await api.signIn(email, password, keep));
      else if (mode === "create") {
        created = await api.signUp(email, password, keep);
        mode = "recovery";
      } else if (mode === "reset") onSignedIn(await api.resetPassword(email, recoveryInput, password));
    } catch (err) {
      showError(err);
    } finally {
      busy = false;
    }
  }

  async function copyKey() {
    if (!created) return;
    await navigator.clipboard.writeText(created.recoveryKey);
    copied = true;
  }

  function pickLang(l: Lang) {
    i18n.lang = l;
    langOpen = false;
  }

  const headline = $derived(mode === "create" || mode === "recovery" ? t("auth.headlineCreate") : t("auth.headlineSignIn"));
  const sub = $derived(mode === "create" || mode === "recovery" ? t("auth.subCreate") : t("auth.subSignIn"));
</script>

<div class="screen">
  <svg class="shapes" viewBox="0 0 1600 1000" preserveAspectRatio="xMidYMid slice" aria-hidden="true">
    <path d="M-120 640a520 520 0 0 1 1040 0z" />
    <rect x="-200" y="700" width="1180" height="120" rx="60" />
    <rect x="980" y="160" width="760" height="120" rx="60" />
    <path d="M1060 120a330 330 0 0 1 660 0z" />
    <rect x="1120" y="860" width="560" height="110" rx="55" />
  </svg>
  <div class="titlebar" data-tauri-drag-region><WindowControls /></div>

  <div class="card" in:fade={{ duration: 220 }}>
    <section class="brand">
      <svg class="giant" viewBox="0 0 100 100" aria-hidden="true"><path d="M24 58a26 26 0 0 1 52 0z" /><rect x="14" y="64" width="72" height="7" rx="3.5" /></svg>
      <div class="logo"><img src={logo} alt="" width="26" height="26" /><span>Relay</span></div>
      <div class="lang">
        <button class="lang-btn" aria-haspopup="listbox" aria-expanded={langOpen} onclick={() => (langOpen = !langOpen)}>
          <Icon name="globe" size={14} />{langName(i18n.lang)}<Icon name="chevronDown" size={14} />
        </button>
        {#if langOpen}
          <div class="lang-menu glass" role="listbox" transition:fade={{ duration: 100 }}>
            {#each languages as l (l)}
              <button role="option" aria-selected={l === i18n.lang} onclick={() => pickLang(l)}>
                {langName(l)}{#if l === i18n.lang}<Icon name="check" size={14} />{/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <div class="copy">
        <h1>{headline}</h1>
        <p>{sub}</p>
      </div>
    </section>

    <section class="form">
      {#if mode === "recovery" && created}
        <div class="recovery">
          <h2>{t("auth.recoveryTitle")}</h2>
          <p>{t("auth.recoveryBody")}</p>
          <div class="key">
            <code>{created.recoveryKey}</code>
            <Button variant="secondary" icon={copied ? "check" : "copy"} onclick={copyKey}>{copied ? t("auth.copied") : t("auth.copy")}</Button>
          </div>
          <Button variant="primary" full onclick={() => created && onSignedIn(created.account, true)}>{t("auth.savedIt")}</Button>
        </div>
      {:else}
        {#if mode === "reset"}
          <div class="reset-head">
            <h2>{t("auth.resetTitle")}</h2>
            <p>{t("auth.resetSub")}</p>
          </div>
        {:else}
          <Segmented
            size="lg"
            label={t("auth.signIn")}
            value={mode}
            options={[
              { value: "signIn", label: t("auth.signIn") },
              { value: "create", label: t("auth.createAccount") },
            ]}
            onchange={(m) => switchMode(m)} />
        {/if}
        <form onsubmit={submit} novalidate>
          <Field label={t("auth.email")} type="email" bind:value={email} placeholder={t("auth.emailPlaceholder")} error={errors.email} autocomplete="email" autofocus />
          {#if mode === "reset"}
            <Field label={t("auth.recoveryKey")} bind:value={recoveryInput} placeholder="XXXX-XXXX-XXXX-XXXX-XXXX" error={errors.recovery} autocomplete="off" />
          {/if}
          <Field
            label={mode === "reset" ? t("auth.newPassword") : t("auth.password")}
            type="password"
            bind:value={password}
            placeholder={mode === "signIn" ? t("auth.passwordPlaceholder") : t("auth.passwordNewPlaceholder")}
            error={errors.password}
            autocomplete={mode === "signIn" ? "current-password" : "new-password"} />
          {#if mode === "create"}
            <Field label={t("auth.confirmPassword")} type="password" bind:value={confirm} placeholder={t("auth.confirmPlaceholder")} error={errors.confirm} autocomplete="new-password" />
          {/if}
          {#if mode !== "reset"}
            <div class="options">
              <label class="keep"><Toggle label={t("auth.keepSignedIn")} checked={keep} onchange={(v) => (keep = v)} />{t("auth.keepSignedIn")}</label>
              {#if mode === "signIn"}<button type="button" class="link" onclick={() => switchMode("reset")}>{t("auth.forgot")}</button>{/if}
            </div>
          {/if}
          {#if errors.form}<p class="form-error"><Icon name="alert" size={14} />{errors.form}</p>{/if}
          <Button variant="primary" type="submit" full {busy}>
            {mode === "signIn" ? t("auth.signIn") : mode === "create" ? t("auth.continue") : t("auth.setPassword")}
          </Button>
          {#if mode === "reset"}
            <button type="button" class="link center" onclick={() => switchMode("signIn")}>{t("auth.backToSignIn")}</button>
          {/if}
        </form>
        <p class="note">{t("auth.localNote")}</p>
      {/if}
    </section>
  </div>
</div>

<style>
  .screen {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: #111113;
    overflow: hidden;
  }
  .shapes {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    fill: rgba(var(--ink), 0.025);
  }
  .titlebar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 48px;
    display: flex;
    justify-content: flex-end;
    align-items: center;
    padding: 0 12px;
  }
  .card {
    position: relative;
    display: grid;
    grid-template-columns: 310px 400px;
    gap: 36px;
    padding: 10px 36px 10px 10px;
    border-radius: 20px;
    background: rgba(var(--panel), 0.9);
    border: 1px solid var(--hairline);
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.45);
  }
  .brand {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 420px;
    padding: 24px;
    overflow: hidden;
    border-radius: 14px;
    background: #202023;
  }
  .giant {
    position: absolute;
    right: -80px;
    bottom: -40px;
    width: 360px;
    height: 360px;
    fill: rgba(var(--ink), 0.04);
  }
  .logo {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--fs-lg);
    font-weight: 700;
  }
  .lang {
    position: relative;
    margin-top: auto;
    margin-bottom: 22px;
  }
  .lang-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-md);
    background: rgba(var(--ink), 0.08);
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .lang-btn:hover {
    background: rgba(var(--ink), 0.12);
  }
  .lang-menu {
    position: absolute;
    left: 0;
    top: 38px;
    z-index: 5;
    min-width: 150px;
    padding: 4px;
    border-radius: var(--r-md);
    background: rgba(var(--panel), 0.92);
  }
  .lang-menu button {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    font-size: var(--fs-sm);
  }
  .lang-menu button:hover {
    background: var(--hover);
  }
  .copy {
    position: relative;
  }
  h1 {
    /* Narrow on purpose so the headline breaks into two balanced lines. */
    max-width: 210px;
    margin: 0 0 10px;
    font-size: 30px;
    line-height: 1.1;
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .copy p {
    margin: 0;
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--text-2);
  }
  .form {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 22px;
    padding: 28px 0;
  }
  form {
    display: grid;
    gap: 16px;
  }
  .options {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .keep {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
    text-decoration: underline;
    text-underline-offset: 3px;
    text-decoration-color: rgba(var(--ink), 0.25);
  }
  .link:hover {
    color: var(--text);
  }
  .center {
    justify-self: center;
  }
  .form-error {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--fs-xs);
  }
  .note {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-3);
    text-align: center;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .reset-head p,
  .recovery p {
    margin: 6px 0 0;
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--text-2);
  }
  .recovery {
    display: grid;
    gap: 18px;
  }
  .key {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 12px 12px 16px;
    border-radius: var(--r-md);
    background: var(--field);
  }
  code {
    font-family: "Cascadia Mono", Consolas, monospace;
    font-size: 15px;
    letter-spacing: 0.04em;
    user-select: all;
  }
</style>
