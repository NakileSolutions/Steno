import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  DEFAULT_CALL_APPS, DEFAULT_CONSENT_MESSAGE, DEFAULT_CONSENT_MESSAGE_EN, LINUX_CALL_APPS, MACOS_CALL_APPS, callAppsFor, callAppId, callAppShortName, callLabel, callOffer,
  consentMessage, defaultCallApps, formatShortcut,
  isBrowserApp, isBuiltinCallApp, isListening, isValidBundle, meetingCallChip, normalizeLiveState,
  normalizeSettings, parseBundles, stopPhrase, withLocalizedConsent,
} from '../listening';
import { setLang } from '../i18n.svelte';
import type { CallApp, LiveState, Settings } from '../types';

const st = (state: LiveState['state'], active: boolean, title: string | null = 'Daily ORION', app: string | null = 'teams', app_name: string | null = 'Microsoft Teams') => ({
  state,
  call: { active, app, app_name, title },
});

describe('callOffer', () => {
  it('propose de démarrer quand un appel est actif et que rien n’écoute', () => {
    const o = callOffer(st('idle', true));
    expect(o?.title).toBe('Daily ORION');
    expect(o?.label).toBe('Appel Teams détecté : Daily ORION');
  });
  it('ne propose rien pendant l’écoute, le traitement, ou sans appel', () => {
    expect(callOffer(st('listening', true))).toBeNull();
    expect(callOffer(st('paused', true))).toBeNull();
    expect(callOffer(st('processing', true))).toBeNull();
    expect(callOffer(st('idle', false))).toBeNull();
  });
  it('donne un titre générique si l’app ne fournit pas de titre', () => {
    expect(callOffer(st('idle', true, null))?.title).toBe('Appel Teams');
    expect(callOffer(st('idle', true, '  '))?.title).toBe('Appel Teams');
    expect(callOffer(st('idle', true, null, 'slack', 'Slack'))?.label).toBe('Appel Slack détecté : Appel Slack');
  });
  it('tolère un état sans appel', () => {
    expect(callOffer({ state: 'idle' } as unknown as LiveState)).toBeNull();
  });
  it('signale le navigateur', () => {
    const o = callOffer(st('idle', true, 'Meet — Point hebdo', 'browser', 'Navigateur (Google Meet…)'));
    expect(o?.browser).toBe(true);
    expect(o?.label).toBe('Appel détecté dans le navigateur : Meet — Point hebdo');
  });
});

describe('callLabel', () => {
  const t = 'Huddle dans #equipe-flotte';
  it('garde exactement « Appel Teams détecté » pour Teams', () => {
    expect(callLabel({ app: 'teams', app_name: 'Microsoft Teams', title: 'Daily ORION' })).toBe('Appel Teams détecté : Daily ORION');
    expect(callLabel({ app: 'teams', app_name: null, title: 'X' }, [])).toBe('Appel Teams détecté : X');
  });
  it('utilise le nom de chaque app de la liste par défaut', () => {
    const expected: Record<string, string> = {
      slack: 'Appel Slack détecté', zoom: 'Appel Zoom détecté', webex: 'Appel Webex détecté',
      discord: 'Appel Discord détecté', facetime: 'Appel FaceTime détecté', browser: 'Appel détecté dans le navigateur',
    };
    for (const [app, head] of Object.entries(expected)) {
      expect(callLabel({ app, app_name: null, title: t })).toBe(`${head} : ${t}`);
    }
  });
  it('préfère le nom des réglages, puis celui du cœur, puis l’identifiant', () => {
    const apps: CallApp[] = [...defaultCallApps(), { id: 'google-chat', name: 'Google Chat', bundles: ['com.google.chat'], enabled: true, browser: false }];
    expect(callLabel({ app: 'google-chat', app_name: 'google-chat', title: 'Point' }, apps)).toBe('Appel Google Chat détecté : Point');
    expect(callLabel({ app: 'tuple', app_name: 'Tuple', title: 'Pair' }, apps)).toBe('Appel Tuple détecté : Pair');
    expect(callLabel({ app: 'tuple', app_name: null, title: 'Pair' }, apps)).toBe('Appel tuple détecté : Pair');
  });
  it('traite une app ajoutée comme navigateur si elle est marquée ainsi', () => {
    const apps: CallApp[] = [{ id: 'arc', name: 'Arc', bundles: ['company.thebrowser.Browser'], enabled: true, browser: true }];
    expect(isBrowserApp('arc', apps)).toBe(true);
    expect(callLabel({ app: 'arc', app_name: 'Arc', title: 'Meet' }, apps)).toBe('Appel détecté dans le navigateur : Meet');
  });
});

