import { en, type Key } from "./en";
import { DEV_BUILD } from "../brand";
import { pickPlural } from "./plural";

// Adding a language: create xx.ts with every key (TypeScript enforces it), then add it to
// `languages`, NAMES and LOADERS. Languages whose plurals need more than "one/other" list their
// forms in plural.ts. Only English is built in; the chosen language loads on its own, so the
// windows don't carry every dictionary.
export type Lang = "en" | "de" | "es" | "fr" | "it" | "pl" | "tr";
/** English and German first, then by name. */
export const languages: Lang[] = ["en", "de", "es", "fr", "it", "pl", "tr"];

const NAMES: Record<Lang, string> = {
  en: "English",
  de: "Deutsch",
  es: "Español",
  fr: "Français",
  it: "Italiano",
  pl: "Polski",
  tr: "Türkçe",
};

type Dictionary = Record<Key, string>;
const LOADERS: Record<Exclude<Lang, "en">, () => Promise<Dictionary>> = {
  de: () => import("./de").then((m) => m.de),
  es: () => import("./es").then((m) => m.es),
  fr: () => import("./fr").then((m) => m.fr),
  it: () => import("./it").then((m) => m.it),
  pl: () => import("./pl").then((m) => m.pl),
  tr: () => import("./tr").then((m) => m.tr),
};

const STORAGE_KEY = "relay.lang";

function isLang(value: string | null): value is Lang {
  return value != null && (languages as string[]).includes(value);
}

function initialLang(): Lang {
  // Storage can be missing or blocked; the system language (or English) is used then.
  let saved: string | null = null;
  try {
    saved = localStorage.getItem(STORAGE_KEY);
  } catch {
    saved = null;
  }
  if (isLang(saved)) return saved;
  const wanted = (globalThis.navigator?.language ?? "en").toLowerCase().slice(0, 2);
  return isLang(wanted) ? wanted : "en";
}

let current = $state<Lang>("en");
let dict = $state.raw<Dictionary>(en);
let loading = 0;

/** Switches once the language's texts are there; a later choice wins over an earlier one. */
async function use(lang: Lang): Promise<void> {
  const id = ++loading;
  const next = lang === "en" ? en : await LOADERS[lang]();
  if (id !== loading) return;
  dict = next;
  current = lang;
  document.documentElement.lang = lang;
}

/** Resolves when the saved language is loaded; windows wait for it before they first draw. */
export const i18nReady: Promise<void> = use(initialLang()).catch((e) => console.error("language failed to load", e));

export const i18n = {
  get lang(): Lang {
    return current;
  },
  set lang(value: Lang) {
    localStorage.setItem(STORAGE_KEY, value);
    use(value).catch((e) => console.error("language failed to load", e));
  },
};

export function t(key: Key, vars?: Record<string, string | number>): string {
  const own = dict[key];
  const lang = own ? current : "en";
  let text: string = own ?? en[key];
  if (text.includes("|") && typeof vars?.n === "number") text = pickPlural(lang, text.split("|"), vars.n);
  // Texts name the app as "Relay"; the dev build says "Relay Dev" everywhere, in every language.
  if (DEV_BUILD) text = text.replace(/\bRelay\b/g, "Relay Dev");
  if (vars) {
    for (const [name, value] of Object.entries(vars)) text = text.replaceAll(`{${name}}`, String(value));
  }
  return text;
}

export function langName(lang: Lang): string {
  return NAMES[lang];
}

export function hasKey(key: string): key is Key {
  return key in en;
}
