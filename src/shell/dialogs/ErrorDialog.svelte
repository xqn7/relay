<!-- An error explained: its name, what it means and how to fix it, with the fix as a button
     when Relay can do it, and a problem report already filled in with the details. -->
<script lang="ts">
  import Button from "../../lib/components/Button.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import type { Explained } from "../../lib/errors";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { youtube } from "../../lib/stores/youtube.svelte";

  let { error, context }: { error: Explained; context: string } = $props();

  function fix() {
    if (error.action === "connection") dialogs.open({ kind: "connection" });
    else if (error.action === "youtube") {
      dialogs.close();
      youtube.signIn();
    }
  }

  function report() {
    const lines = [context, `${error.name} (${error.code})`, error.detail, "", ""].filter((l, i) => l || i >= 3);
    dialogs.open({ kind: "problem", text: lines.join("\n") });
  }
</script>

<Dialog title={error.name} sub={t("err.code", { code: error.code })} width={460} onclose={() => dialogs.close()}>
  {#if context}<p class="context">{context}</p>{/if}
  <h4>{t("err.meaning")}</h4>
  <p>{error.meaning}</p>
  <h4>{t("err.fix")}</h4>
  <p>{error.fix}</p>
  {#snippet footer()}
    <Button variant="ghost" icon="alert" onclick={report}>{t("err.report")}</Button>
    {#if error.action === "connection"}
      <Button variant="primary" icon="globe" onclick={fix}>{t("err.checkConnection")}</Button>
    {:else if error.action === "youtube"}
      <Button variant="primary" icon="youtube" onclick={fix}>{t("err.youtubeSettings")}</Button>
    {:else}
      <Button variant="primary" onclick={() => dialogs.close()}>{t("account.done")}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .context {
    margin: 0 0 12px;
    font-weight: 600;
  }
  h4 {
    margin: 12px 0 4px;
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-3);
  }
  p {
    margin: 0;
    font-size: var(--fs-sm);
    line-height: 1.55;
    color: var(--text-2);
  }
</style>
