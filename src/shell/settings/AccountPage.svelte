<!-- Settings → Account: change the password, make a new recovery key (the old one stops
     working), and clear listening history. Both account changes ask for the current password,
     so nobody at an unlocked PC can take the account over. -->
<script lang="ts">
  import { api, isAppError, type LockStatus } from "../../lib/api";
  import Toggle from "../../lib/components/Toggle.svelte";
  import Button from "../../lib/components/Button.svelte";
  import Field from "../../lib/components/Field.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { showError } from "../../lib/errors";
  import { recoveryFileText } from "../../lib/recoveryFile";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { history } from "../../lib/stores/history.svelte";
  import { session } from "../../lib/stores/session.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import SettingRow from "./SettingRow.svelte";

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();

  const errorText = (e: unknown) => {
    if (isAppError(e) && e.code === "wrong_password") return t("account.wrongPassword");
    if (isAppError(e) && e.code === "password_too_short") return t("error.password_too_short");
    return t("error.generic");
  };

  // Each form opens from its button and closes again after it's done or cancelled.
  let pwOpen = $state(false);
  let keyOpen = $state(false);
  let lockMode = $state<"on" | "change" | "off" | null>(null);

  // ---- password ----
  let current = $state("");
  let next = $state("");
  let repeat = $state("");
  let pwError = $state("");
  let pwBusy = $state(false);

  async function changePassword() {
    pwError = "";
    if (next !== repeat) {
      pwError = t("error.mismatch");
      return;
    }
    pwBusy = true;
    try {
      await api.accountChangePassword(current, next);
      closePassword();
      ui.toast(t("account.passwordChanged"), "check");
    } catch (e) {
      pwError = errorText(e);
    } finally {
      pwBusy = false;
    }
  }

  function closePassword() {
    current = next = repeat = pwError = "";
    pwOpen = false;
  }

  // ---- recovery key ----
  let keyPassword = $state("");
  let keyError = $state("");
  let keyBusy = $state(false);
  let newKey = $state("");
  let copied = $state(false);
  let saved = $state(false);

  async function renewKey() {
    keyError = "";
    keyBusy = true;
    try {
      newKey = await api.accountNewRecoveryKey(keyPassword);
      keyPassword = "";
      copied = saved = false;
    } catch (e) {
      keyError = errorText(e);
    } finally {
      keyBusy = false;
    }
  }

  function closeKey() {
    keyPassword = keyError = newKey = "";
    keyOpen = false;
  }

  async function copyKey() {
    await navigator.clipboard.writeText(newKey);
    copied = true;
  }

  async function downloadKey() {
    try {
      if (await api.recoveryKeySave(recoveryFileText(session.account?.email ?? "", newKey))) saved = true;
    } catch (e) {
      console.error("saving the recovery key failed", e);
      keyError = t("auth.recoverySaveFailed");
    }
  }

  // ---- app lock ----
  let lock = $state<LockStatus | null>(null);
  let lockPassword = $state("");
  let lockPin = $state("");
  let lockPinAgain = $state("");
  let lockError = $state("");
  let lockBusy = $state(false);
  const refreshLock = () =>
    api
      .lockStatus()
      .then((s) => (lock = s))
      .catch(() => {});
  refreshLock();

  function closeLock() {
    lockPassword = lockPin = lockPinAgain = lockError = "";
    lockMode = null;
  }

  async function saveLock(off: boolean) {
    lockError = "";
    if (!off && lockPin !== lockPinAgain) {
      lockError = t("lock.pinsDiffer");
      return;
    }
    lockBusy = true;
    try {
      if (off) await api.lockOff(lockPassword);
      else await api.lockSetPin(lockPassword, lockPin);
      closeLock();
      ui.toast(off ? t("lock.turnedOff") : t("lock.saved"), "check");
      await refreshLock();
    } catch (e) {
      lockError = isAppError(e) && e.code === "bad_pin" ? t("lock.badPin") : errorText(e);
    } finally {
      lockBusy = false;
    }
  }

  async function toggleHello(on: boolean) {
    try {
      await api.lockSetHello(on);
      await refreshLock();
    } catch (e) {
      showError(t("errctx.hello"), e);
    }
  }

  // ---- history ----
  type Range = "hour" | "day" | "week" | "all";
  let range = $state<Range>("day");
  const ranges = $derived<{ value: Range; label: string }[]>([
    { value: "hour", label: t("account.historyHour") },
    { value: "day", label: t("account.historyDay") },
    { value: "week", label: t("account.historyWeek") },
    { value: "all", label: t("account.historyAll") },
  ]);

  function clearHistory() {
    const label = ranges.find((r) => r.value === range)?.label ?? "";
    dialogs.open({
      kind: "confirm",
      title: t("account.historyConfirmTitle"),
      body: t("account.historyConfirmBody", { range: label.toLowerCase() }),
      confirm: t("account.historyClear"),
      onConfirm: async () => {
        try {
          const n = await api.historyClear(range);
          await history.refresh();
          ui.toast(t("account.historyCleared", { n }), "check");
        } catch (e) {
          showError(t("errctx.history"), e);
        }
      },
    });
  }
