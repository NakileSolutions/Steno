// Réglages utiles à l'affichage hors de l'écran Réglages (mot d'appel, raccourci, verbe d'arrêt,
// noms des apps de visio).

import { api } from './api';
import { DEFAULT_CALL_APPS, DEFAULT_STOP_VERB, formatShortcut, stopPhrase } from './listening';
import type { CallApp, Settings } from './types';

export const prefs = $state({
  wake: 'Scribe',
  stopVerbs: [DEFAULT_STOP_VERB] as string[],
  toggle: 'Ctrl+Alt+S',
  callApps: DEFAULT_CALL_APPS.map((a) => ({ ...a })) as CallApp[],
});

export function setPrefs(s: Settings): void {
  prefs.wake = s.voice.wake_word || 'Scribe';
  prefs.stopVerbs = s.voice.verbs.stop?.length ? [...s.voice.verbs.stop] : [DEFAULT_STOP_VERB];
  prefs.toggle = s.audio.shortcuts.toggle || 'Ctrl+Alt+S';
  if (Array.isArray(s.audio.call_apps)) prefs.callApps = s.audio.call_apps.map((a) => ({ ...a }));
}

let started = false;
export function loadPrefs(): void {
  if (started) return;
  started = true;
  api.getSettings().then(setPrefs).catch(() => {});
}

/** « Pour arrêter : bouton, ⌃⌥S, ou dites « Scribe, arrête l’écoute » ». */
export function stopHint(): string {
  const sc = formatShortcut(prefs.toggle);
  const phrase = stopPhrase(prefs.wake, { stop: prefs.stopVerbs });
  return `Pour arrêter : bouton${sc ? `, ${sc}` : ''}, ou dites « ${phrase} »`;
}
