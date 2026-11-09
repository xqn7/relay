<script lang="ts">
  import { api } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Cover from "../../lib/components/Cover.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Field from "../../lib/components/Field.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { fail, library } from "../../lib/stores/library.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  let { playlistId }: { playlistId: string } = $props();

  const self = $derived(library.overview.playlists.find((p) => p.id === playlistId));
  const others = $derived(library.overview.playlists.filter((p) => p.id !== playlistId));
  let other = $state<string | null>(null);
  let target = $state<"new" | "this" | "other">("new");
  let name = $state("");
  let busy = $state(false);
  const otherName = $derived(others.find((p) => p.id === other)?.name ?? "");

  $effect(() => {
    if (other && !name) name = `${self?.name ?? ""} + ${otherName}`;
  });

  async function merge() {
    if (!other) return;
    busy = true;
    try {
      const into = target === "this" ? playlistId : target === "other" ? other : null;
      const id = await api.playlistMerge(playlistId, other, into, target === "new" ? name.trim() : null);
      await library.refresh();
      dialogs.close();
      ui.toast(t("merge.done"), "merge");
      ui.open({ name: "playlist", id });
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title={t("merge.title", { name: self?.name ?? "" })} sub={t("merge.sub")} width={480} onclose={() => dialogs.close()}>
  {#if others.length === 0}
    <p class="empty">{t("merge.noOthers")}</p>
  {:else}
    <div class="list">
      {#each others as p (p.id)}
        <button class="pick" class:on={other === p.id} onclick={() => (other = p.id)}>
          <Cover src={p.cover} size={36} fallback="list" />
          <span class="name truncate">{p.name}</span>
          <span class="count">{t("library.songs", { n: p.count })}</span>
        </button>
      {/each}
    </div>
    {#if other}
      <div class="target">
        <Segmented
          label={t("merge.into")}
          value={target}
          options={[
            { value: "new", label: t("merge.newPlaylist") },
            { value: "this", label: self?.name ?? "" },
            { value: "other", label: otherName },
          ]}
          onchange={(v) => (target = v)} />
        {#if target === "new"}<Field label={t("playlist.name")} bind:value={name} />{/if}
      </div>
    {/if}
  {/if}
  {#snippet footer()}
    <Button variant="ghost" onclick={() => dialogs.close()}>{t("dialog.cancel")}</Button>
    <Button variant="primary" icon="merge" busy={busy} disabled={!other || (target === "new" && !name.trim())} onclick={merge}>{t("merge.confirm")}</Button>
  {/snippet}
</Dialog>

<style>
  .list {
    display: grid;
    gap: 2px;
    max-height: 260px;
    overflow-y: auto;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--r-md);
    background: transparent;
    text-align: left;
  }
  .pick:hover {
    background: var(--hover);
  }
  .pick.on {
    background: var(--selected);
  }
  .name {
    flex: 1;
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .count {
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  .target {
    display: grid;
    gap: 14px;
    margin-top: 16px;
  }
  .empty {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
</style>
