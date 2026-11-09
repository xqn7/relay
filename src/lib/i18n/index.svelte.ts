import { en, type Key } from "./en";
import { de } from "./de";

// Adding a language: create xx.ts with every key, then add it here.
const dictionaries = { en, de } as const;
export type Lang = keyof typeof dictionaries;
export const languages: Lang[] = ["en", "de"];

const STORAGE_KEY = "relay.lang";

function initialLang(): Lang {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved && saved in dictionaries) return saved as Lang;
  return navigator.language.toLowerCase().startsWith("de") ? "de" : "en";
}

let current = $state<Lang>(initialLang());

export const i18n = {
  get lang(): Lang {
    return current;
  },
  set lang(value: Lang) {
    current = value;
    document.documentElement.lang = value;
    localStorage.setItem(STORAGE_KEY, value);
  },
};

export function t(key: Key, vars?: Record<string, string | number>): string {
  let text: string = dictionaries[current][key] ?? en[key];
  // "one|other" (or "zero|one|other") variants are picked by the {n} count.
  if (text.includes("|") && typeof vars?.n === "number") {
    const forms = text.split("|");
    const n = vars.n;
    text = forms.length === 3 ? forms[n === 0 ? 0 : n === 1 ? 1 : 2] : forms[n === 1 ? 0 : 1];
  }
  if (vars) {
    for (const [name, value] of Object.entries(vars)) text = text.replaceAll(`{${name}}`, String(value));
  }
  return text;
}

export function langName(lang: Lang): string {
  return dictionaries[lang]["lang.name"];
}

export function hasKey(key: string): key is Key {
  return key in en;
}
