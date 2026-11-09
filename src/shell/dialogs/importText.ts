// Human wording for import results and failures.
import { isAppError, type Skipped } from "../../lib/api";
import { t } from "../../lib/i18n/index.svelte";

export function errorText(e: unknown): string {
  const code = isAppError(e) ? e.code : "";
  switch (code) {
    case "unsupported_link":
      return t("import.error.unsupported");
    case "not_found":
      return t("import.error.private");
    case "empty_list":
      return t("import.error.empty");
    case "no_links":
      return t("import.error.noLinks");
    case "too_many":
      return t("import.error.tooMany");
    case "bad_file":
      return t("import.error.badFile");
    case "offline":
    case "network":
      return t("error.offline");
    case "timeout":
      return t("error.timeout");
    case "rate_limited":
      return t("error.rate_limited");
    default:
      return t("import.error.generic");
  }
}

export function serviceName(service: string): string {
  switch (service) {
    case "spotify":
      return "Spotify";
    case "deezer":
      return "Deezer";
    case "apple":
      return "Apple Music";
    case "soundcloud":
      return "SoundCloud";
    case "m3u":
      return "M3U";
    default:
      return "YouTube";
  }
}

export function skippedReason(s: Skipped): string {
  if (s.reason === "deleted" || s.reason === "private") return t("report.deleted");
  if (s.reason === "no_match") return t("report.noMatch");
  return t("report.unavailable");
}
