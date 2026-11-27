// Logique pure autour de l'écoute : jamais de démarrage ni d'arrêt automatique,
// seulement des propositions (appel détecté dans une app de visio) et des rappels (comment arrêter).

import type { CallApp, LiveCall, LiveState, Settings, VoiceVerbs } from './types';
import { normalizeAgentSettings } from './agents';
import { t, tk } from './i18n.svelte';

export const DEFAULT_STOP_VERB = 'arrête l’écoute';

/** L'écoute est en cours (ou en pause) : il y a quelque chose à arrêter. */
export function isListening(s: Pick<LiveState, 'state'>): boolean {
  return s.state === 'listening' || s.state === 'paused';
}

// ---------- Apps de visio (contracts.md §4 `audio.call_apps`) ----------

/** Liste par défaut des apps de visio surveillées (copie de contracts.md §4). */
export const DEFAULT_CALL_APPS: readonly CallApp[] = [
  { id: 'teams', name: 'Microsoft Teams', bundles: ['com.microsoft.teams2', 'com.microsoft.teams'], enabled: true, browser: false },
  { id: 'slack', name: 'Slack', bundles: ['com.tinyspeck.slackmacgap'], enabled: true, browser: false },
  { id: 'zoom', name: 'Zoom', bundles: ['us.zoom.xos', 'us.zoom.CptHost'], enabled: true, browser: false },
  { id: 'webex', name: 'Webex', bundles: ['Cisco-Systems.Spark', 'com.cisco.webexmeetingsapp', 'com.webex.meetingmanager'], enabled: true, browser: false },
  { id: 'discord', name: 'Discord', bundles: ['com.hnc.Discord'], enabled: true, browser: false },
  { id: 'facetime', name: 'FaceTime', bundles: ['com.apple.FaceTime'], enabled: true, browser: false },
  {
    id: 'browser', name: tk('Navigateur (Google Meet…)'),
    bundles: ['com.google.Chrome', 'com.apple.Safari', 'company.thebrowser.Browser', 'com.microsoft.edgemac', 'org.mozilla.firefox', 'com.brave.Browser'],
    enabled: false, browser: true,
  },
];

export const defaultCallApps = (): CallApp[] => DEFAULT_CALL_APPS.map((a) => ({ ...a, bundles: [...a.bundles] }));

/** App de la liste par défaut (non supprimable, seulement désactivable). */
export const isBuiltinCallApp = (id: string): boolean => DEFAULT_CALL_APPS.some((a) => a.id === id);

type Apps = readonly CallApp[] | null | undefined;
const findApp = (id: string | null | undefined, apps: Apps) => (id ? (apps ?? DEFAULT_CALL_APPS).find((a) => a.id === id) : undefined);

/** L'app capte tout le son d'un navigateur (Google Meet…). */
export function isBrowserApp(id: string | null | undefined, apps?: Apps): boolean {
  const a = findApp(id, apps) ?? findApp(id, DEFAULT_CALL_APPS);
  return a ? a.browser : id === 'browser';
}

/**
 * Nom court d'une app pour les libellés : « Teams » pour Microsoft Teams, sinon le nom
 * de l'app (réglages, puis nom transmis par le cœur, puis identifiant).
 */
export function callAppShortName(id: string | null | undefined, appName?: string | null, apps?: Apps): string {
  const name = findApp(id, apps)?.name?.trim() || appName?.trim() || findApp(id, DEFAULT_CALL_APPS)?.name || id?.trim() || '';
  if (id === 'teams' || /^microsoft teams$/i.test(name)) return 'Teams';
  return name || t('visio');
}

export type CallOffer = {
  app: string | null;
  /** Titre de l'appel (ou titre générique s'il est illisible). */
  title: string;
  /** « Appel Slack détecté », « Appel détecté dans le navigateur ». */
  heading: string;
  /** Libellé complet : « Appel Slack détecté : Huddle… ». */
  label: string;
  browser: boolean;
};

/** Libellé d'un appel détecté, sans le titre : « Appel Teams détecté ». */
export function callHeading(call: Pick<LiveCall, 'app' | 'app_name'>, apps?: Apps): string {
  if (isBrowserApp(call.app, apps)) return t('Appel détecté dans le navigateur');
  return t('Appel {app} détecté', { app: callAppShortName(call.app, call.app_name, apps) });
}

/** Libellé complet : « Appel Teams détecté : Daily ORION ». */
export function callLabel(call: Pick<LiveCall, 'app' | 'app_name' | 'title'>, apps?: Apps): string {
  return t('{heading} : {title}', { heading: callHeading(call, apps), title: callTitle(call, apps) });
}

function callTitle(call: Pick<LiveCall, 'app' | 'app_name' | 'title'>, apps?: Apps): string {
  const title = call.title?.trim();
  if (title) return title;
  return isBrowserApp(call.app, apps) ? t('Appel sans titre') : t('Appel {app}', { app: callAppShortName(call.app, call.app_name, apps) });
}

/**
 * Proposition à afficher quand un appel est détecté et que rien n'écoute ;
 * `null` s'il n'y a rien à proposer. Un appel sans titre lisible reçoit un titre générique
 * (« Appel Teams »).
 */
export function callOffer(s: Pick<LiveState, 'state' | 'call'>, apps?: Apps): CallOffer | null {
  if (s.state !== 'idle' || !s.call?.active) return null;
  const heading = callHeading(s.call, apps);
  const title = callTitle(s.call, apps);
  return { app: s.call.app, title, heading, label: t('{heading} : {title}', { heading, title }), browser: isBrowserApp(s.call.app, apps) };
}

