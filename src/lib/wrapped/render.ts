// Draws a Wrapped card into a PNG (1080 × 1350) to save and share. Laid out on the same
// 540 × 675 grid as the card on screen, at twice the size.
import { coverAt, proxiedCover } from "../api";
import type { Rgb } from "../backdrop/colors";
import type { Card } from "./cards";

export const CARD_W = 540;
export const CARD_H = 675;
const SCALE = 2;
const PAD = 44;
const FONT = "Inter, 'Segoe UI', sans-serif";
const BG = "#0a0a0b";

function loadImage(url: string): Promise<HTMLImageElement | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => resolve(img);
    img.onerror = () => resolve(null);
    img.src = url;
  });
}

const cover = (url: string | null) => (url ? loadImage(proxiedCover(coverAt(url, 240) ?? url)) : Promise.resolve(null));

/** Cuts text to fit `max` pixels, ending in "…". */
function fit(ctx: CanvasRenderingContext2D, text: string, max: number): string {
  if (ctx.measureText(text).width <= max) return text;
  let cut = text;
  while (cut.length > 1 && ctx.measureText(`${cut}…`).width > max) cut = cut.slice(0, -1);
  return `${cut.trimEnd()}…`;
}

function font(ctx: CanvasRenderingContext2D, weight: number, size: number, alpha = 1): void {
  ctx.font = `${weight} ${size}px ${FONT}`;
  ctx.fillStyle = `rgba(255,255,255,${alpha})`;
}

function image(ctx: CanvasRenderingContext2D, img: HTMLImageElement | null, x: number, y: number, size: number, round: boolean, initial = ""): void {
  ctx.save();
  ctx.beginPath();
  if (round) ctx.arc(x + size / 2, y + size / 2, size / 2, 0, Math.PI * 2);
  else ctx.roundRect(x, y, size, size, 8);
  ctx.clip();
  ctx.fillStyle = "rgba(255,255,255,0.14)";
  ctx.fillRect(x, y, size, size);
  if (img) {
    // Like object-fit: cover.
    const s = Math.max(size / img.naturalWidth, size / img.naturalHeight);
    const w = img.naturalWidth * s;
    const h = img.naturalHeight * s;
    ctx.drawImage(img, x + (size - w) / 2, y + (size - h) / 2, w, h);
  } else if (initial) {
    font(ctx, 700, size * 0.42);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(initial, x + size / 2, y + size / 2 + 1);
    ctx.textAlign = "left";
    ctx.textBaseline = "alphabetic";
  }
  ctx.restore();
}

/** The cover colors as soft light from the top, melting into near-black (like the app). */
function backdrop(ctx: CanvasRenderingContext2D, colors: Rgb[]): void {
  ctx.fillStyle = BG;
  ctx.fillRect(0, 0, CARD_W, CARD_H);
  const spots: [number, number, number][] = [
    [80, 40, 340],
    [300, 0, 320],
    [500, 120, 300],
  ];
  spots.forEach(([x, y, r], i) => {
    const c = colors[i] ?? colors[0];
    if (!c) return;
    const g = ctx.createRadialGradient(x, y, 0, x, y, r);
    g.addColorStop(0, `rgba(${c[0]},${c[1]},${c[2]},0.85)`);
    g.addColorStop(1, `rgba(${c[0]},${c[1]},${c[2]},0)`);
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, CARD_W, CARD_H);
  });
  const shade = ctx.createLinearGradient(0, 0, 0, CARD_H);
  shade.addColorStop(0, "rgba(10,10,11,0)");
  shade.addColorStop(0.45, "rgba(10,10,11,0.6)");
  shade.addColorStop(0.8, BG);
  ctx.fillStyle = shade;
  ctx.fillRect(0, 0, CARD_W, CARD_H);
}

