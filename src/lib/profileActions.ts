// Changing the profile picture, banner and name. Pictures are picked here, cropped in the
// app, and saved by the backend into the app's data folder.
import { api } from "./api";
import { t } from "./i18n/index.svelte";
import { dialogs } from "./stores/dialogs.svelte";
import { fail } from "./stores/library.svelte";
import { session } from "./stores/session.svelte";
import { ui } from "./stores/ui.svelte";

const AVATAR_PX = 512;
const BANNER_PX = 1800;
export const BANNER_ASPECT = 4;

/** Opens the system's file picker for one image. */
export function pickImage(accept: string, onPicked: (file: File) => void): void {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = accept;
  input.onchange = () => {
    const file = input.files?.[0];
    if (file) onPicked(file);
  };
  input.click();
}

export function changeProfileImage(kind: "avatar" | "banner"): void {
  pickImage("image/jpeg,image/png,image/webp", (file) =>
    dialogs.open({
      kind: "crop",
      title: kind === "avatar" ? t("profile.cropAvatar") : t("profile.cropBanner"),
      file,
      aspect: kind === "avatar" ? 1 : BANNER_ASPECT,
      outWidth: kind === "avatar" ? AVATAR_PX : BANNER_PX,
      round: kind === "avatar",
      onDone: async (jpeg) => {
        session.account = await api.profileImageSet(kind, jpeg);
      },
    }),
  );
}

export async function removeProfileImage(kind: "avatar" | "banner"): Promise<void> {
  try {
    session.account = await api.profileImageClear(kind);
  } catch (e) {
    fail(e);
  }
}

export function renameProfile(): void {
  dialogs.open({
    kind: "prompt",
    title: t("profile.rename"),
    label: t("profile.name"),
    value: session.account?.displayName ?? "",
    confirm: t("dialog.save"),
    onConfirm: async (name) => {
      try {
        session.account = await api.profileNameSet(name);
        ui.toast(t("profile.renamed"), "check");
      } catch (e) {
        fail(e);
      }
    },
  });
}