/** Puce d'une réunion : nom court de l'app de l'appel, `null` sans appel. */
/** `call_app` d'une réunion en présentiel (micro du Mac seul). */
export const ROOM = 'room';

export function meetingCallChip(callApp: string | null | undefined, apps?: Apps): string | null {
  if (!callApp) return null;
  if (callApp === ROOM) return t('En présentiel');
  if (isBrowserApp(callApp, apps)) return t('Navigateur');
  return callAppShortName(callApp, null, apps);
}

/**
 * Identifiant d'une app ajoutée : nom en minuscules, sans accents ni espaces
 * (« Google Chat » → « google-chat »), rendu unique parmi `taken`.
 */
export function callAppId(name: string, taken: Iterable<string>): string {
  const base =
    name
      .normalize('NFD')
      .replace(/[\u0300-\u036f]/g, '')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '') || 'app';
  const used = new Set(taken);
  if (!used.has(base)) return base;
  let n = 2;
  while (used.has(`${base}-${n}`)) n++;
  return `${base}-${n}`;
}

/** Préfixes de bundle saisis (séparés par des virgules ou des espaces), dédoublonnés. */
export function parseBundles(text: string): string[] {
  return [...new Set(text.split(/[\s,;]+/).map((b) => b.trim()).filter(Boolean))];
}

/** Identifiant de bundle plausible : « com.example.app ». */
export function isValidBundle(b: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9-]*(\.[A-Za-z0-9][A-Za-z0-9-]*)+$/.test(b);
}

/** État en direct d'un cœur plus ancien (`teams_call`) ramené au contrat actuel (`call`). */
export function normalizeLiveState(input: LiveState): LiveState {
  if (input.call && typeof input.mic_muted === 'boolean') return input;
  // Cœur plus ancien : pas de `mic_muted`.
  const s: LiveState = { ...input, mic_muted: input.mic_muted ?? false };
  if (s.call) return s;
  const legacy = (s as unknown as { teams_call?: { active: boolean; title: string | null } }).teams_call;
  const { teams_call: _old, ...rest } = s as LiveState & { teams_call?: unknown };
  void _old;
  return {
    ...rest,
    call: legacy?.active
      ? { active: true, app: 'teams', app_name: 'Microsoft Teams', title: legacy.title ?? null }
      : { active: false, app: null, app_name: null, title: null },
  };
}

/** Réglages d'un cœur plus ancien (`auto_detect_teams`, sans `call_apps`) ramenés au contrat actuel. */
/** Message d'information par défaut (identique au cœur : `DEFAULT_CONSENT_MESSAGE`). */
export const DEFAULT_CONSENT_MESSAGE =
  "Pour information : j\'utilise Sténo pour transcrire cette réunion et en préparer un compte rendu. La transcription reste sur mon ordinateur ; un assistant IA s\'en sert pour rédiger le compte rendu. Dites-le-moi si vous préférez que je ne transcrive pas.";

export const DEFAULT_REPORT_SETTINGS = { default_template: 'standard', ask_context_on_stop: false, context_delay_s: 60, templates: [] };

export function normalizeSettings(input: Settings): Settings {
  let s = normalizeAgentSettings(input);
  // Cœur plus ancien : pas de formats de compte rendu.
  if (!s.report) s = { ...s, report: { ...DEFAULT_REPORT_SETTINGS, templates: [] } };
  // Cœur plus ancien : pas de message d'information aux participants.
  if (s.privacy && (s.privacy.consent_reminder === undefined || s.privacy.consent_message === undefined))
    s = {
      ...s,
      privacy: {
        ...s.privacy,
        consent_reminder: s.privacy.consent_reminder ?? true,
        consent_message: s.privacy.consent_message || DEFAULT_CONSENT_MESSAGE,
      },
    };
  // Périphériques (cœur plus ancien) : `null` = défaut du système.
  if (s.audio && (s.audio.input_device === undefined || s.audio.input_device_name === undefined))
    s = { ...s, audio: { ...s.audio, input_device: s.audio.input_device ?? null, input_device_name: s.audio.input_device_name ?? null } };
  if (s.voice && s.voice.feedback_device === undefined) s = { ...s, voice: { ...s.voice, feedback_device: null } };
  const audio = s.audio as Settings['audio'] & { auto_detect_teams?: boolean };
  if (audio.auto_detect_calls !== undefined && Array.isArray(audio.call_apps) && audio.auto_detect_teams === undefined) return s;
  const { auto_detect_teams, ...rest } = audio;
  return {
    ...s,
    audio: {
      ...rest,
      auto_detect_calls: audio.auto_detect_calls ?? auto_detect_teams ?? true,
      call_apps: Array.isArray(audio.call_apps) ? audio.call_apps : defaultCallApps(),
    },
  };
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
  const space = key === 'Espace';
  const k = space ? t('Espace') : key;
  return mods.join('') + (space && mods.length ? ` ${k}` : k);
}

/** Phrase d'arrêt à la voix : « Sténo, arrête l’écoute ». */
export function stopPhrase(wakeWord: string | null | undefined, verbs?: Partial<VoiceVerbs> | null): string {
  const w = wakeWord?.trim() || 'Sténo';
  const v = verbs?.stop?.find((x) => x.trim())?.trim() || DEFAULT_STOP_VERB;
  return `${w}, ${v.replace(/'/g, '’')}`;
}
