<!-- Settings → Account: change the password, make a new recovery key (the old one stops
     working), and clear listening history. Both account changes ask for the current password,
     so nobody at an unlocked PC can take the account over. -->
<script lang="ts">
  import { api, isAppError } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Field from "../../lib/components/Field.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
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
      current = next = repeat = "";
      ui.toast(t("account.passwordChanged"), "check");
    } catch (e) {
      pwError = errorText(e);
    } finally {
      pwBusy = false;
    }
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
          console.error("clearing history failed", e);
          ui.toast(t("error.generic"), "alert");
        }
      },
    });
  }
</script>

{#if visible("account.password", "account.passwordDesc")}
  <h3>{t("account.password")}</h3>
  <div class="card form">
    <p class="desc">{t("account.passwordDesc")}</p>
    <Field label={t("account.currentPassword")} type="password" bind:value={current} autocomplete="current-password" />
    <Field label={t("account.newPassword")} type="password" bind:value={next} autocomplete="new-password" />
    <Field label={t("auth.confirmPassword")} type="password" bind:value={repeat} autocomplete="new-password" error={pwError} />
    <div class="actions">
      <Button variant="primary" busy={pwBusy} disabled={!current || !next || !repeat} onclick={changePassword}>{t("account.changePassword")}</Button>
    </div>
  </div>
{/if}

{#if visible("account.recovery", "account.recoveryDesc")}
  <h3>{t("account.recovery")}</h3>
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
    {:else}
      <Field label={t("account.currentPassword")} type="password" bind:value={keyPassword} autocomplete="current-password" error={keyError} />
      <div class="actions">
        <Button variant="secondary" icon="refresh" busy={keyBusy} disabled={!keyPassword} onclick={renewKey}>{t("account.newRecoveryKey")}</Button>
      </div>
    {/if}
  </div>
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
