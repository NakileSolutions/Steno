// Détection du mot d'appel côté interface (faux backend et aperçus) : même règle que le cœur,
// en plus simple. Précédé d'un préfixe (« OK Sténo »), le mot d'appel est reconnu n'importe où
// dans la phrase ; seul (hors mode strict), dans les 2 premiers mots. Le verbe qui suit
// détermine l'intention (recherche par défaut).

import type { Settings, WakeDetect } from './types';
import { t, tk } from './i18n.svelte';

export const fold = (s: string) => s.normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/’/g, "'").toLowerCase();

const words = (s: string) => s.replace(/[^\p{L}\p{N}\s'’-]/gu, ' ').split(/\s+/).filter(Boolean);

export function detectWake(text: string, voice: Settings['voice']): WakeDetect {
  const folded = words(fold(text));
  const wake = new Set([fold(voice.wake_word), ...voice.variants.map(fold)].filter(Boolean));
  const prefixes = voice.prefixes.map(fold);
  let idx = -1;
  for (let i = 0; i < folded.length; i++) {
    if (!wake.has(folded[i])) continue;
    const prefixed = i > 0 && prefixes.includes(folded[i - 1]);
    if (prefixed || (!voice.strict && i < 2)) idx = i;
  }
  if (idx < 0) return { detected: false, intent: null, query: null };
  const orig = words(text);
  const restText = folded.slice(idx + 1).join(' ');
  // Les verbes les plus longs d'abord : « ajoute une action » avant « ajoute ».
  const verbs = Object.entries(voice.verbs)
    .flatMap(([intent, list]) => (list as string[]).map((v) => ({ intent, v: words(fold(v)).join(' ') })))
    .filter((x) => x.v)
    .sort((a, b) => b.v.length - a.v.length);
  for (const { intent, v } of verbs) {
    if (restText === v || restText.startsWith(v + ' ')) {
      const n = v.split(' ').length;
      return { detected: true, intent, query: orig.slice(idx + 1 + n).join(' ') || null };
    }
  }
  return { detected: true, intent: 'search', query: orig.slice(idx + 1).join(' ') || null };
}

// ---------- Wake word test (PLAN §6.4) ----------

/**
 * Text the wake word test asks the user to read aloud: words that sound like the default wake
 * word (« Sténo »), none of which should trigger. Shown in the interface language with `t()`.
 */
// One string literal: the translation coverage test only reads the first literal of `tk(…)`.
export const WAKE_TEST_TRAP = tk(
  'Le sténographe a noté le scénario du créneau de jeudi. Stéphane a prêté son stylo au ténor de la chorale. Il faudra rester sur le même tempo et étendre le délai. Tout est noté sur le tableau.',
);

/** Command the wake word test asks the user to say, in the interface language. */
export function wakeTestPhrase(voice: Pick<Settings['voice'], 'wake_word' | 'strict' | 'prefixes'>): string {
  const prefix = voice.strict ? (voice.prefixes[0] ?? 'ok') + ' ' : '';
  return `${prefix}${voice.wake_word}, ${t('cherche quelque chose')}`;
}
