// "Download as MP3" and clip export: picks a folder (in the backend), shows progress, then
// says where the files went.
import { listen } from "@tauri-apps/api/event";
import { api, type Track } from "./api";
import { t } from "./i18n/index.svelte";
import { fail } from "./stores/library.svelte";
import { ui } from "./stores/ui.svelte";

let busy = false;

export async function exportSongs(tracks: Track[], clip?: { startMs: number; endMs: number }): Promise<void> {
  if (busy) {
    ui.toast(t("export.busy"), "info");
    return;
  }
  busy = true;
  let toastId: number | null = null;
  const label = clip ? t("export.clipSaving") : t("export.saving", { n: tracks.length });
  const off = await listen<{ done: number; total: number }>("export-progress", ({ payload }) => {
    if (payload.done < payload.total) toastId = ui.progressToast(toastId, label, payload.total > 0 ? payload.done / payload.total : 0);
  });
  try {
    const done = await api.exportMp3(tracks, clip?.startMs ?? null, clip?.endMs ?? null);
    if (toastId != null) ui.dismiss(toastId);
    if (!done) return;
    const show = { label: t("export.show"), run: () => api.exportReveal().catch(fail) };
    if (done.saved > 0) ui.toast(clip ? t("export.clipSaved") : t("export.saved", { n: done.saved }), "check", show);
    if (done.failed.length > 0) ui.toast(t("export.failed", { list: done.failed.slice(0, 3).join(", "), n: done.failed.length }), "alert");
  } catch (e) {
    if (toastId != null) ui.dismiss(toastId);
    fail(e);
  } finally {
    off();
    busy = false;
  }
}
