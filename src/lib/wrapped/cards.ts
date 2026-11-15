// Wrapped: the periods (a month or a year) and the cards shown for them. The same card data
// is drawn on screen and into the saved image, so both always match.
import { artistLine, type Stats } from "../api";
import type { WrappedPeriod } from "../stores/ui.svelte";

export interface CardRow {
  cover: string | null;
  title: string;
  sub: string;
  value: string;
  round?: boolean;
}

export interface Card {
  id: "intro" | "minutes" | "top" | "songs" | "artists" | "hour" | "day" | "streak" | "summary";
  kicker: string;
  title: string;
  /** A big number or date, with a small unit under it. */
  big?: string;
  unit?: string;
  note?: string;
  rows?: CardRow[];
  /** One big cover with its song: the #1 of the period. */
  hero?: CardRow;
  /** The closing card: a few numbers side by side. */
  grid?: { label: string; value: string }[];
}

/** Night owl, early bird…: what the busiest listening hour says. */
function hourKind(hour: number): string {
  if (hour >= 22 || hour < 4) return "wrapped.hourNight";
  if (hour < 10) return "wrapped.hourMorning";
  if (hour < 17) return "wrapped.hourDay";
  return "wrapped.hourEvening";
}

type Translate = (key: string, vars?: Record<string, string | number>) => string;

/** The calendar month `offset` months from `now` (0 = this month), in local time. */
export function monthPeriod(now: Date, offset: number, lang: string): WrappedPeriod {
  const start = new Date(now.getFullYear(), now.getMonth() + offset, 1);
  const end = new Date(start.getFullYear(), start.getMonth() + 1, 1);
  return { kind: "month", from: start.getTime() / 1000, to: end.getTime() / 1000, label: start.toLocaleDateString(lang, { month: "long", year: "numeric" }) };
}

export function yearPeriod(now: Date, offset: number): WrappedPeriod {
  const year = now.getFullYear() + offset;
  const start = new Date(year, 0, 1);
  const end = new Date(year + 1, 0, 1);
  return { kind: "year", from: start.getTime() / 1000, to: end.getTime() / 1000, label: String(year) };
}

export function buildCards(s: Stats, period: WrappedPeriod, lang: string, t: Translate): Card[] {
  const num = (n: number) => n.toLocaleString(lang);
  const plays = (n: number) => t("wrapped.plays", { n, count: num(n) });
  const day = (secs: number, opts: Intl.DateTimeFormatOptions) => new Date(secs * 1000).toLocaleDateString(lang, opts);
  const cards: Card[] = [
    { id: "intro", kicker: period.label, title: period.kind === "year" ? t("wrapped.introYear") : t("wrapped.introMonth") },
    {
      id: "minutes",
      kicker: t("wrapped.minutesKicker"),
      title: "",
      big: num(s.minutes),
      unit: t("wrapped.minutes", { n: s.minutes }),
      note: t("wrapped.minutesNote", { plays: num(s.plays), songs: num(s.songs) }),
    },
  ];
  const first = s.topSongs[0];
  if (first) {
    cards.push({
      id: "top",
      kicker: t("wrapped.topKicker"),
      title: "",
      hero: { cover: first.track.cover, title: first.track.title, sub: artistLine(first.track), value: plays(first.plays) },
    });
  }
  if (s.topSongs.length) {
    cards.push({
      id: "songs",
      kicker: t("wrapped.songsKicker"),
      title: "",
      rows: s.topSongs.map((c) => ({ cover: c.track.cover, title: c.track.title, sub: artistLine(c.track), value: plays(c.plays) })),
    });
  }
  if (s.topArtists.length) {
    cards.push({
      id: "artists",
      kicker: t("wrapped.artistsKicker"),
      title: "",
      rows: s.topArtists.map((a) => ({
        cover: a.cover,
        title: a.name,
        sub: t("wrapped.minutesOf", { n: a.minutes, count: num(a.minutes) }),
        value: plays(a.plays),
        round: true,
      })),
    });
  }
  if (s.peakHour != null) {
    const at = new Date(2000, 0, 1, s.peakHour).toLocaleTimeString(lang, { hour: "numeric", minute: "2-digit" });
    cards.push({ id: "hour", kicker: t("wrapped.hourKicker"), title: "", big: at, unit: t(hourKind(s.peakHour)), note: t("wrapped.hourNote", { time: at }) });
  }
  if (s.topDay) {
    cards.push({
      id: "day",
      kicker: t("wrapped.dayKicker"),
      title: "",
      big: day(s.topDay.day, { day: "numeric", month: "long" }),
      unit: day(s.topDay.day, { weekday: "long" }),
      note: t("wrapped.minutesOf", { n: s.topDay.minutes, count: num(s.topDay.minutes) }),
    });
  }
  if (s.longestStreak >= 2 && s.streakStart != null) {
    cards.push({
      id: "streak",
      kicker: t("wrapped.streakKicker"),
      title: "",
      big: num(s.longestStreak),
      unit: t("wrapped.streakUnit"),
      note: t("wrapped.streakNote", { date: day(s.streakStart, { day: "numeric", month: "long" }) }),
    });
  }
  cards.push({
    id: "summary",
    kicker: period.label,
    title: t("wrapped.summaryTitle"),
    grid: [
      { label: t("wrapped.summaryMinutes"), value: num(s.minutes) },
      { label: t("wrapped.summarySongs"), value: num(s.songs) },
      { label: t("wrapped.summaryArtists"), value: num(s.artists) },
      { label: t("wrapped.summaryTopArtist"), value: s.topArtists[0]?.name ?? "–" },
    ],
  });
  return cards;
}

/** The cover whose colors the cards take: the top song's. */
export function leadCover(s: Stats): string | null {
  return s.topSongs[0]?.track.cover ?? s.topArtists[0]?.cover ?? null;
}