describe('callAppShortName et meetingCallChip', () => {
  it('« Teams » pour Microsoft Teams, sinon le nom de l’app', () => {
    expect(callAppShortName('teams')).toBe('Teams');
    expect(callAppShortName('ms', 'Microsoft Teams', [])).toBe('Teams');
    expect(callAppShortName('zoom')).toBe('Zoom');
  });
  it('puce de réunion', () => {
    expect(meetingCallChip(null)).toBeNull();
    expect(meetingCallChip('slack')).toBe('Slack');
    expect(meetingCallChip('teams')).toBe('Teams');
    expect(meetingCallChip('browser')).toBe('Navigateur');
    expect(meetingCallChip('supprimee', [])).toBe('supprimee');
  });
});

describe('apps de visio par défaut', () => {
  it('reprennent la liste du contrat, navigateur désactivé', () => {
    expect(DEFAULT_CALL_APPS.map((a) => a.id)).toEqual(['teams', 'slack', 'zoom', 'webex', 'discord', 'facetime', 'browser']);
    expect(DEFAULT_CALL_APPS.filter((a) => !a.enabled).map((a) => a.id)).toEqual(['browser']);
    expect(DEFAULT_CALL_APPS.find((a) => a.browser)?.id).toBe('browser');
    expect(isBuiltinCallApp('zoom')).toBe(true);
    expect(isBuiltinCallApp('google-chat')).toBe(false);
  });
  it('sont identiques à docs/contracts.md §4', () => {
    const md = fs.readFileSync(path.resolve(__dirname, '../../../../../docs/contracts.md'), 'utf8');
    const m = md.match(/"call_apps": (\[[\s\S]*?\n\s*\])/);
    expect(m).not.toBeNull();
    expect(JSON.parse(m![1])).toEqual(MACOS_CALL_APPS);
    expect(DEFAULT_CALL_APPS).toBe(MACOS_CALL_APPS);
  });
  it('sous Linux : mêmes valeurs que le cœur (linux_call_apps), sans FaceTime', () => {
    expect(callAppsFor('linux')).toBe(LINUX_CALL_APPS);
    expect(callAppsFor('other')).toBe(MACOS_CALL_APPS);
    expect(LINUX_CALL_APPS.map((a) => a.id)).toEqual(['teams', 'slack', 'zoom', 'webex', 'discord', 'browser']);
    expect(LINUX_CALL_APPS.filter((a) => !a.enabled).map((a) => a.id)).toEqual(['browser']);
    const rs = fs.readFileSync(path.resolve(__dirname, '../../../../../crates/steno-core/src/settings.rs'), 'utf8');
    const block = rs.slice(rs.indexOf('const LINUX_BUNDLES'), rs.indexOf('fn is_macos_bundle'));
    for (const a of LINUX_CALL_APPS) {
      const line = block.slice(block.indexOf(`"${a.id}"`));
      const body = line.slice(0, line.indexOf(']'));
      const bundles = [...body.matchAll(/"([^"]+)"/g)].map((x) => x[1]).slice(1);
      expect(bundles, a.id).toEqual(a.bundles);
    }
  });
  it('defaultCallApps renvoie une copie modifiable', () => {
    const a = defaultCallApps();
    a[0].bundles.push('x');
    expect(DEFAULT_CALL_APPS[0].bundles).not.toContain('x');
  });
});

