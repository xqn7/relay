// Running imports. They keep going when the import dialog is closed; progress then shows as a
// toast, and the result offers the report of songs that couldn't be matched or played.
import { listen } from "@tauri-apps/api/event";
import { api, type ImportDone, type ImportProgress } from "../api";
import { t } from "../i18n/index.svelte";
import { dialogs } from "./dialogs.svelte";
import { library } from "./library.svelte";
import { ui } from "./ui.svelte";

export interface Job {
  jobId: string;
  title: string;
  phase: ImportProgress["phase"] | "starting";
  done: number;
  total: number;
  result: ImportDone | null;
}

class ImportsStore {
  jobs = $state<Job[]>([]);
  private toastIds = new Map<string, number>();
  private attached = false;
  /** Jobs whose progress the open dialog is showing (no toast for those). */
  watched = $state<string | null>(null);

  async attach(): Promise<void> {
    if (this.attached) return;
    this.attached = true;
    await listen<ImportProgress>("import-progress", ({ payload }) => this.onProgress(payload));
    await listen<ImportDone>("import-done", ({ payload }) => this.onDone(payload));
  }

  add(jobId: string, title: string): void {
    this.jobs = [...this.jobs, { jobId, title, phase: "starting", done: 0, total: 0, result: null }];
  }

  get(jobId: string | null): Job | undefined {
    return this.jobs.find((j) => j.jobId === jobId);
  }

  cancel(jobId: string): void {
    api.importCancel(jobId).catch(() => {});
  }

  private onProgress(p: ImportProgress): void {
    this.jobs = this.jobs.map((j) => (j.jobId === p.jobId ? { ...j, phase: p.phase, done: p.done, total: p.total } : j));
    const job = this.get(p.jobId);
    if (!job || this.watched === p.jobId) return;
    const text = t(p.phase === "reading" ? "import.reading" : "import.matching", { done: p.done, total: p.total });
    this.toastIds.set(p.jobId, ui.progressToast(this.toastIds.get(p.jobId) ?? null, `${job.title}: ${text}`, p.total ? p.done / p.total : 0));
  }

  private onDone(d: ImportDone): void {
    const job = this.get(d.jobId);
    this.jobs = this.jobs.map((j) => (j.jobId === d.jobId ? { ...j, result: d } : j));
    library.refresh().catch(() => {});
    const toastId = this.toastIds.get(d.jobId) ?? null;
    this.toastIds.delete(d.jobId);
    if (this.watched === d.jobId) {
      if (toastId != null) ui.dismiss(toastId);
      return;
    }
    const title = d.title || job?.title || "";
    if (d.cancelled) {
      ui.progressToast(toastId, t("import.cancelled"), null);
      return;
    }
    if (!d.ok) {
      ui.progressToast(toastId, t("import.failed", { title }), null);
      return;
    }
    const problems = d.unmatched.length + d.skipped.length;
    const text = problems > 0 ? t("import.doneWithProblems", { title, n: problems }) : t("import.done", { title });
    const action = problems > 0 ? { label: t("import.showReport"), run: () => openReport(d, title) } : undefined;
    ui.progressToast(toastId, text, null, action);
  }

  forget(jobId: string): void {
    this.jobs = this.jobs.filter((j) => j.jobId !== jobId);
  }
}

export function openReport(d: ImportDone, title: string): void {
  dialogs.open({ kind: "report", title, playlistId: d.playlistId, unmatched: d.unmatched, skipped: d.skipped, truncated: d.truncated });
}

export const imports = new ImportsStore();
