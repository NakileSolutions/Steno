// Sections de navigation et des réglages. Libellés en français (marqués `tk`) : à traduire
// à l'affichage avec `t(label)`.
import { tk } from './i18n.svelte';

export const SETTINGS_SECTIONS = [
  { id: 'agent', label: tk('Agent IA'), icon: 'agent' },
  { id: 'repos', label: tk('Dépôts'), icon: 'folder' },
  { id: 'connectors', label: tk('Connecteurs'), icon: 'plug' },
  { id: 'correlation', label: tk('Corrélation'), icon: 'link2' },
  { id: 'reports', label: tk('Comptes rendus'), icon: 'note' },
  { id: 'voice', label: tk('Voix'), icon: 'mic' },
  { id: 'audio', label: tk('Audio & transcription'), icon: 'wave' },
  { id: 'permissions', label: tk('Permissions macOS'), icon: 'lock' },
  { id: 'privacy', label: tk('Confidentialité'), icon: 'shield' },
  { id: 'updates', label: tk('Mises à jour'), icon: 'download' },
  { id: 'profile', label: tk('Profil'), icon: 'user' },
  { id: 'import', label: tk('Import / export'), icon: 'swap' },
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]['id'];

export const MAIN_SECTIONS = [
  { id: 'meetings', label: tk('Réunions'), icon: 'meetings', path: '/meetings' },
  { id: 'live', label: tk('En direct'), icon: 'live', path: '/live' },
  { id: 'search', label: tk('Recherche'), icon: 'search', path: '/search' },
  { id: 'actions', label: tk('Décisions & actions'), icon: 'actions', path: '/actions' },
  { id: 'settings', label: tk('Réglages'), icon: 'settings', path: '/settings' },
] as const;
