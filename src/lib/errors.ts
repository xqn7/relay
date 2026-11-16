// Smart errors: every error the user sees gets a name, what it means and how to fix it, with
// the fix as a button where Relay can do it (the connection check, YouTube sign-in). Shown as
// a toast with "Details", which opens the full explanation and a prefilled problem report.
import { isAppError } from "./api";
import type { Key } from "./i18n/en";
import { t } from "./i18n/index.svelte";
import { dialogs } from "./stores/dialogs.svelte";
import { ui } from "./stores/ui.svelte";

/** Codes with their own explanation; anything else is explained as "unknown". */
const KNOWN = new Set(["offline", "network", "timeout", "rate_limited", "service_error", "forbidden", "not_found", "unavailable", "preview_only", "no_match", "nothing_playable", "signin_required", "youtube_expired", "helper_installing", "helper_blocked", "install_failed", "source_failed", "bad_response", "bad_file", "too_large", "io", "internal", "unknown"]);

/** What a button in the explanation does. */
export type ErrorFix = "connection" | "youtube";

export interface Explained {
  /** The code as the backend named it (shown so a report can mention it). */
  code: string;
  detail: string;
  name: string;
  meaning: string;
  fix: string;
  action: ErrorFix | null;
}

function actionFor(code: string): ErrorFix | null {
  if (["offline", "network", "timeout", "forbidden", "service_error", "install_failed"].includes(code)) return "connection";
  if (code === "signin_required" || code === "youtube_expired") return "youtube";
  return null;
}

export function explain(e: unknown): Explained {
  const code = isAppError(e) ? e.code : "unknown";
  const detail = isAppError(e) ? e.detail : e instanceof Error ? e.message : String(e ?? "");
  const known = KNOWN.has(code) ? code : "unknown";
  return {
    code,
    detail,
    name: t(`err.${known}.name` as Key),
    meaning: t(`err.${known}.meaning` as Key),
    fix: t(`err.${known}.fix` as Key),
    action: actionFor(code),
  };
}

/** A short toast ("<what failed>: <error name>") with Details for the whole explanation. */
export function showError(what: string, e: unknown, context = ""): void {
  console.error(what, e);
  const x = explain(e);
  ui.toast(`${what}: ${x.name}`, "alert", { label: t("err.details"), run: () => dialogs.open({ kind: "error", error: x, context }) });
}
