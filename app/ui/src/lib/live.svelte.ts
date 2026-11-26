// État « en direct » partagé (panneau de barre de menu, barre latérale, vue En direct).

import { api, onEvent } from './api';
import type { LiveState } from './types';

export const live = $state<{ s: LiveState; ready: boolean }>({
  s: {
    state: 'idle',
    meeting: null,
    levels: { me: -90, them: -90 },
    aside: false,
    sidecar: 'starting',
    last_error: null,
    pending_requests: 0,
  },
  ready: false,
});

export const clock = $state({ now: Date.now() });

let started = false;
export function initLive(): void {
  if (started) return;
  started = true;
  api.liveState().then((s) => {
    live.s = s;
    live.ready = true;
  }).catch(() => {});
  onEvent('scribe://live', (s) => {
    live.s = s;
    live.ready = true;
  });
  setInterval(() => (clock.now = Date.now()), 1000);
}

export const STATE_LABEL: Record<LiveState['state'], string> = {
  idle: 'Inactif',
  listening: 'Écoute',
  paused: 'En pause',
  processing: 'Traitement',
};
