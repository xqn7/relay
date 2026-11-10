import { coverAt, proxiedCover } from "../api";
import { dominantColors, toneForBackdrop, type Rgb } from "./colors";

/** The 3 backdrop colors of a cover (read through the app's proxy, which allows reading pixels). */
export async function coverColors(url: string): Promise<Rgb[]> {
  const img = new Image();
  img.crossOrigin = "anonymous";
  img.src = proxiedCover(coverAt(url, 120) ?? url);
  await img.decode();
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 32;
  const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
  ctx.drawImage(img, 0, 0, 32, 32);
  return dominantColors(ctx.getImageData(0, 0, 32, 32).data, 3).map(toneForBackdrop);
}

export const rgba = (c: Rgb | undefined, a: number): string => (c ? `rgba(${c[0]},${c[1]},${c[2]},${a})` : "transparent");
