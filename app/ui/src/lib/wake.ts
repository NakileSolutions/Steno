// Détection du mot d'appel côté interface (faux backend et aperçus) : même règle que le cœur,
// en plus simple. Le mot d'appel doit être dans les 2 premiers mots ; en mode strict, précédé
// d'un préfixe. Le verbe qui suit détermine l'intention (recherche par défaut).

import type { Settings, WakeDetect } from './types';

export const fold = (s: string) => s.normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/’/g, "'").toLowerCase();

const words = (s: string) => s.replace(/[^\p{L}\p{N}\s'’-]/gu, ' ').split(/\s+/).filter(Boolean);

export function detectWake(text: string, voice: Settings['voice']): WakeDetect {
  const folded = words(fold(text));
  const wake = new Set([fold(voice.wake_word), ...voice.variants.map(fold)].filter(Boolean));
  const prefixes = voice.prefixes.map(fold);
  let idx = -1;
  for (let i = 0; i < Math.min(2, folded.length); i++) {
    if (wake.has(folded[i])) {
      if (voice.strict && !(i === 1 && prefixes.includes(folded[0]))) continue;
      idx = i;
      break;
    }
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
