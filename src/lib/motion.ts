/** The soft spring used when reordering (queue, playlists, speed dial): overshoots slightly,
 * then settles. */
export function spring(x: number): number {
  const s = 0.9;
  const t = x - 1;
  return t * t * ((s + 1) * t + s) + 1;
}
