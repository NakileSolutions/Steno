export const SETTINGS_SECTIONS = [
  { id: 'agent', label: 'Agent IA', icon: 'agent' },
  { id: 'repos', label: 'Dépôts', icon: 'folder' },
  { id: 'connectors', label: 'Connecteurs', icon: 'plug' },
  { id: 'correlation', label: 'Corrélation', icon: 'link2' },
  { id: 'voice', label: 'Voix', icon: 'mic' },
  { id: 'audio', label: 'Audio & transcription', icon: 'wave' },
  { id: 'privacy', label: 'Confidentialité', icon: 'shield' },
  { id: 'profile', label: 'Profil', icon: 'user' },
  { id: 'import', label: 'Import / export', icon: 'swap' },
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number]['id'];

export const MAIN_SECTIONS = [
  { id: 'meetings', label: 'Réunions', icon: 'meetings', path: '/meetings' },
  { id: 'live', label: 'En direct', icon: 'live', path: '/live' },
  { id: 'search', label: 'Recherche', icon: 'search', path: '/search' },
  { id: 'actions', label: 'Décisions & actions', icon: 'actions', path: '/actions' },
  { id: 'settings', label: 'Réglages', icon: 'settings', path: '/settings' },
] as const;
