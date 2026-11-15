<!-- "Connection issue": checks the internet and every service the app uses, and says which one
     is the problem, so it's clear whether it's the connection or a service that's down. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api, type ConnectionCheck } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  const ids: ConnectionCheck["id"][] = ["internet", "youtube", "soundcloud", "spotify", "covers", "lyrics"];
  let checks = $state<ConnectionCheck[] | null>(null);
  let running = $state(false);

  async function run() {
    running = true;
    checks = null;
    try {
      checks = await api.connectionCheck();
    } catch (e) {
      console.error(e);
      checks = ids.map((id) => ({ id, ok: false, code: "generic", detail: String(e) }));
    } finally {
      running = false;
    }
  }

  onMount(run);

  function explain(c: ConnectionCheck): string {
    if (c.ok) return t(`connection.ok.${c.id}` as Key);
    if (c.code === "skipped") return t("connection.skipped");
    if (c.id === "youtube" && c.code === "helper_installing") return t("connection.helper");
    if (c.id === "youtube" && c.code === "signin_required") return t("connection.signin");
    return t(`connection.fail.${c.id}` as Key);
  }

  const allOk = $derived(checks?.every((c) => c.ok) ?? false);

  function copy() {
    if (!checks) return;
    const lines = [`Relay connection check, ${new Date().toISOString()}`, `User agent: ${navigator.userAgent}`, ""];
    for (const c of checks) lines.push(`${c.ok ? "PASS" : "FAIL"}  ${c.id}${c.ok ? "" : `  [${c.code}] ${c.detail}`}`);
    navigator.clipboard.writeText(lines.join("\n")).then(() => ui.toast(t("connection.copied"), "check"));
  }
</script>

<Dialog title={t("connection.title")} sub={checks ? (allOk ? t("connection.allOk") : t("connection.someFailed")) : t("connection.checking")} width={480} onclose={() => dialogs.close()}>
  <ul>
    {#each ids as id (id)}
      {@const c = checks?.find((x) => x.id === id)}
      <li class:fail={c && !c.ok}>
        <span class="mark">
          {#if !c}<Spinner size={16} />{:else if c.ok}<Icon name="check" size={16} />{:else}<Icon name="alert" size={16} />{/if}
        </span>
        <div class="text">
          <div class="name">{t(`connection.name.${id}` as Key)}</div>
          {#if c}<div class="why">{explain(c)}</div>{/if}
        </div>
      </li>
    {/each}
  </ul>
  {#snippet footer()}
    <Button variant="ghost" icon="copy" disabled={!checks} onclick={copy}>{t("connection.copy")}</Button>
    <Button variant="primary" icon="refresh" disabled={running} onclick={run}>{t("connection.retry")}</Button>
  {/snippet}
</Dialog>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    gap: 12px;
    padding: 9px 0;
  }
  li + li {
    border-top: 1px solid var(--hairline);
  }
  .mark {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    color: var(--text-2);
  }
  .fail .mark {
    color: var(--text);
  }
  .name {
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .why {
    margin-top: 2px;
    font-size: var(--fs-sm);
    color: var(--text-2);
    line-height: 1.45;
  }
</style>