describe('callAppId', () => {
  it('minuscules, sans accents ni espaces', () => {
    expect(callAppId('Google Chat', [])).toBe('google-chat');
    expect(callAppId('  Visio Équipe  ', [])).toBe('visio-equipe');
    expect(callAppId('Tuple!', [])).toBe('tuple');
  });
  it('unique parmi les identifiants existants', () => {
    expect(callAppId('Zoom', ['teams', 'zoom'])).toBe('zoom-2');
    expect(callAppId('Zoom', ['zoom', 'zoom-2'])).toBe('zoom-3');
  });
  it('valeur de repli si le nom ne contient rien d’utilisable', () => {
    expect(callAppId('—', [])).toBe('app');
    expect(callAppId('', ['app'])).toBe('app-2');
  });
});

describe('bundles', () => {
  it('découpe et dédoublonne la saisie', () => {
    expect(parseBundles(' com.a.b, com.c.d  com.a.b ')).toEqual(['com.a.b', 'com.c.d']);
    expect(parseBundles('')).toEqual([]);
  });
  it('reconnaît un identifiant de bundle', () => {
    expect(isValidBundle('com.example.app')).toBe(true);
    expect(isValidBundle('Cisco-Systems.Spark')).toBe(true);
    expect(isValidBundle('example')).toBe(false);
    expect(isValidBundle('com example.app')).toBe(false);
    expect(isValidBundle('.com.app')).toBe(false);
    // Linux : nom de programme ou identifiant Flatpak.
    expect(isValidBundle('example', 'linux')).toBe(true);
    expect(isValidBundle('teams-for-linux', 'linux')).toBe(true);
    expect(isValidBundle('us.zoom.Zoom', 'linux')).toBe(true);
    expect(isValidBundle('com example', 'linux')).toBe(false);
    expect(isValidBundle('/usr/bin/zoom', 'linux')).toBe(false);
  });
});

describe('compatibilité avec un cœur plus ancien', () => {
  it('convertit teams_call en call', () => {
    const old = { state: 'idle', teams_call: { active: true, title: 'Daily' } } as unknown as LiveState;
    const n = normalizeLiveState(old);
    expect(n.call).toEqual({ active: true, app: 'teams', app_name: 'Microsoft Teams', title: 'Daily' });
    expect('teams_call' in n).toBe(false);
    expect(normalizeLiveState({ state: 'idle' } as unknown as LiveState).call.active).toBe(false);
  });
  it('laisse un état à jour tel quel', () => {
    const s = { ...st('idle', true, 'X', 'slack', 'Slack'), mic_muted: false } as unknown as LiveState;
    expect(normalizeLiveState(s)).toBe(s);
  });
  it('complète mic_muted pour un cœur plus ancien', () => {
    const s = st('idle', true, 'X', 'slack', 'Slack') as unknown as LiveState;
    expect(normalizeLiveState(s).mic_muted).toBe(false);
  });
  it('convertit auto_detect_teams et complète call_apps', () => {
    const old = { audio: { auto_detect_teams: false, silence_ms: 1000 } } as unknown as Settings;
    const n = normalizeSettings(old);
    expect(n.audio.auto_detect_calls).toBe(false);
    expect(n.audio.call_apps.map((a) => a.id)).toContain('slack');
    expect('auto_detect_teams' in n.audio).toBe(false);
  });
});

describe('isListening', () => {
  it('écoute en cours ou en pause', () => {
    expect(isListening({ state: 'listening' })).toBe(true);
    expect(isListening({ state: 'paused' })).toBe(true);
    expect(isListening({ state: 'idle' })).toBe(false);
    expect(isListening({ state: 'processing' })).toBe(false);
  });
});

describe('formatShortcut', () => {
  it('convertit en notation macOS, dans l’ordre d’Apple', () => {
    expect(formatShortcut('Ctrl+Alt+S')).toBe('⌃⌥S');
    expect(formatShortcut('Alt+Ctrl+s')).toBe('⌃⌥S');
    expect(formatShortcut('Cmd+Shift+N')).toBe('⇧⌘N');
    expect(formatShortcut('CmdOrCtrl+Alt+Space')).toBe('⌥⌘ Espace');
    expect(formatShortcut('F5')).toBe('F5');
    expect(formatShortcut('Ctrl+Alt+S', 'macos')).toBe('⌃⌥S');
  });
  it('sous Linux : noms des touches, Ctrl puis Alt puis Maj puis Super', () => {
    expect(formatShortcut('Ctrl+Alt+S', 'linux')).toBe('Ctrl+Alt+S');
    expect(formatShortcut('alt+ctrl+s', 'linux')).toBe('Ctrl+Alt+S');
    expect(formatShortcut('Ctrl+Alt+Space', 'linux')).toBe('Ctrl+Alt+Espace');
    expect(formatShortcut('Cmd+Shift+N', 'linux')).toBe('Maj+Super+N');
    expect(formatShortcut('CmdOrCtrl+Enter', 'linux')).toBe('Ctrl+Entrée');
    expect(formatShortcut('F5', 'linux')).toBe('F5');
    expect(formatShortcut('', 'linux')).toBe('');
  });
  it('vide si rien n’est configuré', () => {
    expect(formatShortcut('')).toBe('');
    expect(formatShortcut(null)).toBe('');
  });
});

