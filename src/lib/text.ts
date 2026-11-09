// Text folding shared with the backend's matcher (src-tauri/src/matcher.rs `fold`): lowercase,
// accents removed, Cyrillic written in Latin. Hidden artists are stored in this form, so both
// sides must fold the same way.

const CYRILLIC: Record<string, string> = {
  а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ё: "e", ж: "zh", з: "z", и: "i", й: "y", к: "k", л: "l", м: "m",
  н: "n", о: "o", п: "p", р: "r", с: "s", т: "t", у: "u", ф: "f", х: "kh", ц: "ts", ч: "ch", ш: "sh", щ: "shch",
  ъ: "", ы: "y", ь: "", э: "e", ю: "yu", я: "ya", і: "i", ї: "yi", є: "ye", ґ: "g", ў: "u",
};

const ACCENTS: [string, string][] = [
  ["àáâãäåāăą", "a"], ["æ", "ae"], ["çćĉċč", "c"], ["ďđ", "d"], ["èéêëēĕėęě", "e"], ["ĝğġģ", "g"], ["ĥħ", "h"],
  ["ìíîïĩīĭįı", "i"], ["ĵ", "j"], ["ķ", "k"], ["ĺļľŀł", "l"], ["ñńņňŉ", "n"], ["òóôõöøōŏő", "o"], ["œ", "oe"],
  ["ŕŗř", "r"], ["śŝşšș", "s"], ["ß", "ss"], ["ţťŧț", "t"], ["ùúûüũūŭůűų", "u"], ["ŵ", "w"], ["ýÿŷ", "y"], ["źżž", "z"], ["þ", "th"],
];

const MAP = new Map<string, string>(Object.entries(CYRILLIC));
for (const [set, plain] of ACCENTS) for (const ch of set) MAP.set(ch, plain);

export function fold(s: string): string {
  let out = "";
  for (const ch of s.toLowerCase()) out += MAP.get(ch) ?? ch;
  return out;
}

/** Case- and accent-insensitive "contains", for the search boxes in lists. */
export function matchesQuery(haystack: string, foldedQuery: string): boolean {
  return foldedQuery === "" || fold(haystack).includes(foldedQuery);
}
