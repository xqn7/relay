<script lang="ts">
  import { api, type TrashEntry } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import { i18n, t } from "../lib/i18n/index.svelte";
  import type { IconName } from "../lib/icons";
  import { dialogs } from "../lib/stores/dialogs.svelte";
  import { fail, library } from "../lib/stores/library.svelte";
  import EmptyState from "./EmptyState.svelte";

  const DAY = 86_400;
  let entries = $state<TrashEntry[]>([]);
  let loaded = $state(false);

  async function load() {
    try {
      entries = await api.trashList();
    } catch (e) {
      fail(e);
    } finally {
      loaded = true;
    }
  }

  $effect(() => {
    void library.version;
    load();
  });

  const icon = (k: TrashEntry["kind"]): IconName => (k === "folder" ? "folder" : k === "local" ? "music" : "list");

  function detail(e: TrashEntry): string {
    if (e.kind === "local") return e.detail || t("trash.localSong");
    const n = Number(e.detail);
    return e.kind === "folder" ? t("library.playlists", { n }) : t("library.songs", { n });
  }

  function daysLeft(e: TrashEntry): number {
    return Math.max(0, 30 - Math.floor((Date.now() / 1000 - e.deletedAt) / DAY));
  }

  async function restore(e: TrashEntry) {
    try {
      await api.trashRestore(e.kind, e.id);
      await library.refresh();
    } catch (err) {
      fail(err);
    }
  }

  function remove(e: TrashEntry) {
    dialogs.open({
      kind: "confirm",
      title: t("trash.deleteTitle"),
      body: t("trash.deleteBody", { name: e.name }),
      confirm: t("trash.deleteForever"),
      onConfirm: async () => {
        await api.trashDelete(e.kind, e.id).catch(fail);
        await load();
      },
    });
  }

  function empty() {
    dialogs.open({
      kind: "confirm",
      title: t("trash.emptyTitle"),
      body: t("trash.emptyBody", { n: entries.length }),
      confirm: t("trash.empty"),
      onConfirm: async () => {
        await api.trashEmpty().catch(fail);
        await library.refresh();
      },
    });
  }

  const fmt = $derived(new Intl.DateTimeFormat(i18n.lang, { day: "numeric", month: "short" }));
</script>

<div class="head">
  <div>
    <h1 class="page-title">{t("trash.title")}</h1>
    <p class="sub">{t("trash.sub")}</p>
  </div>
  {#if entries.length}<Button variant="secondary" icon="trash" onclick={empty}>{t("trash.empty")}</Button>{/if}
</div>

{#if loaded && entries.length === 0}
  <EmptyState icon="trash" title={t("trash.emptyState")} />
{:else}
  <div class="list">
    {#each entries as e (e.kind + e.id)}
      <div class="row">
        <div class="art"><Icon name={icon(e.kind)} size={20} /></div>
        <div class="text">
          <div class="name truncate">{e.name}</div>
          <div class="meta truncate">{detail(e)} · {t("trash.deletedOn", { date: fmt.format(new Date(e.deletedAt * 1000)) })} · {t("trash.daysLeft", { n: daysLeft(e) })}</div>
        </div>
        <Button variant="secondary" icon="undo" onclick={() => restore(e)}>{t("trash.restore")}</Button>
        <Button variant="ghost" onclick={() => remove(e)}>{t("trash.deleteForever")}</Button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    margin-bottom: var(--s5);
  }
  .sub {
    margin: 6px 0 0;
    color: var(--text-2);
    font-size: var(--fs-sm);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--s3);
    padding: 8px;
    border-radius: var(--r-md);
  }
  .row:hover {
    background: var(--hover);
  }
  .art {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.07);
    color: var(--text-2);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 500;
  }
  .meta {
    margin-top: 2px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
</style>
