// Couche d'accès au backend : commandes Tauri (docs/contracts.md §5) et événements.
// Hors de Tauri (navigateur), bascule automatiquement sur le faux backend `mock.ts`.

import type {
  Action, ActionFilter, AgentInfo, Connector, Decision, DoctorItem, EventMap, EventName,
  LiveState, Meeting, MeetingDetail, MeetingFilter, MeetingSummary, Note, Repo, Report,
  Request, SearchHit, Segment, Settings, TestAgentResult, VariantCheck, WakeCheck, WakeDetect,
} from './types';

export type Unlisten = () => void;

export interface Backend {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<K extends EventName>(name: K, cb: (payload: EventMap[K]) => void): Promise<Unlisten>;
  /** Affiche la fenêtre principale (depuis le panneau de barre de menu) et y ouvre `route`. */
  openMain(route?: string): Promise<void>;
  /** Masque la fenêtre courante (panneau de barre de menu). */
  hideCurrent(): Promise<void>;
  /** Ouvre un lien http(s) dans le navigateur du système. */
  openExternal(url: string): Promise<void>;
}

export const isTauri: boolean = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function tauriBackend(): Promise<Backend> {
  const core = await import('@tauri-apps/api/core');
  const ev = await import('@tauri-apps/api/event');
  const win = await import('@tauri-apps/api/window');
  return {
    invoke: (cmd, args) => core.invoke(cmd, args),
    listen: (name, cb) => ev.listen(name, (e) => cb(e.payload as never)),
    async openMain(route) {
      const main = await win.Window.getByLabel('main');
      if (main) {
        await main.show();
        await main.unminimize();
        await main.setFocus();
        if (route) await ev.emitTo('main', 'scribe://navigate', { route });
      }
      const current = win.getCurrentWindow();
      if (current.label !== 'main') await current.hide();
    },
    async hideCurrent() {
      const current = win.getCurrentWindow();
      if (current.label !== 'main') await current.hide();
    },
    async openExternal(url) {
      try {
        const opener = await import('@tauri-apps/plugin-opener');
        await opener.openUrl(url);
      } catch {
        // Plugin absent ou refusé : on ne navigue jamais dans la WebView.
      }
    },
  };
}

const backendPromise: Promise<Backend> = isTauri
  ? tauriBackend()
  : import('./mock').then((m) => m.createMockBackend());

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const b = await backendPromise;
  try {
    return await b.invoke<T>(cmd, args);
  } catch (e) {
    // Les erreurs du backend sont des chaînes en français.
    throw new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e));
  }
}

/** Abonnement à un événement `scribe://…`. Renvoie une fonction de désabonnement synchrone. */
export function onEvent<K extends EventName>(name: K, cb: (payload: EventMap[K]) => void): Unlisten {
  let un: Unlisten | null = null;
  let cancelled = false;
  backendPromise
    .then((b) => b.listen(name, cb))
    .then((u) => {
      if (cancelled) u();
      else un = u;
    })
    .catch(() => {});
  return () => {
    cancelled = true;
    un?.();
  };
}

