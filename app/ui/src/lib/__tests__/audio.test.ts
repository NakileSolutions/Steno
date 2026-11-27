import { describe, expect, it } from 'vitest';
import { defaultInputLabel, defaultOutputLabel, isDeviceMissing, missingInputLabel, transportIcon } from '../audio';
import { normalizeSettings } from '../listening';
import type { AudioDevice, Settings } from '../types';

const inputs: AudioDevice[] = [
  { id: 'BuiltInMicrophoneDevice', name: 'Micro du MacBook Pro', default: true, transport: 'builtin' },
  { id: 'AirPodsPro-UID:input', name: 'AirPods Pro', default: false, transport: 'bluetooth' },
  { id: 'ScarlettSolo-UID', name: 'Scarlett Solo USB', default: false, transport: 'usb' },
];

describe('micro par défaut', () => {
  it('nomme le micro par défaut du système', () => {
    expect(defaultInputLabel(inputs)).toBe('Micro par défaut du système (Micro du MacBook Pro)');
  });
  it('sans liste ni défaut connu : libellé seul', () => {
    expect(defaultInputLabel([])).toBe('Micro par défaut du système');
    expect(defaultInputLabel(null)).toBe('Micro par défaut du système');
    expect(defaultInputLabel(inputs.map((d) => ({ ...d, default: false })))).toBe('Micro par défaut du système');
  });
  it('sortie par défaut', () => {
    expect(defaultOutputLabel([{ id: 'a', name: 'AirPods Pro', default: true, transport: 'bluetooth' }])).toBe('Sortie par défaut (AirPods Pro)');
    expect(defaultOutputLabel([])).toBe('Sortie par défaut');
  });
});

describe('micro débranché', () => {
  it('détecte un micro choisi absent de la liste', () => {
    expect(isDeviceMissing('AirPodsPro-UID:input', inputs)).toBe(false);
    expect(isDeviceMissing('AirPodsPro-UID:input', inputs.filter((d) => d.transport !== 'bluetooth'))).toBe(true);
  });
  it('rien de choisi, ou liste inconnue (pas de sidecar) : pas d’alerte', () => {
    expect(isDeviceMissing(null, inputs)).toBe(false);
    expect(isDeviceMissing('', inputs)).toBe(false);
    expect(isDeviceMissing('AirPodsPro-UID:input', [])).toBe(false);
  });
  it('libellé de l’avertissement', () => {
    expect(missingInputLabel('AirPods Pro')).toBe('AirPods Pro — débranché, micro par défaut utilisé');
    expect(missingInputLabel(null, 'X-UID')).toBe('X-UID — débranché, micro par défaut utilisé');
  });
  it('icône selon le transport', () => {
    expect(['builtin', 'bluetooth', 'usb', 'virtual', 'other'].map(transportIcon)).toEqual(['laptop', 'bluetooth', 'usb', 'mic', 'mic']);
  });
});

describe('réglages d’un cœur plus ancien', () => {
  it('complète les périphériques à null', () => {
    const old = { audio: { auto_detect_calls: true, call_apps: [] }, voice: { feedback: 'system' } } as unknown as Settings;
    const n = normalizeSettings(old);
    expect(n.audio.input_device).toBeNull();
    expect(n.audio.input_device_name).toBeNull();
    expect(n.voice.feedback_device).toBeNull();
  });
});
