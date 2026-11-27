// Logique pure autour des périphériques audio (contracts.md §4 `audio.input_device`,
// `voice.feedback_device` ; §5 `list_audio_devices`).

import type { AudioDevice } from './types';
import { t, tk } from './i18n.svelte';

/** « Micro par défaut du système (Micro du MacBook Pro) » ; sans nom connu, sans parenthèses. */
export function defaultInputLabel(inputs: readonly AudioDevice[] | null | undefined): string {
  const d = inputs?.find((x) => x.default);
  return d ? t('Micro par défaut du système ({name})', { name: d.name }) : t('Micro par défaut du système');
}

/** « Sortie par défaut (AirPods Pro) ». */
export function defaultOutputLabel(outputs: readonly AudioDevice[] | null | undefined): string {
  const d = outputs?.find((x) => x.default);
  return d ? t('Sortie par défaut ({name})', { name: d.name }) : t('Sortie par défaut');
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
  return t('{name} — débranché, micro par défaut utilisé', { name: name?.trim() || id?.trim() || t('Micro choisi') });
}

/** Libellés (marqués `tk`) : à traduire à l'affichage avec `t(...)`. */
export const TRANSPORT_LABEL: Record<AudioDevice['transport'], string> = {
  builtin: tk('Intégré'),
  usb: tk('USB'),
  bluetooth: tk('Bluetooth'),
  virtual: tk('Virtuel'),
  aggregate: tk('Agrégé'),
  other: tk('Autre'),
};

/** Icône (Icon.svelte) selon le transport : intégré, bluetooth, usb, autre. */
export function transportIcon(t: AudioDevice['transport'] | string): string {
  if (t === 'builtin') return 'laptop';
  if (t === 'bluetooth') return 'bluetooth';
  if (t === 'usb') return 'usb';
  return 'mic';
}
