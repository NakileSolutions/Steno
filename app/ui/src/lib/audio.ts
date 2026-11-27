// Logique pure autour des périphériques audio (contracts.md §4 `audio.input_device`,
// `voice.feedback_device` ; §5 `list_audio_devices`).

import type { AudioDevice } from './types';

/** « Micro par défaut du système (Micro du MacBook Pro) » ; sans nom connu, sans parenthèses. */
export function defaultInputLabel(inputs: readonly AudioDevice[] | null | undefined): string {
  const d = inputs?.find((x) => x.default);
  return d ? `Micro par défaut du système (${d.name})` : 'Micro par défaut du système';
}

/** « Sortie par défaut (AirPods Pro) ». */
export function defaultOutputLabel(outputs: readonly AudioDevice[] | null | undefined): string {
  const d = outputs?.find((x) => x.default);
  return d ? `Sortie par défaut (${d.name})` : 'Sortie par défaut';
}

/**
 * Le périphérique enregistré n'est plus branché. Faux si rien n'est choisi, ou si la liste
 * est vide (pas de sidecar, hors Mac) : on ne sait pas.
 */
export function isDeviceMissing(id: string | null | undefined, devices: readonly AudioDevice[] | null | undefined): boolean {
  if (!id || !devices?.length) return false;
  return !devices.some((d) => d.id === id);
}

/** « AirPods Pro — débranché, micro par défaut utilisé ». */
export function missingInputLabel(name: string | null | undefined, id?: string | null): string {
  return `${name?.trim() || id?.trim() || 'Micro choisi'} — débranché, micro par défaut utilisé`;
}

export const TRANSPORT_LABEL: Record<AudioDevice['transport'], string> = {
  builtin: 'Intégré',
  usb: 'USB',
  bluetooth: 'Bluetooth',
  virtual: 'Virtuel',
  aggregate: 'Agrégé',
  other: 'Autre',
};

/** Icône (Icon.svelte) selon le transport : intégré, bluetooth, usb, autre. */
export function transportIcon(t: AudioDevice['transport'] | string): string {
  if (t === 'builtin') return 'laptop';
  if (t === 'bluetooth') return 'bluetooth';
  if (t === 'usb') return 'usb';
  return 'mic';
}
