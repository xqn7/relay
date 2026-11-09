<script lang="ts">
  import { untrack } from "svelte";
  import { api, artistLine, type Track } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Cover from "../../lib/components/Cover.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Field from "../../lib/components/Field.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { fail } from "../../lib/stores/library.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  let { track }: { track: Track } = $props();
  // The fields start from the song as it was when the dialog opened.
  let title = $state(untrack(() => track.title));
  let artist = $state(untrack(() => artistLine(track)));
  let busy = $state(false);

  async function save(pickCover: boolean) {
    if (!title.trim()) return;
    busy = true;
    try {
      const saved = await api.localEdit(track.sourceId, title, artist, pickCover);
      if (saved) {
        ui.toast(t("editLocal.saved"), "check");
        dialogs.close();
      }
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title={t("editLocal.title")} sub={t("editLocal.sub")} width={440} onclose={() => dialogs.close()}>
  <form
    class="form"
    onsubmit={(e) => {
      e.preventDefault();
      save(false);
    }}>
    <div class="cover">
      <Cover src={track.cover} size={72} radius={8} />
      <Button variant="secondary" icon="image" disabled={busy} onclick={() => save(true)}>{t("editLocal.cover")}</Button>
    </div>
    <Field label={t("editLocal.songTitle")} bind:value={title} autofocus />
    <Field label={t("editLocal.artist")} bind:value={artist} />
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => dialogs.close()}>{t("dialog.cancel")}</Button>
    <Button variant="primary" busy={busy} disabled={!title.trim()} onclick={() => save(false)}>{t("editLocal.save")}</Button>
  {/snippet}
</Dialog>

<style>
  .form {
    display: grid;
    gap: 14px;
  }
  .cover {
    display: flex;
    align-items: center;
    gap: 16px;
  }
</style>
