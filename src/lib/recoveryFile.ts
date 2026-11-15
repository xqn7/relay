// The text of a saved recovery key file, the same from sign-up and from Settings → Account.
import { t } from "./i18n/index.svelte";

export function recoveryFileText(email: string, key: string): string {
  return [t("auth.recoveryFileTitle"), "", `${t("auth.email")}: ${email}`, `${t("auth.recoveryKey")}: ${key}`, "", t("auth.recoveryFileNote"), ""].join("\n");
}
