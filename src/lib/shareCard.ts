// "Share as image": a card for one song, drawn like the Wrapped cards (same size, the cover's
// colors behind it, the app's name at the foot), saved as a PNG to post anywhere.
import { artistLine, coverAt, proxiedCover, type Track } from "./api";
import { coverColors } from "./backdrop/extract";
import { neutralColors } from "./backdrop/colors";
import { APP_NAME } from "./brand";
import { backdrop, CARD_H, CARD_W, fit, font, image, loadImage, wrap } from "./wrapped/render";

const SCALE = 2;
const PAD = 44;
const COVER = 360;

export async function renderShareCard(track: Track, kicker: string, sourceLine: string): Promise<Uint8Array> {
  await Promise.all([document.fonts.load("800 34px Inter"), document.fonts.load("600 18px Inter")]).catch(() => {});
  const [colors, cover] = await Promise.all([
    track.cover ? coverColors(track.cover).catch(() => neutralColors()) : Promise.resolve(neutralColors()),
    track.cover ? loadImage(proxiedCover(coverAt(track.cover, 720) ?? track.cover)) : Promise.resolve(null),
  ]);

  const canvas = document.createElement("canvas");
  canvas.width = CARD_W * SCALE;
  canvas.height = CARD_H * SCALE;
  const ctx = canvas.getContext("2d")!;
  ctx.scale(SCALE, SCALE);
  backdrop(ctx, colors);
  const inner = CARD_W - PAD * 2;

  font(ctx, 600, 18, 0.62);
  ctx.fillText(fit(ctx, kicker, inner), PAD, 76);
  image(ctx, cover, PAD, 110, COVER, false);

  font(ctx, 800, 34);
  let y = 110 + COVER + 56;
  for (const line of wrap(ctx, track.title, inner, 2)) {
    ctx.fillText(line, PAD, y);
    y += 40;
  }
  font(ctx, 600, 20, 0.8);
  ctx.fillText(fit(ctx, artistLine(track), inner), PAD, y + 2);

  const footY = CARD_H - PAD;
  font(ctx, 400, 15, 0.62);
  ctx.fillText(fit(ctx, sourceLine, inner - 120), PAD, footY);
  font(ctx, 700, 15, 0.62);
  ctx.fillText(APP_NAME, CARD_W - PAD - ctx.measureText(APP_NAME).width, footY);

  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) throw new Error("the card could not be drawn");
  return new Uint8Array(await blob.arrayBuffer());
}
