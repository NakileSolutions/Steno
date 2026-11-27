import { describe, expect, it } from 'vitest';
import { formatShortcut, isListening, stopPhrase, teamsOffer } from '../listening';
import type { LiveState } from '../types';

const st = (state: LiveState['state'], active: boolean, title: string | null = 'Daily ORION') => ({
  state,
  teams_call: { active, title },
});

describe('teamsOffer', () => {
  it('propose de démarrer quand un appel Teams est actif et que rien n’écoute', () => {
    expect(teamsOffer(st('idle', true))).toBe('Daily ORION');
  });
  it('ne propose rien pendant l’écoute, le traitement, ou sans appel', () => {
    expect(teamsOffer(st('listening', true))).toBeNull();
    expect(teamsOffer(st('paused', true))).toBeNull();
    expect(teamsOffer(st('processing', true))).toBeNull();
    expect(teamsOffer(st('idle', false))).toBeNull();
  });
  it('donne un titre générique si Teams ne fournit pas de titre', () => {
    expect(teamsOffer(st('idle', true, null))).toBe('Appel Teams');
    expect(teamsOffer(st('idle', true, '  '))).toBe('Appel Teams');
  });
  it('tolère un état sans teams_call (ancien backend)', () => {
    expect(teamsOffer({ state: 'idle' } as unknown as LiveState)).toBeNull();
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
  });
  it('vide si rien n’est configuré', () => {
    expect(formatShortcut('')).toBe('');
    expect(formatShortcut(null)).toBe('');
  });
});

describe('stopPhrase', () => {
  it('compose le mot d’appel et le premier verbe d’arrêt', () => {
    expect(stopPhrase('Scribe', { stop: ["arrête l'écoute", 'termine la réunion'] })).toBe('Scribe, arrête l’écoute');
    expect(stopPhrase('Jarvis', { stop: ['  ', 'stop'] })).toBe('Jarvis, stop');
  });
  it('a des valeurs par défaut', () => {
    expect(stopPhrase('', null)).toBe('Scribe, arrête l’écoute');
    expect(stopPhrase('Scribe', { stop: [] })).toBe('Scribe, arrête l’écoute');
  });
});
