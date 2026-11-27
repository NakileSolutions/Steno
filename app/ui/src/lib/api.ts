// Couche d'accès au backend : commandes Tauri (docs/contracts.md §5) et événements.
// Trois modes :
//  - Tauri (l'app) : `invoke` / `listen` de Tauri ;
//  - HTTP (`?backend=http` dans l'URL, servi par `scribe serve`) : POST /api/invoke/<commande>
//    et un flux SSE /api/events, même origine ; les opérations de fenêtre sont sans effet ;
//  - sinon (navigateur seul) : le faux backend en mémoire `mock.ts`.

import type {
  Action, ActionFilter, AgentInfo, AudioDevices, Connector, Decision, DoctorItem, EventMap, EventName,
  LiveState, Meeting, MeetingDetail, MeetingFilter, MeetingSummary, ModelInfo, Note, Repo, Report,
  Permissions, PermissionKind, Request, SearchHit, Segment, Settings, TestAgentResult, VariantCheck, WakeCheck, WakeDetect,
  ContextFile, MeetingContext, ReportTemplate,
} from './types';
import { normalizeLiveState, normalizeSettings } from './listening';

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

/** Mode backend HTTP (`?backend=http`) : tests e2e réels et développement sans Tauri. */
export const isHttp: boolean =
  !isTauri && typeof location !== 'undefined' && new URLSearchParams(location.search).get('backend') === 'http';

/** Message d'erreur d'une réponse HTTP en échec : `{"error": "…"}`, sinon le texte brut ou le statut. */
export function httpErrorMessage(status: number, body: string): string {
  try {
    const j = JSON.parse(body) as unknown;
    if (j && typeof j === 'object' && typeof (j as { error?: unknown }).error === 'string') return (j as { error: string }).error;
    if (typeof j === 'string' && j) return j;
  } catch {
    // Corps non JSON.
  }
  return body.trim() || `Erreur ${status} du serveur Scribe.`;
}

export function httpBackend(base = ''): Backend {
  // Commandes traitées dans la page : pas de boîte de dialogue native hors de Tauri.
  const local: Record<string, () => unknown> = { pick_folder: () => null };

  // Une seule connexion SSE, ouverte au premier abonnement ; un écouteur par nom d'événement.
  const subs = new Map<string, Set<(p: unknown) => void>>();
  let es: EventSource | null = null;
  let lostOnce = false;
  const dispatch = (name: string, payload: unknown) => {
    subs.get(name)?.forEach((cb) => {
      try {
        cb(payload);
      } catch (e) {
        console.error(e);
      }
    });
  };
  const attach = (name: string) => {
    es?.addEventListener(name, (e) => {
      let payload: unknown = null;
      try {
        payload = JSON.parse((e as MessageEvent<string>).data || 'null');
      } catch {
        return;
      }
      dispatch(name, payload);
    });
  };
  const ensureSource = () => {
    if (es) return;
    es = new EventSource(`${base}/api/events`);
    es.addEventListener('error', () => (lostOnce = true));
    es.addEventListener('open', () => {
      if (!lostOnce) return;
      // Reconnexion : des événements ont pu être perdus, on resynchronise l'essentiel.
      backend.invoke<LiveState>('live_state').then((s) => dispatch('scribe://live', s)).catch(() => {});
      dispatch('scribe://meetings', null);
    });
    for (const name of subs.keys()) attach(name);
  };

  const backend: Backend = {
    async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
      if (cmd in local) return local[cmd]() as T;
      let res: Response;
      try {
        res = await fetch(`${base}/api/invoke/${encodeURIComponent(cmd)}`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(args ?? {}),
        });
      } catch {
        throw new Error('Serveur Scribe injoignable.');
      }
      const text = await res.text();
      if (!res.ok) throw new Error(httpErrorMessage(res.status, text));
      return (text ? JSON.parse(text) : null) as T;
    },
    async listen(name, cb) {
      let set = subs.get(name);
      if (!set) {
        subs.set(name, (set = new Set()));
        if (es) attach(name);
      }
      ensureSource();
      const fn = cb as (p: unknown) => void;
      set.add(fn);
      return () => set!.delete(fn);
    },
    // Une seule page dans le navigateur : « ouvrir la fenêtre principale » revient à y naviguer.
    async openMain(route) {
      if (route) location.hash = '#' + route;
      else if (location.hash.startsWith('#/tray')) location.hash = '#/meetings';
    },
    async hideCurrent() {},
    async openExternal(url) {
      window.open(url, '_blank', 'noopener,noreferrer');
    },
  };
  return backend;
}

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
  : isHttp
    ? Promise.resolve(httpBackend())
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

/** Contenu d'un fichier en base64 (sans l'en-tête `data:`). */
export async function fileBase64(file: Blob): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin);
}

