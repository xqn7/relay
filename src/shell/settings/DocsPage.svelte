<!-- Settings → Help → Docs: short how-tos, each with a button that does the thing. -->
<script lang="ts">
  import Button from "../../lib/components/Button.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { settings } from "../../lib/stores/settings.svelte";

  let { onPage }: { onPage: (page: "about" | "youtube") => void } = $props();

  // Key names as printed on the keyboard in the UI language (German keyboards say "Strg").
  const keyName = (combo: string) => combo.replace("Ctrl", t("docs.key.ctrl")).replace("Space", t("docs.key.space"));
  const shortcuts: [string, Key][] = $derived([
    ["Ctrl+K", "docs.keys.search"],
    ["Space", "docs.keys.play"],
    ["Alt+← / Alt+→", "docs.keys.nav"],
    [settings.values.miniShortcut, "docs.keys.mini"],
    ["Ctrl+F", "docs.keys.lyrics"],
  ]);
</script>

<section>
  <h3>{t("docs.import.title")}</h3>
  <div class="card">
    <ol>
      <li>{t("docs.import.1")}</li>
      <li>{t("docs.import.2")}</li>
      <li>{t("docs.import.3")}</li>
    </ol>
    <p class="note">{t("docs.import.note")}</p>
    <div class="actions"><Button variant="secondary" icon="download" onclick={() => dialogs.open({ kind: "import" })}>{t("import.title")}</Button></div>
  </div>
</section>

<section>
  <h3>{t("docs.liked.title")}</h3>
  <div class="card">
    <p class="lead">{t("docs.liked.paste")}</p>
    <p class="lead">{t("docs.liked.export")}</p>
    <div class="actions"><Button variant="secondary" icon="heart" onclick={() => dialogs.open({ kind: "importLiked" })}>{t("liked.import")}</Button></div>
  </div>
</section>

<section>
  <h3>{t("docs.local.title")}</h3>
  <div class="card">
    <p class="lead">{t("docs.local.body")}</p>
  </div>
</section>

<section>
  <h3>{t("docs.broken.title")}</h3>
  <div class="card">
    <ol>
      <li>{t("docs.broken.1")}</li>
      <li>{t("docs.broken.2")}</li>
      <li>{t("docs.broken.3")}</li>
      <li>{t("docs.broken.4")}</li>
    </ol>
    <div class="actions">
      <Button variant="secondary" icon="globe" onclick={() => dialogs.open({ kind: "connection" })}>{t("connection.run")}</Button>
      <Button variant="ghost" onclick={() => onPage("about")}>{t("docs.broken.update")}</Button>
      <Button variant="ghost" onclick={() => onPage("youtube")}>{t("docs.broken.signIn")}</Button>
    </div>
  </div>
</section>

<section>
  <h3>{t("docs.keys.title")}</h3>
  <div class="card keys">
    {#each shortcuts as [combo, label] (label)}
      <div class="key"><span>{t(label)}</span><kbd>{keyName(combo)}</kbd></div>
    {/each}
  </div>
</section>

<style>
  section + section {
    margin-top: 22px;
  }
  ol {
    margin: 0;
    padding: 14px 16px 6px 36px;
    font-size: var(--fs-sm);
    line-height: 1.55;
    color: var(--text-2);
  }
  li + li {
    margin-top: 6px;
  }
  .lead {
    margin: 0;
    padding: 14px 16px 0;
    font-size: var(--fs-sm);
    line-height: 1.55;
    color: var(--text-2);
  }
  .card .note {
    padding-top: 8px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2);
    padding: 12px 16px 16px;
  }
  .keys {
    padding: 6px 0;
  }
  .key {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s4);
    padding: 8px 16px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  kbd {
    padding: 2px 8px;
    border-radius: var(--r-sm);
    background: var(--glass-strong);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text);
  }
</style>