export async function openMain(route?: string): Promise<void> {
  return (await backendPromise).openMain(route);
}
export async function hideCurrent(): Promise<void> {
  return (await backendPromise).hideCurrent();
}
export async function openExternal(url: string): Promise<void> {
  if (!/^https?:\/\//i.test(url)) return;
  return (await backendPromise).openExternal(url);
}

export const api = {
  // Direct
  liveState: () => call<LiveState>('live_state'),
  startMeeting: (title?: string) => call<Meeting>('start_meeting', title ? { title } : {}),
  stopMeeting: () => call<null>('stop_meeting'),
  setAside: (on: boolean) => call<null>('set_aside', { on }),
  setLiveConfidential: (on: boolean) => call<null>('set_live_confidential', { on }),

  // Demandes
  ask: (query: string, meetingId?: string) =>
    call<Request>('ask', meetingId ? { query, meetingId } : { query }),
  askHistory: (query: string) => call<Request>('ask_history', { query }),
  cancelRequest: (id: string) => call<null>('cancel_request', { id }),
  recentRequests: (limit: number) => call<Request[]>('recent_requests', { limit }),

  // Réunions
  listMeetings: (filter: MeetingFilter = {}) => call<MeetingSummary[]>('list_meetings', { filter }),
  getMeeting: (id: string) => call<MeetingDetail>('get_meeting', { id }),
  getTranscript: (id: string, includePrivate: boolean) =>
    call<Segment[]>('get_transcript', { id, includePrivate }),
  getReportVersion: (id: string, version: number) => call<Report>('get_report_version', { id, version }),
  renameMeeting: (id: string, title: string) => call<null>('rename_meeting', { id, title }),
  setConfidential: (id: string, on: boolean) => call<null>('set_confidential', { id, on }),
  deleteMeeting: (id: string) => call<null>('delete_meeting', { id }),
  renameSpeaker: (meetingId: string, label: string, displayName: string) =>
    call<null>('rename_speaker', { meetingId, label, displayName }),
  addNote: (meetingId: string, text: string) => call<Note>('add_note', { meetingId, text }),
  updateNote: (id: number, text: string) => call<null>('update_note', { id, text }),
  deleteNote: (id: number) => call<null>('delete_note', { id }),
  regenerateReport: (id: string) => call<null>('regenerate_report', { id }),
  correlate: (id: string) => call<null>('correlate', { id }),
  exportMeeting: (id: string) => call<string>('export_meeting', { id }),

  // Recherche, décisions, actions
  search: (query: string) => call<SearchHit[]>('search', { query }),
  listActions: (filter: ActionFilter = {}) => call<Action[]>('list_actions', { filter }),
  setActionStatus: (id: number, status: 'open' | 'done') => call<null>('set_action_status', { id, status }),
  listDecisions: (since?: string) => call<Decision[]>('list_decisions', since ? { since } : {}),

  // Réglages
  getSettings: () => call<Settings>('get_settings'),
  saveSettings: (settings: Settings) => call<Settings>('save_settings', { settings }),
  exportSettings: () => call<string>('export_settings'),
  importSettings: (toml: string) => call<Settings>('import_settings', { toml }),

  // Agents, connecteurs, dépôts
  detectAgents: () => call<AgentInfo[]>('detect_agents'),
  testAgent: (provider: string) => call<TestAgentResult>('test_agent', { provider }),
  listConnectors: () => call<Connector[]>('list_connectors'),
  refreshConnectors: () => call<Connector[]>('refresh_connectors'),
  updateConnector: (id: number, useLive: boolean, useCorrelation: boolean) =>
    call<Connector>('update_connector', { id, useLive, useCorrelation }),
  listRepos: () => call<Repo[]>('list_repos'),
  addRepo: (path: string, label?: string) => call<Repo>('add_repo', label ? { path, label } : { path }),
  updateRepo: (repo: Repo) => call<Repo>('update_repo', { repo }),
  removeRepo: (id: number) => call<null>('remove_repo', { id }),
  pickFolder: () => call<string | null>('pick_folder'),

  // Voix
  checkWakeWord: (word: string, strict: boolean) => call<WakeCheck>('check_wake_word', { word, strict }),
  generateVariants: (word: string) => call<VariantCheck>('generate_variants', { word }),
  checkVariants: (word: string, variants: string[]) => call<VariantCheck>('check_variants', { word, variants }),
  wakeDetect: (text: string) => call<WakeDetect>('wake_detect', { text }),
  wakeTestRecord: (seconds: number) => call<{ text: string }>('wake_test_record', { seconds }),

  // Divers
  doctor: () => call<DoctorItem[]>('doctor'),
  deleteAllData: () => call<null>('delete_all_data'),
};
