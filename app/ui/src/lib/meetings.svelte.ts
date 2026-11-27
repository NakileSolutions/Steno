// Réunions récentes (barre latérale), rafraîchies sur `scribe://meetings`.
import { api, onEvent } from './api';
import type { MeetingSummary } from './types';

export const recent = $state<{ list: MeetingSummary[] }>({ list: [] });

let started = false;
export function initRecent(): void {
  if (started) return;
  started = true;
  const load = () => api.listMeetings({ limit: 6 }).then((l) => (recent.list = l)).catch(() => {});
  load();
  onEvent('scribe://meetings', load);
  onEvent('scribe://report', load);
}