</script>

{#if visible("account.password", "account.passwordDesc")}
  <h3>{t("account.password")}</h3>
  {#if pwOpen}
    <div class="card form">
      <p class="desc">{t("account.passwordDesc")}</p>
      <!-- svelte-ignore a11y_autofocus -->
      <Field label={t("account.currentPassword")} type="password" bind:value={current} autocomplete="current-password" autofocus />
      <Field label={t("account.newPassword")} type="password" bind:value={next} autocomplete="new-password" />
      <Field label={t("auth.confirmPassword")} type="password" bind:value={repeat} autocomplete="new-password" error={pwError} />
      <div class="actions">
        <Button variant="ghost" onclick={closePassword}>{t("dialog.cancel")}</Button>
        <Button variant="primary" busy={pwBusy} disabled={!current || !next || !repeat} onclick={changePassword}>{t("account.changePassword")}</Button>
      </div>
    </div>
  {:else}
    <div class="card">
      <SettingRow label={t("account.password")} desc={t("account.passwordDesc")}>
        <Button variant="secondary" onclick={() => (pwOpen = true)}>{t("account.changePassword")}</Button>
      </SettingRow>
    </div>
  {/if}
{/if}

{#if visible("account.recovery", "account.recoveryDesc")}
  <h3>{t("account.recovery")}</h3>
  {#if !keyOpen}
    <div class="card">
      <SettingRow label={t("account.recovery")} desc={t("account.recoveryDesc")}>
        <Button variant="secondary" icon="refresh" onclick={() => (keyOpen = true)}>{t("account.newRecoveryKey")}</Button>
      </SettingRow>
    </div>
  {:else}
  <div class="card form">
    <p class="desc">{t("account.recoveryDesc")}</p>
    {#if newKey}
      <div class="key">
        <code>{newKey}</code>
        <span class="key-actions">
          <Button variant="secondary" icon={copied ? "check" : "copy"} onclick={copyKey}>{copied ? t("auth.copied") : t("auth.copy")}</Button>
          <Button variant="secondary" icon={saved ? "check" : "download"} onclick={downloadKey}>{saved ? t("auth.downloaded") : t("auth.download")}</Button>
        </span>
      </div>
      <p class="desc">{t("account.recoveryShownOnce")}</p>
      {#if keyError}<p class="error">{keyError}</p>{/if}
      <div class="actions">
        <Button variant="primary" onclick={closeKey}>{t("account.done")}</Button>
      </div>
    {:else}
      <!-- svelte-ignore a11y_autofocus -->
      <Field label={t("account.currentPassword")} type="password" bind:value={keyPassword} autocomplete="current-password" error={keyError} autofocus />
      <div class="actions">
        <Button variant="ghost" onclick={closeKey}>{t("dialog.cancel")}</Button>
        <Button variant="primary" icon="refresh" busy={keyBusy} disabled={!keyPassword} onclick={renewKey}>{t("account.newRecoveryKey")}</Button>
      </div>
    {/if}
  </div>
  {/if}
{/if}

{#if visible("lock.title", "lock.desc")}
  <h3>{t("lock.title")}</h3>
  {#if lockMode}
    <div class="card form">
      <p class="desc">{lockMode === "off" ? t("lock.offDesc") : lock?.pin ? t("lock.onDesc") : t("lock.desc")}</p>
      <!-- svelte-ignore a11y_autofocus -->
      <Field label={t("account.currentPassword")} type="password" bind:value={lockPassword} autocomplete="current-password" error={lockMode === "off" ? lockError : ""} autofocus />
      {#if lockMode !== "off"}
        <Field label={lockMode === "change" ? t("lock.newPin") : t("lock.pin")} type="password" bind:value={lockPin} autocomplete="off" placeholder={t("lock.pinHint")} />
        <Field label={t("lock.pinAgain")} type="password" bind:value={lockPinAgain} autocomplete="off" error={lockError} />
      {/if}
      <div class="actions">
        <Button variant="ghost" onclick={closeLock}>{t("dialog.cancel")}</Button>
        {#if lockMode === "off"}
          <Button variant="primary" busy={lockBusy} disabled={!lockPassword} onclick={() => saveLock(true)}>{t("lock.turnOff")}</Button>
        {:else}
          <Button variant="primary" busy={lockBusy} disabled={!lockPassword || !lockPin || !lockPinAgain} onclick={() => saveLock(false)}>{lockMode === "change" ? t("lock.changePin") : t("lock.turnOn")}</Button>
        {/if}
      </div>
    </div>
  {:else}
    <div class="card">
      <SettingRow label={t("lock.title")} desc={lock?.pin ? t("lock.onDesc") : t("lock.desc")}>
        {#if lock?.pin}
          <span class="row-actions">
            <Button variant="secondary" onclick={() => (lockMode = "change")}>{t("lock.changePin")}</Button>
            <Button variant="ghost" onclick={() => (lockMode = "off")}>{t("lock.turnOff")}</Button>
          </span>
        {:else}
          <Button variant="secondary" icon="lock" onclick={() => (lockMode = "on")}>{t("lock.turnOn")}</Button>
        {/if}
      </SettingRow>
      {#if lock?.pin && lock.helloAvailable}
        <SettingRow label={t("lock.hello")} desc={t("lock.helloDesc")}>
          <Toggle label={t("lock.hello")} checked={lock.hello} onchange={toggleHello} />
        </SettingRow>
      {/if}
    </div>
  {/if}
{/if}

{#if visible("account.history", "account.historyDesc")}
  <h3>{t("account.history")}</h3>
  <div class="card">
    <SettingRow label={t("account.history")} desc={t("account.historyDesc")}>
      <Segmented value={range} options={ranges} label={t("account.history")} onchange={(v) => (range = v)} />
    </SettingRow>
    <SettingRow label={t("account.historyClearLabel")} desc={t("account.historyClearDesc")}>
      <Button variant="secondary" icon="trash" onclick={clearHistory}>{t("account.historyClear")}</Button>
    </SettingRow>
  </div>
{/if}

<style>
  .form {
    display: grid;
    gap: 12px;
    padding: 16px;
  }
  .desc {
    margin: 0;
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--text-2);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .row-actions {
    display: flex;
    gap: 6px;
  }
  .key {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 12px 12px 16px;
    border-radius: var(--r-md);
    background: var(--field);
  }
  .key-actions {
    display: flex;
    gap: 6px;
  }
  code {
    font-family: "Cascadia Mono", Consolas, monospace;
    font-size: var(--fs-md);
    letter-spacing: 0.04em;
  }
  .error {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text);
  }
</style>
