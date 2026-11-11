// One dialog at a time, opened from anywhere (menus, pages, the sidebar).
import type { HardwareInfo, Skipped, Track, UpdateInfo } from "../api";

export type Dialog =
  | { kind: "prompt"; title: string; label: string; value: string; confirm: string; onConfirm: (value: string) => void }
  | { kind: "confirm"; title: string; body: string; confirm: string; onConfirm: () => void }
  | { kind: "import"; url?: string }
  | { kind: "importLiked" }
  | { kind: "report"; title: string; playlistId: string | null; unmatched: Track[]; skipped: Skipped[]; truncated: boolean }
  | { kind: "fixMatch"; track: Track }
  | { kind: "editLocal"; track: Track }
  | { kind: "merge"; playlistId: string }
  | { kind: "connection" }
  | { kind: "update"; info: UpdateInfo }
  | { kind: "hardware"; info: HardwareInfo }
  | { kind: "versions"; track: Track; playlist?: { id: string; itemId: string; onReplaced: () => void } }
  /** Crops a picked image to `aspect` (width / height) and hands back a JPEG `outWidth` wide. */
  | { kind: "crop"; title: string; file: File; aspect: number; outWidth: number; round?: boolean; onDone: (jpeg: Uint8Array) => Promise<void> };

class DialogStore {
  current = $state<Dialog | null>(null);

  open(d: Dialog): void {
    this.current = d;
  }

  close(): void {
    this.current = null;
  }
}

export const dialogs = new DialogStore();
