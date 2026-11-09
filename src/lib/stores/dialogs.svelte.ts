// One dialog at a time, opened from anywhere (menus, pages, the sidebar).
import type { Skipped, Track } from "../api";

export type Dialog =
  | { kind: "prompt"; title: string; label: string; value: string; confirm: string; onConfirm: (value: string) => void }
  | { kind: "confirm"; title: string; body: string; confirm: string; onConfirm: () => void }
  | { kind: "import"; url?: string }
  | { kind: "importLiked" }
  | { kind: "report"; title: string; playlistId: string | null; unmatched: Track[]; skipped: Skipped[]; truncated: boolean }
  | { kind: "fixMatch"; track: Track }
  | { kind: "editLocal"; track: Track }
  | { kind: "merge"; playlistId: string };

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