describe('stopPhrase', () => {
  it('compose le mot d’appel et le premier verbe d’arrêt', () => {
    expect(stopPhrase('Sténo', { stop: ["arrête l'écoute", 'termine la réunion'] })).toBe('Sténo, arrête l’écoute');
    expect(stopPhrase('Jarvis', { stop: ['  ', 'stop'] })).toBe('Jarvis, stop');
  });
  it('a des valeurs par défaut', () => {
    expect(stopPhrase('', null)).toBe('Sténo, arrête l’écoute');
    expect(stopPhrase('Sténo', { stop: [] })).toBe('Sténo, arrête l’écoute');
  });
});

describe('English interface defaults', () => {
  afterEach(() => setLang('fr'));
  const builtinStop = ["arrête l'écoute", "arrête d'écouter", 'arrête la transcription', 'termine la réunion', 'fin de la réunion', 'stop listening', 'end the meeting'];

  it('stopPhrase picks a built-in verb of the interface language', () => {
    setLang('en');
    expect(stopPhrase('Sténo', { stop: builtinStop })).toBe('Sténo, stop listening');
    expect(stopPhrase('', null)).toBe('Sténo, stop listening');
    expect(stopPhrase('Sténo', { stop: ["arrête l'écoute", 'halt'] })).toBe('Sténo, arrête l’écoute');
    setLang('fr');
    expect(stopPhrase('Sténo', { stop: ['stop listening', "arrête l'écoute"] })).toBe('Sténo, arrête l’écoute');
    expect(stopPhrase('Sténo', { stop: ['stop listening'] })).toBe('Sténo, stop listening');
  });

  it('a never-edited consent message follows the interface language', () => {
    setLang('en');
    expect(consentMessage(DEFAULT_CONSENT_MESSAGE)).toBe(DEFAULT_CONSENT_MESSAGE_EN);
    expect(consentMessage('')).toBe(DEFAULT_CONSENT_MESSAGE_EN);
    expect(consentMessage(null)).toBe(DEFAULT_CONSENT_MESSAGE_EN);
    expect(consentMessage('Je transcris cette réunion.')).toBe('Je transcris cette réunion.');
    setLang('fr');
    expect(consentMessage(DEFAULT_CONSENT_MESSAGE_EN)).toBe(DEFAULT_CONSENT_MESSAGE);
    expect(consentMessage(` ${DEFAULT_CONSENT_MESSAGE}\n`)).toBe(DEFAULT_CONSENT_MESSAGE);
  });

  it('withLocalizedConsent only replaces a default message', () => {
    setLang('en');
    const s = { privacy: { consent_message: DEFAULT_CONSENT_MESSAGE } } as unknown as Settings;
    expect(withLocalizedConsent(s).privacy.consent_message).toBe(DEFAULT_CONSENT_MESSAGE_EN);
    expect(s.privacy.consent_message).toBe(DEFAULT_CONSENT_MESSAGE);
    const edited = { privacy: { consent_message: 'Custom' } } as unknown as Settings;
    expect(withLocalizedConsent(edited)).toBe(edited);
  });

  it('the French default matches the core', () => {
    const rs = fs.readFileSync(path.resolve(__dirname, '../../../../../crates/steno-core/src/settings.rs'), 'utf8');
    expect(rs).toContain(`pub const DEFAULT_CONSENT_MESSAGE: &str = "${DEFAULT_CONSENT_MESSAGE}";`);
  });
});
