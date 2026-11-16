<!-- App lock: shown at start for a remembered account with a PIN. Windows Hello is offered at
     once when it's turned on; the PIN always works. Forgetting the PIN means signing in with
     the password again (that never asks for the PIN). -->
<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { api, isAppError, type Account, type LockStatus } from "../lib/api";
  import Avatar from "../lib/components/Avatar.svelte";
  import Button from "../lib/components/Button.svelte";
  import Field from "../lib/components/Field.svelte";
  import { t } from "../lib/i18n/index.svelte";

  let { account, onUnlocked, onSignOut }: { account: Account; onUnlocked: () => void; onSignOut: () => void } = $props();

  let status = $state<LockStatus | null>(null);
  let pin = $state("");
  let error = $state("");
  let busy = $state(false);

  async function hello() {
    error = "";
    try {
      if (await api.lockUnlockHello(t("lock.helloMessage"))) onUnlocked();
    } catch (e) {
      console.error("Windows Hello failed", e);
      error = t("lock.helloFailed");
    }
  }

  async function unlock(e: SubmitEvent) {
    e.preventDefault();
    error = "";
    busy = true;
    try {
      await api.lockUnlockPin(pin);
      onUnlocked();
    } catch (err) {
      pin = "";
      if (isAppError(err) && err.code === "lock_wait") error = t("lock.wait", { n: Number(err.detail) || 30 });
      else if (isAppError(err) && err.code === "wrong_pin") error = t("lock.wrongPin");
      else error = t("error.generic");
    } finally {
      busy = false;
    }
  }

  onMount(async () => {
    status = await api.lockStatus().catch(() => null);
    if (status?.hello && status.helloAvailable) hello();
  });
</script>

<div class="lock" transition:fade={{ duration: 180 }}>
  <form class="panel glass" onsubmit={unlock}>
    <Avatar src={account.avatar} name={account.displayName} size={72} />
    <h1>{account.displayName}</h1>
    <p class="sub">{t("lock.sub")}</p>
    <!-- svelte-ignore a11y_autofocus -->
    <Field label={t("lock.pin")} type="password" bind:value={pin} error={error} autocomplete="off" autofocus />
    <Button variant="primary" type="submit" full {busy} disabled={pin.length < 4}>{t("lock.unlock")}</Button>
    {#if status?.hello && status.helloAvailable}
      <Button variant="secondary" full icon="lock" onclick={hello}>{t("lock.useHello")}</Button>
    {/if}
    <button type="button" class="link" onclick={onSignOut}>{t("lock.forgot")}</button>
  </form>
</div>

<style>
  .lock {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    background: var(--bg);
  }
  .panel {
    width: min(360px, calc(100vw - 48px));
    display: grid;
    justify-items: center;
    gap: 12px;
    padding: 30px 28px 22px;
    border-radius: 16px;
    background: rgba(var(--panel), 0.82);
  }
  .panel :global(.field) {
    width: 100%;
  }
  h1 {
    margin: 4px 0 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .sub {
    margin: 0 0 6px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .link {
    margin-top: 4px;
    border: 0;
    background: none;
    font: inherit;
    font-size: var(--fs-sm);
    color: var(--text-2);
    cursor: pointer;
  }
  .link:hover {
    color: var(--text);
  }
</style>