export const api = {
  // Direct
  liveState: () => call<LiveState>('live_state').then(normalizeLiveState),
  startMeeting: (title?: string, inPerson = false) =>
    call<Meeting>('start_meeting', { ...(title ? { title } : {}), ...(inPerson ? { in_person: true } : {}) }),
  stopMeeting: () => call<null>('stop_meeting'),
  setAside: (on: boolean) => call<null>('set_aside', { on }),
  setMicMuted: (on: boolean) => call<null>('set_mic_muted', { on }),
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

  // Contexte, documents et format du compte rendu
  getMeetingContext: (id: string) => call<MeetingContext>('get_meeting_context', { id }),
  setMeetingContext: (id: string, text: string, template: string | null) =>
    call<MeetingContext>('set_meeting_context', { id, text, template }),
  /** Joint un fichier choisi dans la page : son contenu part en base64, le texte est extrait par le cœur. */
  addContextFile: async (id: string, file: File) =>
    call<ContextFile>('add_context_file', { id, name: file.name, data: await fileBase64(file) }),
  deleteContextFile: (fileId: number) => call<null>('delete_context_file', { fileId }),
  listReportTemplates: () => call<ReportTemplate[]>('list_report_templates'),
  /** La rédaction du compte rendu part sans attendre la fin du délai de contexte. */
  reportNow: (id: string) => call<null>('report_now', { id }),
  correlate: (id: string) => call<null>('correlate', { id }),
  exportMeeting: (id: string, format: 'markdown' | 'txt' | 'srt' | 'vtt' = 'markdown') =>
    call<string>('export_meeting', format === 'markdown' ? { id } : { id, format }),
  saveExport: (id: string, format: 'markdown' | 'txt' | 'srt' | 'vtt') => call<string | null>('save_export', { id, format }),
  printWindow: () => call<null>('print_window'),
  pauseMeeting: () => call<null>('pause_meeting'),
  resumeMeeting: () => call<null>('resume_meeting'),

  // Recherche, décisions, actions
  search: (query: string) => call<SearchHit[]>('search', { query }),
  listActions: (filter: ActionFilter = {}) => call<Action[]>('list_actions', { filter }),
  setActionStatus: (id: number, status: 'open' | 'done') => call<null>('set_action_status', { id, status }),
  listDecisions: (since?: string) => call<Decision[]>('list_decisions', since ? { since } : {}),

  // Réglages
  getSettings: () => call<Settings>('get_settings').then(normalizeSettings),
  saveSettings: (settings: Settings) => call<Settings>('save_settings', { settings }).then(normalizeSettings),
  exportSettings: () => call<string>('export_settings'),
  importSettings: (toml: string) => call<Settings>('import_settings', { toml }).then(normalizeSettings),

  // Agents, connecteurs, dépôts
  detectAgents: () => call<AgentInfo[]>('detect_agents'),
  listModels: (provider: string) => call<ModelInfo[]>('list_models', { provider }),
  /** Teste l'agent ; `model` et `effort` vides = défauts du CLI (non transmis). */
  testAgent: (provider: string, model?: string, effort?: string) =>
    call<TestAgentResult>('test_agent', {
      provider,
      ...(model?.trim() ? { model: model.trim() } : {}),
      ...(effort?.trim() ? { effort: effort.trim() } : {}),
    }),
  listConnectors: () => call<Connector[]>('list_connectors'),
  refreshConnectors: () => call<Connector[]>('refresh_connectors'),
  updateConnector: (id: number, useLive: boolean, useCorrelation: boolean) =>
    call<Connector>('update_connector', { id, useLive, useCorrelation }),
  listRepos: () => call<Repo[]>('list_repos'),
  addRepo: (path: string, label?: string) => call<Repo>('add_repo', label ? { path, label } : { path }),
  updateRepo: (repo: Repo) => call<Repo>('update_repo', { repo }),
  removeRepo: (id: number) => call<null>('remove_repo', { id }),
  pickFolder: () => call<string | null>('pick_folder'),

  // Périphériques audio (dernière liste reçue du sidecar ; vide sans sidecar)
  listAudioDevices: () =>
    call<AudioDevices>('list_audio_devices').then((d) => ({ inputs: d?.inputs ?? [], outputs: d?.outputs ?? [] })),

  // Voix
  checkWakeWord: (word: string, strict: boolean) => call<WakeCheck>('check_wake_word', { word, strict }),
  generateVariants: (word: string) => call<VariantCheck>('generate_variants', { word }),
  checkVariants: (word: string, variants: string[]) => call<VariantCheck>('check_variants', { word, variants }),
  wakeDetect: (text: string) => call<WakeDetect>('wake_detect', { text }),
  wakeTestRecord: (seconds: number) => call<{ text: string }>('wake_test_record', { seconds }),

  // Divers
  doctor: () => call<DoctorItem[]>('doctor'),
  deleteAllData: () => call<null>('delete_all_data'),

  // Permissions macOS et premier lancement
  getPermissions: () => call<Permissions>('get_permissions'),
  requestPermission: (kind: PermissionKind) => call<null>('request_permission', { kind }),
  openPermissionSettings: (kind: PermissionKind) => call<null>('open_permission_settings', { kind }),
  completeOnboarding: () => call<null>('complete_onboarding'),
  deleteSegment: (id: number) => call<null>('delete_segment', { id }),
};
