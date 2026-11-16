<!-- "Send a problem report": what went wrong in the user's words, an optional screenshot, and
     the app's log. Names, emails and folder paths are removed before it's sent. -->
<script lang="ts">
  import { untrack } from "svelte";
  import { api, isAppError } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { pickImage } from "../../lib/profileActions";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  const MAX_PICTURE_BYTES = 8 * 1024 * 1024;

  /** Filled in when opened from an error (the song that couldn't play), ready to send. */
  let { prefill = "" }: { prefill?: string } = $props();

  let text = $state(untrack(() => prefill));
  let picture = $state<File | null>(null);
  let preview = $state("");
  let sending = $state(false);
  let error = $state("");

  function attach() {
    pickImage("image/png,image/jpeg", (file) => {
      if (file.size > MAX_PICTURE_BYTES) {
        error = t("problem.tooLarge");
        return;
      }
      error = "";
      if (preview) URL.revokeObjectURL(preview);
      picture = file;
      preview = URL.createObjectURL(file);
    });
  }

  function removePicture() {
    if (preview) URL.revokeObjectURL(preview);
    picture = null;
    preview = "";
  }

  async function send() {
    sending = true;
    error = "";
    try {
      const bytes = picture ? new Uint8Array(await picture.arrayBuffer()) : new Uint8Array();
      await api.reportProblem(text.trim(), bytes);
      removePicture();
      dialogs.close();
      ui.toast(t("problem.sent"), "check");
    } catch (e) {
      error = isAppError(e) && (e.code === "offline" || e.code === "network" || e.code === "timeout") ? t("error.offline") : t("problem.failed");
    } finally {
      sending = false;
    }
  }
</script>

<Dialog title={t("problem.title")} sub={t("problem.sub")} width={480} onclose={() => (removePicture(), dialogs.close())}>
  <!-- svelte-ignore a11y_autofocus -->
  <textarea bind:value={text} placeholder={t("problem.placeholder")} maxlength="2000" autofocus></textarea>
  {#if preview}
    <div class="picture">
      <img src={preview} alt="" />
      <button class="remove" aria-label={t("problem.removePicture")} onclick={removePicture}><Icon name="close" size={14} /></button>
    </div>
  {/if}
  <p class="note">{error || t("problem.privacy")}</p>
  {#snippet footer()}
    <Button variant="ghost" icon="image" onclick={attach}>{picture ? t("problem.otherPicture") : t("problem.addPicture")}</Button>
    <Button variant="primary" busy={sending} disabled={!text.trim()} onclick={send}>{t("problem.send")}</Button>
  {/snippet}
</Dialog>

<style>
  textarea {
    width: 100%;
    height: 140px;
    padding: 10px 12px;
    resize: none;
    border: 0;
    border-radius: var(--r-md);
    outline: none;
    background: var(--field);
    font: inherit;
    font-size: var(--fs-sm);
    color: var(--text);
  }
  .picture {
    position: relative;
    width: fit-content;
    margin-top: 10px;
  }
  .picture img {
    display: block;
    max-width: 100%;
    max-height: 140px;
    border-radius: var(--r-md);
  }
  .remove {
    position: absolute;
    top: 6px;
    right: 6px;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    cursor: pointer;
  }
  .note {
    margin: 10px 0 0;
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--text-3);
  }
</style>
