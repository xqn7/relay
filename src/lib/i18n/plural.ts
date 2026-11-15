// Picks the right variant of "a|b" texts for a count, using the language's own plural rules.

/** Plural forms a language writes, in order (Intl.PluralRules categories). Default: one|other. */
const FORMS: Record<string, Intl.LDMLPluralRule[]> = { pl: ["one", "few", "many"] };

const rules = new Map<string, Intl.PluralRules>();

/** One form more than the language's plural forms means the first one is for zero
 * ("Never played|Played once|Played {n} times"). */
export function pickPlural(lang: string, forms: string[], n: number): string {
  const order = FORMS[lang] ?? ["one", "other"];
  if (forms.length === order.length + 1) {
    if (n === 0) return forms[0];
    forms = forms.slice(1);
  }
  if (!rules.has(lang)) rules.set(lang, new Intl.PluralRules(lang));
  const index = order.indexOf(rules.get(lang)!.select(n));
  return forms[index >= 0 && index < forms.length ? index : forms.length - 1];
}