/** Splits text into at most `lines` lines that fit `max` pixels. */
function wrap(ctx: CanvasRenderingContext2D, text: string, max: number, lines: number): string[] {
  const out: string[] = [];
  let line = "";
  for (const word of text.split(/\s+/)) {
    const next = line ? `${line} ${word}` : word;
    if (ctx.measureText(next).width <= max || !line) line = next;
    else {
      out.push(line);
      line = word;
    }
  }
  if (line) out.push(line);
  if (out.length > lines) {
    const kept = out.slice(0, lines);
    kept[lines - 1] = fit(ctx, `${kept[lines - 1]} ${out.slice(lines).join(" ")}`, max);
    return kept;
  }
  return out.map((l) => fit(ctx, l, max));
}

export async function renderCard(card: Card, colors: Rgb[], who: { name: string; avatar: string | null }): Promise<Uint8Array> {
  await Promise.all([document.fonts.load(`800 40px Inter`), document.fonts.load(`600 18px Inter`), document.fonts.load(`400 16px Inter`)]).catch(() => {});
  const [avatar, ...covers] = await Promise.all([who.avatar ? loadImage(who.avatar) : Promise.resolve(null), ...(card.rows ?? []).map((r) => cover(r.cover))]);

  const canvas = document.createElement("canvas");
  canvas.width = CARD_W * SCALE;
  canvas.height = CARD_H * SCALE;
  const ctx = canvas.getContext("2d")!;
  ctx.scale(SCALE, SCALE);
  backdrop(ctx, colors);
  const inner = CARD_W - PAD * 2;
  const initial = who.name.slice(0, 1).toUpperCase();

  font(ctx, 600, 18, 0.62);
  ctx.fillText(fit(ctx, card.kicker, inner), PAD, 76);
  let y = 76;

  if (card.id === "intro") {
    image(ctx, avatar, PAD, 140, 132, true, initial);
    font(ctx, 800, 44);
    y = 340;
    for (const line of wrap(ctx, card.title, inner, 3)) {
      ctx.fillText(line, PAD, y);
      y += 52;
    }
    font(ctx, 600, 20, 0.62);
    ctx.fillText(fit(ctx, who.name, inner), PAD, y + 8);
  } else if (card.title) {
    font(ctx, 800, 34);
    y = 124;
    ctx.fillText(fit(ctx, card.title, inner), PAD, y);
  }

  if (card.big) {
    font(ctx, 800, 84);
    ctx.fillText(fit(ctx, card.big, inner), PAD, 300);
    font(ctx, 600, 22, 0.8);
    ctx.fillText(fit(ctx, card.unit ?? "", inner), PAD, 340);
    y = 340;
  }

  card.rows?.forEach((row, i) => {
    const top = 118 + i * 84;
    image(ctx, covers[i] ?? null, PAD, top, 60, !!row.round);
    font(ctx, 400, 15, 0.62);
    const valueW = ctx.measureText(row.value).width;
    ctx.fillText(row.value, CARD_W - PAD - valueW, top + 36);
    const textW = inner - 76 - valueW - 16;
    font(ctx, 600, 18);
    ctx.fillText(fit(ctx, row.title, textW), PAD + 76, top + 26);
    font(ctx, 400, 15, 0.62);
    ctx.fillText(fit(ctx, row.sub, textW), PAD + 76, top + 48);
  });

  if (card.note) {
    font(ctx, 400, 18, 0.62);
    let noteY = (card.rows ? 118 + card.rows.length * 84 : y) + 44;
    for (const line of wrap(ctx, card.note, inner, 3)) {
      ctx.fillText(line, PAD, noteY);
      noteY += 26;
    }
  }

  // Footer: who it belongs to, and the app.
  const footY = CARD_H - PAD - 28;
  image(ctx, avatar, PAD, footY, 28, true, initial);
  font(ctx, 600, 15);
  ctx.fillText(fit(ctx, who.name, inner - 120), PAD + 38, footY + 19);
  font(ctx, 700, 15, 0.62);
  const brand = "Relay";
  ctx.fillText(brand, CARD_W - PAD - ctx.measureText(brand).width, footY + 19);

  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) throw new Error("the card could not be drawn");
  return new Uint8Array(await blob.arrayBuffer());
}
