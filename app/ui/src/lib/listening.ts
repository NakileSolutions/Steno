// Logique pure autour de l'écoute : jamais de démarrage ni d'arrêt automatique,
// seulement des propositions (appel Teams détecté) et des rappels (comment arrêter).

import type { LiveState, VoiceVerbs } from './types';

export const DEFAULT_STOP_VERB = 'arrête l’écoute';

/** L'écoute est en cours (ou en pause) : il y a quelque chose à arrêter. */
export function isListening(s: Pick<LiveState, 'state'>): boolean {
  return s.state === 'listening' || s.state === 'paused';
}

/**
 * Titre à proposer quand un appel Teams est détecté et que rien n'écoute ;
 * `null` s'il n'y a rien à proposer. Un appel sans titre lisible renvoie « Appel Teams ».
 */
export function teamsOffer(s: Pick<LiveState, 'state' | 'teams_call'>): string | null {
  if (s.state !== 'idle' || !s.teams_call?.active) return null;
  return s.teams_call.title?.trim() || 'Appel Teams';
}

const KEY_GLYPH: Record<string, string> = {
  ctrl: '⌃',
  control: '⌃',
  alt: '⌥',
  option: '⌥',
  opt: '⌥',
  shift: '⇧',
  cmd: '⌘',
  command: '⌘',
  meta: '⌘',
  super: '⌘',
  cmdorctrl: '⌘',
  commandorcontrol: '⌘',
  space: 'Espace',
  enter: '↩',
  return: '↩',
  escape: '⎋',
  esc: '⎋',
};
const ORDER = ['⌃', '⌥', '⇧', '⌘'];

/** « Ctrl+Alt+S » → « ⌃⌥S » (notation macOS, modificateurs dans l'ordre d'Apple). */
export function formatShortcut(accel: string | null | undefined): string {
  if (!accel?.trim()) return '';
  const parts = accel.split('+').map((p) => p.trim()).filter(Boolean);
  const mods: string[] = [];
  let key = '';
  for (const p of parts) {
    const g = KEY_GLYPH[p.toLowerCase()];
    if (g && ORDER.includes(g)) {
      if (!mods.includes(g)) mods.push(g);
    } else key = g ?? (p.length === 1 ? p.toUpperCase() : p);
  }
  mods.sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  return mods.join('') + (key === 'Espace' && mods.length ? ' Espace' : key);
}

/** Phrase d'arrêt à la voix : « Scribe, arrête l’écoute ». */
export function stopPhrase(wakeWord: string | null | undefined, verbs?: Partial<VoiceVerbs> | null): string {
  const w = wakeWord?.trim() || 'Scribe';
  const v = verbs?.stop?.find((x) => x.trim())?.trim() || DEFAULT_STOP_VERB;
  return `${w}, ${v.replace(/'/g, '’')}`;
}
