// Draws the lock screen picture for a song: the cover large in the middle on its own blurred
// colors, title and artist under it. Sized to the screen, so Windows doesn't crop it oddly.
import { coverAt, proxiedCover } from "../api";
import { fit, font, loadImage } from "../wrapped/render";

const BG = "#0a0a0b";

export async function renderLockArt(title: string, artist: string, coverUrl: string | null): Promise<Uint8Array> {
  const w = Math.round(screen.width * devicePixelRatio);
  const h = Math.round(screen.height * devicePixelRatio);
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d")!;
  const img = coverUrl ? await loadImage(proxiedCover(coverAt(coverUrl, 1200) ?? coverUrl)) : null;

  ctx.fillStyle = BG;
  ctx.fillRect(0, 0, w, h);
  if (img) {
    // The cover itself, blown up and blurred, gives the colors; dimmed so text stays readable.
    ctx.save();
    ctx.filter = `blur(${Math.round(h / 14)}px) saturate(1.2)`;
    ctx.globalAlpha = 0.55;
    const side = Math.max(w, h) * 1.2;
    ctx.drawImage(img, (w - side) / 2, (h - side) / 2, side, side);
    ctx.restore();
    ctx.fillStyle = "rgba(10,10,11,0.35)";
    ctx.fillRect(0, 0, w, h);
  }

  // Windows draws the clock at the top, so the song sits a little below the middle.
  const size = Math.round(h * 0.34);
  const x = (w - size) / 2;
  const y = Math.round(h * 0.4);
  if (img) {
    ctx.save();
    ctx.shadowColor = "rgba(0,0,0,0.45)";
    ctx.shadowBlur = size / 10;
    ctx.shadowOffsetY = size / 40;
    ctx.beginPath();
    ctx.roundRect(x, y, size, size, size / 30);
    ctx.fillStyle = BG;
    ctx.fill();
    ctx.clip();
    ctx.shadowColor = "transparent";
    ctx.drawImage(img, x, y, size, size);
    ctx.restore();
  }

  ctx.textAlign = "center";
  ctx.textBaseline = "top";
  const titleSize = Math.round(h * 0.03);
  font(ctx, 700, titleSize);
  ctx.fillText(fit(ctx, title, w * 0.6), w / 2, y + size + titleSize * 1.1);
  font(ctx, 500, Math.round(titleSize * 0.7), 0.7);
  ctx.fillText(fit(ctx, artist, w * 0.6), w / 2, y + size + titleSize * 2.5);

  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) throw new Error("lock screen image could not be drawn");
  return new Uint8Array(await blob.arrayBuffer());
}
