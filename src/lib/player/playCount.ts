// When a play counts: after about 30 seconds of actual listening, or half the song if that
// comes first. Seeking doesn't count as listening, so skipping through a song isn't a play.

export const COUNT_AFTER_MS = 30_000;
/** Position steps bigger than this between two reports are seeks, not listening. */
const MAX_STEP_MS = 2_500;

export class PlayCounter {
  /** The playback being counted (a queue entry's uid). */
  key: string | null = null;
  listenedMs = 0;
  counted = false;
  private lastPos: number | null = null;

  /** A new playback starts (also when the same song starts over). */
  start(key: string): void {
    this.key = key;
    this.listenedMs = 0;
    this.counted = false;
    this.lastPos = null;
  }

  /** Fed the position as it moves; true exactly once, when the play starts to count. */
  tick(key: string, posMs: number, durationMs: number): boolean {
    if (key !== this.key) return false;
    if (this.lastPos != null) {
      const step = posMs - this.lastPos;
      if (step > 0 && step < MAX_STEP_MS) this.listenedMs += step;
    }
    this.lastPos = posMs;
    if (this.counted) return false;
    const needed = durationMs > 0 ? Math.min(COUNT_AFTER_MS, durationMs / 2) : COUNT_AFTER_MS;
    if (this.listenedMs < needed) return false;
    this.counted = true;
    return true;
  }
}
