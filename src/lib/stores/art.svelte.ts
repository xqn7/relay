// The playing song's cover and its colors, switched together: a new cover waits until its
// colors are read (and, in fullscreen, until the big picture is loaded), then the cover and
// the backdrop change in the same moment with the same fade. A slow cover is never waited on
// for long: after HOLD_MS it shows anyway and its colors follow.
import { coverAt, proxiedCover } from "../api";
import { neutralColors, type Rgb } from "../backdrop/colors";
import { coverColors } from "../backdrop/extract";

const HOLD_MS = 700;
/** The fullscreen player's picture size. */
export const BIG_ART = 1200;

class NowArt {
  /** The cover on screen now; lags the playing song by the moment its colors take. */
  url = $state<string | null>(null);
  colors = $state.raw<Rgb[]>(neutralColors());
  private seq = 0;

  show(url: string | null, big: boolean): void {
    const id = ++this.seq;
    // Back to the cover already shown: a cover still being prepared is dropped.
    if (url === this.url) return;
    if (!url) {
      this.url = null;
      this.colors = neutralColors();
      return;
    }
    let revealed = false;
    const reveal = () => {
      if (id !== this.seq || revealed) return;
      revealed = true;
      this.url = url;
    };
    const timer = window.setTimeout(reveal, HOLD_MS);
    const colors = coverColors(url).catch(() => neutralColors());
    const picture = big ? decode(proxiedCover(coverAt(url, BIG_ART) ?? url)) : Promise.resolve();
    Promise.all([colors, picture]).then(([c]) => {
      if (id !== this.seq) return;
      clearTimeout(timer);
      this.colors = c;
      reveal();
    });
  }
}

/** Resolves once the image is ready to draw, or failed (then it just shows when it can). */
function decode(src: string): Promise<void> {
  const img = new Image();
  img.src = src;
  return img.decode().catch(() => {});
}

export const nowArt = new NowArt();
