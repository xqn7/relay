// Windowing for long song lists: only the rows near the visible area are rendered.

/** Rows [start, end) to render for a list whose top edge is `top` px below the top of its
 * scroll viewport (negative once scrolled past). */
export function visibleRange(top: number, viewport: number, rowHeight: number, count: number, overscan: number): { start: number; end: number } {
  if (count === 0 || rowHeight <= 0) return { start: 0, end: 0 };
  const first = Math.floor(-top / rowHeight);
  const last = Math.ceil((viewport - top) / rowHeight);
  const start = Math.min(count, Math.max(0, first - overscan));
  const end = Math.max(start, Math.min(count, last + overscan));
  return { start, end };
}

/** Where a dragged row would be inserted, from the pointer's y relative to the list's top. */
export function insertIndex(y: number, rowHeight: number, count: number): number {
  return Math.max(0, Math.min(count, Math.round(y / rowHeight)));
}
