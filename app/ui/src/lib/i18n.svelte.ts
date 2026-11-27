// Traduction de l'interface. Le français est la langue source : chaque texte s'écrit
// `t('Texte en français')`, et sa traduction anglaise se trouve dans `i18n/en/*.ts` (clé = le
// texte français exact). Un texte sans traduction reste en français ; le test
// `i18n.test.ts` vérifie que chaque `t('…')` du code a sa traduction.
//
// Variables : `t('Sténo {v} est disponible', { v: '0.2.0' })` (mêmes `{noms}` dans les deux langues).
// Pluriel : `tn(n, '{n} demande', '{n} demandes')`.

import en from './i18n/en/index';

export type Lang = 'fr' | 'en';
export type LangPref = 'auto' | Lang;

export const i18n = $state<{ lang: Lang }>({ lang: 'fr' });

/** Langue du système (navigateur / WebView) : anglais sauf si elle commence par « fr ». */
export function systemLang(): Lang {
  const l = (typeof navigator !== 'undefined' && (navigator.languages?.[0] || navigator.language)) || 'fr';
  return l.toLowerCase().startsWith('fr') ? 'fr' : 'en';
}

export function resolveLang(pref: LangPref | null | undefined): Lang {
  return pref === 'fr' || pref === 'en' ? pref : systemLang();
}

export function setLang(pref: LangPref | null | undefined): void {
  i18n.lang = resolveLang(pref);
  if (typeof document !== 'undefined') document.documentElement.lang = i18n.lang;
}

function fill(s: string, vars?: Record<string, string | number>): string {
  return vars ? s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : s;
}

/** Texte traduit (français = clé et repli). */
export function t(fr: string, vars?: Record<string, string | number>): string {
  const s = i18n.lang === 'en' ? ((en as Record<string, string>)[fr] ?? fr) : fr;
  return fill(s, vars);
}

/** Singulier / pluriel (variable `{n}` fournie automatiquement). */
export function tn(n: number, one: string, many: string, vars?: Record<string, string | number>): string {
  return t(n > 1 || (i18n.lang === 'en' && n === 0) ? many : one, { n, ...vars });
}

/** Locale pour `Intl` (dates, heures, nombres). */
export function locale(): string {
  return i18n.lang === 'en' ? 'en-GB' : 'fr-FR';
}
