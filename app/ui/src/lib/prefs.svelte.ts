// Réglages utiles à l'affichage hors de l'écran Réglages (mot d'appel, raccourci, verbe d'arrêt,
// noms des apps de visio).

import { api, onEvent } from './api';
import { DEFAULT_CALL_APPS, DEFAULT_CONSENT_MESSAGE, DEFAULT_STOP_VERB, formatShortcut, normalizeSettings, stopPhrase } from './listening';
import type { CallApp, Settings } from './types';
import { setLang, t } from './i18n.svelte';

export const prefs = $state({
  wake: 'Sténo',
  stopVerbs: [DEFAULT_STOP_VERB] as string[],
  toggle: 'Ctrl+Alt+S',
  callApps: DEFAULT_CALL_APPS.map((a) => ({ ...a })) as CallApp[],
  /** Micro choisi (`null` = micro par défaut du système) et son nom au moment du choix. */
  inputDevice: null as string | null,
  inputDeviceName: null as string | null,
  /** Rappel d'information des participants au début de l'écoute, et le message à copier. */
  consentReminder: true,
  consentMessage: DEFAULT_CONSENT_MESSAGE,
});

export function setPrefs(s: Settings): void {
  setLang(s.profile?.language ?? 'auto');
  prefs.wake = s.voice.wake_word || 'Sténo';
  prefs.stopVerbs = s.voice.verbs.stop?.length ? [...s.voice.verbs.stop] : [DEFAULT_STOP_VERB];
  prefs.toggle = s.audio.shortcuts.toggle || 'Ctrl+Alt+S';
  prefs.inputDevice = s.audio.input_device ?? null;
  prefs.inputDeviceName = s.audio.input_device_name ?? null;
  prefs.consentReminder = s.privacy?.consent_reminder ?? true;
  prefs.consentMessage = s.privacy?.consent_message || DEFAULT_CONSENT_MESSAGE;
  if (Array.isArray(s.audio.call_apps)) prefs.callApps = s.audio.call_apps.map((a) => ({ ...a }));
}

let started = false;
export function loadPrefs(): void {
  if (started) return;
  started = true;
  api.getSettings().then(setPrefs).catch(() => {});
  // Settings saved in another window (main window ↔ menu bar panel, which stays loaded in
  // the background): language, wake word and shortcuts follow.
  onEvent('steno://settings', (s) => setPrefs(normalizeSettings(s)));
}

/** « Pour arrêter : bouton, ⌃⌥S, ou dites « Sténo, arrête l’écoute » ». */
export function stopHint(): string {
  const sc = formatShortcut(prefs.toggle);
  const phrase = stopPhrase(prefs.wake, { stop: prefs.stopVerbs });
  return sc
    ? t('Pour arrêter : bouton, {shortcut}, ou dites « {phrase} »', { shortcut: sc, phrase })
    : t('Pour arrêter : bouton, ou dites « {phrase} »', { phrase });
}
