// Réunions récentes (barre latérale), rafraîchies sur `steno://meetings`.
import { api, onEvent } from './api';
import type { MeetingSummary } from './types';

export const recent = $state<{ list: MeetingSummary[] }>({ list: [] });

let started = false;
export function initRecent(): void {
  if (started) return;
  started = true;
  const load = () => api.listMeetings({ limit: 6 }).then((l) => (recent.list = l)).catch(() => {});
  load();
  onEvent('steno://meetings', load);
  onEvent('steno://report', load);
}
