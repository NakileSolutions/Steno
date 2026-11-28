// Faux backend en mémoire : utilisé automatiquement hors de Tauri (développement, captures).
// Il imite les commandes et les événements de docs/contracts.md §5.

import type { Backend, Unlisten } from './api';
import type {
  Action, ActionFilter, DoctorItem, EventMap, EventName, LiveState, Meeting, MeetingDetail,
  MeetingFilter, MeetingSummary, Note, Repo, Request, SearchHit, Segment, Settings,
  PermissionKind, PermissionValue, VariantCheck, WakeCheck, WakeDetect,
  ContextFile, MeetingContext, ReportTemplate, SkillStatus, UpdateStatus, Participant, UpcomingEvent,
} from './types';
import {
  LIVE_SCRIPT, TEAMS_TITLE, defaultSettings, iso, nextIds, seedAgents, seedAudioDevices, seedConnectors,
  seedHistoryRequests, seedMeetings, seedModels, seedRepos, type MeetingStore, MOCK_BUILTIN_TEMPLATES,
} from './mock-data';
import { detectWake, fold } from './wake';
import { effortLabel, normalizeAgentSettings } from './agents';
import { isLinux } from './platform';

// Skill « Sténo » simulé : pas encore installé.
const skillStatus: SkillStatus = {
  targets: [
    { agents: 'Claude Code', path: '~/.claude/skills/steno', state: 'missing' },
    { agents: 'Codex, Gemini CLI, OpenCode…', path: '~/.agents/skills/steno', state: 'missing' },
  ],
  mcp: [{ agent: 'Claude Code', registered: false }],
  cli: '~/.local/bin/steno',
};

// Réglages simulés conservés dans l'onglet (rechargement de la page) ; jamais requis.
const SETTINGS_KEY = 'steno-mock-settings';
function loadSettings(): Settings {
  try {
    const raw = sessionStorage.getItem(SETTINGS_KEY);
    if (raw) return normalizeAgentSettings({ ...defaultSettings(), ...(JSON.parse(raw) as Settings) });
  } catch {
    // Stockage indisponible ou illisible : réglages par défaut.
  }
  return defaultSettings();
}
function storeSettings(s: Settings) {
  try {
    sessionStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
  } catch {
    // Stockage indisponible : les réglages restent en mémoire.
  }
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const clone = <T>(v: T): T => structuredClone(v);

// Excerpt of the core list (`crates/steno-core/src/wake/forbidden_fr.txt`): neighbours of
// « Sténo », then of « Scribe » (the former default, still a possible wake word).
const BANNED_VARIANTS: Record<string, string> = {
  stenographie: 'Mot courant en français (liste intégrée).',
  creneau: 'Mot courant en réunion de dev (liste intégrée).',
  scenario: 'Mot courant en réunion de dev (liste intégrée).',
  stylo: 'Mot courant en français (liste intégrée).',
  tenor: 'Mot courant en français (liste intégrée).',
  stereo: 'Mot courant en français (liste intégrée).',
  script:'Mot courant en réunion de dev (liste intégrée).',
  scripts: 'Mot courant en réunion de dev (liste intégrée).',
  description: 'Mot courant en réunion de dev (liste intégrée).',
  inscrit: 'Mot courant en français (liste intégrée).',
  inscrite: 'Mot courant en français (liste intégrée).',
  prescrire: 'Mot courant en français (liste intégrée).',
  transcrire: 'Mot courant en français (liste intégrée).',
  décrit: 'Mot courant en français (liste intégrée).',
};

export function createMockBackend(): Backend {
  const listeners = new Map<string, Set<(p: unknown) => void>>();
  const emit = <K extends EventName>(name: K, payload: EventMap[K]) => {
    listeners.get(name)?.forEach((cb) => {
      try {
        cb(clone(payload));
      } catch (e) {
        console.error(e);
      }
    });
  };

  let meetings: MeetingStore[] = seedMeetings();
  let historyRequests: Request[] = seedHistoryRequests();
  let settings: Settings = loadSettings();
  let connectors = seedConnectors();
  let repos = seedRepos();
  const agents = seedAgents();
  const audioDevices = seedAudioDevices();
  const unplugged = new Set<string>();
  let reqSeq = 100;
  let wakeTestCount = 0;
  const permissions: Record<PermissionKind, PermissionValue> = { microphone: 'granted', system_audio: 'unknown', accessibility: 'denied', calendar: 'granted' };

  const liveStore = () => meetings.find((m) => m.meeting.status === 'live');
  const find = (id: string) => {
    const m = meetings.find((x) => x.meeting.id === id);
    if (!m) throw 'Réunion introuvable.';
    return m;
  };

  // L'écoute ne démarre jamais seule : un appel (Teams au départ) est simplement signalé.
  const live: LiveState = {
    state: 'idle',
    meeting: null,
    levels: { me: -90, them: -90 },
    aside: false,
    mic_muted: false,
    sidecar: 'running',
    last_error: null,
    pending_requests: 0,
    call: { active: true, app: 'teams', app_name: 'Microsoft Teams', title: TEAMS_TITLE },
  };
  const appName = (id: string) => settings.audio.call_apps.find((a) => a.id === id)?.name ?? id;
  const emitLive = () => {
    live.pending_requests = allRequests().filter((r) => r.status === 'queued' || r.status === 'running').length;
    const m = liveStore();
    live.meeting = m ? clone(m.meeting) : live.state === 'processing' ? live.meeting : null;
    emit('steno://live', live);
  };

  /** Comme le cœur : un appel d'une app désactivée n'est plus signalé. */
  const forgetDisabledCall = () => {
    if (live.call.active && !settings.audio.call_apps?.some((x) => x.id === live.call.app && x.enabled)) {
      live.call = { active: false, app: null, app_name: null, title: null };
      emitLive();
    }
  };

  function allRequests(): Request[] {
    return [...meetings.flatMap((m) => m.requests), ...historyRequests];
  }

  // --- Simulation de la réunion en direct ---
  let scriptPos = 0;
  let speaking = { me: false, them: false };
  const levelTimer = () => {
    if (live.state !== 'listening') {
      live.levels = { me: -90, them: -90 };
      return;
    }
    const target = (on: boolean) => (on ? -18 - Math.random() * 14 : -52 - Math.random() * 8);
    live.levels = {
      me: Math.round((live.levels.me * 0.35 + target(speaking.me || live.aside) * 0.65) * 10) / 10,
      them: Math.round((live.levels.them * 0.35 + target(speaking.them) * 0.65) * 10) / 10,
    };
    emit('steno://live', live);
  };
  setInterval(levelTimer, 250);

  function pushSegment(store: MeetingStore, speaker: string, text: string, priv = false): Segment {
    const start = new Date(store.meeting.started_at).getTime();
    const t0 = Math.round((Date.now() - start) / 100) / 10;
    const seg: Segment = {
      id: nextIds.seg(), meeting_id: store.meeting.id, seq: store.segments.length + 1, ts: iso(new Date()),
      t0, t1: t0 + Math.max(2, Math.round(text.length / 14)), channel: speaker === 'Moi' ? 'me' : 'them',
      speaker, text, private: priv, is_echo: false, request_id: null,
    };
    store.segments.push(seg);
    return seg;
  }

  const wakeDetect = (text: string): WakeDetect => detectWake(text, settings.voice);

  function createRequest(query: string, meetingId: string | null, origin: Request['origin'], intent?: Request['intent']): Request {
    const q = query.trim();
    let it: Request['intent'] = intent ?? 'search';
    if (!intent) {
      const f = fold(q);
      if (/^(note|retiens|ajoute)/.test(f)) it = 'note';
      else if (/^(verifie|confirme)/.test(f)) it = 'check';
      else if (/^(resume|recap)/.test(f)) it = 'recap';
    }
    const req: Request = {
      id: `req-${++reqSeq}`, meeting_id: meetingId, created_at: iso(new Date()), origin, intent: it, query: q,
      context_seconds: meetingId ? settings.agent.live_context_seconds : null,
      tier: it === 'note' || it === 'recap' ? 'quick' : 'search', status: 'queued', started_at: null,
      finished_at: null, error: null, summary: null, answer_md: null, tools_used: [],
    };
    const store = meetingId ? meetings.find((m) => m.meeting.id === meetingId) : undefined;
    if (store) store.requests.push(req);
    else historyRequests.push(req);

    // Politique lecture seule : toute demande de publication est refusée.
    if (/\b(poste|publie|envoie|partage)\b/i.test(q) && it !== 'note') {
      req.status = 'refused';
      req.error = 'Demande refusée : Sténo ne publie rien à l’extérieur (lecture seule).';
      req.finished_at = iso(new Date());
      setTimeout(() => { emit('steno://request', req); emitLive(); }, 50);
      return clone(req);
    }

    if (store?.meeting.confidential && it !== 'note') {
      req.status = 'refused';
      req.error = 'Réunion confidentielle : aucun appel à l’IA.';
      req.finished_at = iso(new Date());
      setTimeout(() => { emit('steno://request', req); emitLive(); }, 50);
      return clone(req);
    }

    setTimeout(() => { emit('steno://request', req); emitLive(); }, 50);
    setTimeout(() => {
      if (req.status !== 'queued') return;
      req.status = 'running';
      req.started_at = iso(new Date());
      emit('steno://request', req);
    }, 700);
    setTimeout(() => {
      if (req.status !== 'running') return;
      req.status = 'done';
      req.finished_at = iso(new Date());
      if (it === 'note') {
        req.answer_md = `Note ajoutée : **${q.replace(/^note\s*:?\s*/i, '')}**.`;
        req.summary = 'Note ajoutée.';
        if (store) store.notes.push({ id: nextIds.note(), meeting_id: store.meeting.id, ts: iso(new Date()), text: q.replace(/^note\s*:?\s*/i, ''), source: 'voice' });
      } else if (it === 'history') {
        req.answer_md = `D'après vos réunions des 30 derniers jours :\n\n- **Daily ORION** (aujourd'hui) : pagination par 500 pour le batch de stations (APP-1251).\n- **Revue d'architecture** (hier) : export Brindille par fichier SFTP quotidien.\n\n_Recherche effectuée via le serveur MCP Sténo._`;
        req.summary = 'Deux réunions pertinentes trouvées.';
        req.tools_used = ['steno.search_meetings', 'steno.get_report'];
      } else if (/numéro de cadre/i.test(q)) {
        req.answer_md = `La validation est faite dans \`FrameNumberValidator.java:23\` par l'expression \`^[A-Z]{2}\\d{6}$\` : **aucune règle** pour les cadres importés ni les anciens modèles.\n\n- Les vélos cargo ont un préfixe à trois lettres et sont refusés.\n- Aucune table de référence des modèles dans le dépôt.\n\n**Sources**\n- [FrameNumberValidator.java:23](https://gitlab.example.com/orion/ms-fleet/-/blob/main/src/main/java/fr/orion/fleet/validation/FrameNumberValidator.java#L23)\n- [APP-1260 — Validation des numéros de cadre](https://jira.example.com/browse/APP-1260)`;
        req.summary = 'Regex ^[A-Z]{2}\\d{6}$ dans FrameNumberValidator.java:23, sans cadres importés ni vélos cargo.';
        req.tools_used = ['code.grep', 'code.read', 'jira.get_issue'];
      } else {
        req.answer_md = `Voici ce que j'ai trouvé pour « ${q} » :\n\n- Aucun résultat probant dans les dépôts cochés.\n- Deux tickets Jira proches : [APP-1251](https://jira.example.com/browse/APP-1251), [APP-1234](https://jira.example.com/browse/APP-1234).\n\n**Sources**\n- [Recherche Jira](https://jira.example.com/issues/?jql=text~%22${encodeURIComponent(q)}%22)`;
        req.summary = 'Deux tickets proches, rien dans le code.';
        req.tools_used = ['code.grep', 'jira.search'];
      }
      emit('steno://request', req);
      emitLive();
    }, it === 'note' ? 1400 : 6500);
    return clone(req);
  }

  /** Une phrase entendue pendant l'écoute : transcrite, puis éventuellement commande vocale. */
  function hear(store: MeetingStore, speaker: string, text: string) {
    speaking = { me: speaker === 'Moi', them: speaker !== 'Moi' };
    setTimeout(() => (speaking = { me: false, them: false }), 1600);
    const priv = live.aside && speaker === 'Moi';
    const seg = pushSegment(store, speaker, text, priv);
    const det = speaker === 'Moi' && !priv ? wakeDetect(text) : null;
    if (det?.detected && det.intent === 'stop') {
      // « Sténo, arrête l'écoute » : seule façon, avec le bouton et le raccourci, de terminer.
      const now = iso(new Date());
      const req: Request = {
        id: `req-${++reqSeq}`, meeting_id: store.meeting.id, created_at: now, origin: 'voice', intent: 'stop',
        query: text.replace(/^[^,]*,\s*/, '').replace(/[.!]+$/, ''), context_seconds: null, tier: 'quick',
        status: 'done', started_at: now, finished_at: now, error: null, summary: 'Écoute arrêtée.',
        answer_md: 'Écoute arrêtée à la voix. Le compte rendu est en préparation.', tools_used: [],
      };
      store.requests.push(req);
      seg.request_id = req.id;
      emit('steno://segment', seg);
      emit('steno://request', req);
      setTimeout(() => liveStore() === store && finishMeeting(store), 300);
      return;
    }
    if (det?.detected && det.query) {
      const req = createRequest(det.query, store.meeting.id, 'voice', det.intent as Request['intent']);
      seg.request_id = req.id;
    }
    emit('steno://segment', seg);
  }

  const segTimer = () => {
    const store = liveStore();
    if (!store || live.state !== 'listening') return;
    const [speaker, text] = LIVE_SCRIPT[scriptPos % LIVE_SCRIPT.length];
    scriptPos++;
    hear(store, speaker, text);
  };
  setInterval(segTimer, 2800);

  /** Rédactions en attente (délai laissé pour ajouter du contexte), comme la file du cœur. */
  const pendingReports = new Map<string, { at: number; timer: ReturnType<typeof setTimeout> }>();

  function writeReport(store: MeetingStore) {
    pendingReports.delete(store.meeting.id);
    store.meeting.status = 'done';
    store.reports.push({
      meeting_id: store.meeting.id, version: store.reports.length + 1, correlated: false,
      generated_at: iso(new Date()), provider: settings.agent.provider, model: null,
      markdown: `# ${store.meeting.title ?? 'Réunion'}\n\n## En bref\n- ${store.segments.length} interventions transcrites.\n- Estimation des tickets APP-1255 et APP-1260.\n\n## Décisions\n- APP-1260 estimé à 3 points, avec la table de référence des modèles de vélos.\n\n## Actions — qui / quoi / échéance\n| Qui | Quoi | Échéance |\n|---|---|---|\n| Moi | Analyser APP-1262 (doublons de comptes) | cette semaine |`,
    });
    store.decisions.push({ id: nextIds.decision(), meeting_id: store.meeting.id, text: 'APP-1260 estimé à 3 points, avec la table de référence des modèles de vélos.', created_at: iso(new Date()) });
    store.actions.push({ id: nextIds.action(), meeting_id: store.meeting.id, owner: 'Moi', text: 'Analyser APP-1262 (doublons de comptes)', due: null, status: 'open' });
    emit('steno://report', { meeting_id: store.meeting.id, version: store.reports.length });
    emit('steno://meetings', null);
  }

  function scheduleReport(store: MeetingStore, at: number) {
    const prev = pendingReports.get(store.meeting.id);
    if (prev) clearTimeout(prev.timer);
    const timer = setTimeout(() => writeReport(store), Math.max(0, at - Date.now()));
    pendingReports.set(store.meeting.id, { at, timer });
  }

  /** Une modification du contexte repousse une rédaction en attente d'au moins 30 s. */
  function contextTouched(id: string) {
    const p = pendingReports.get(id);
    const store = meetings.find((x) => x.meeting.id === id);
    if (p && store && p.at < Date.now() + 30_000) scheduleReport(store, Date.now() + 30_000);
  }

  function finishMeeting(store: MeetingStore) {
    store.meeting.status = 'processing';
    store.meeting.ended_at = iso(new Date());
    live.state = 'processing';
    live.meeting = clone(store.meeting);
    emitLive();
    emit('steno://meetings', null);
    if (store.meeting.confidential) store.meeting.status = 'ended';
    else scheduleReport(store, Date.now() + Math.max(3, settings.report?.context_delay_s ?? 0) * 1000);
    setTimeout(() => {
      live.state = 'idle';
      live.meeting = null;
      live.aside = false;
      live.mic_muted = false;
      emitLive();
      emit('steno://meetings', null);
    }, 3000);
  }

  // --- Utilitaires de lecture ---
  function summary(s: MeetingStore, query?: string): MeetingSummary {
    const report = s.reports.at(-1);
    let snippet: string | null = null;
    if (query) {
      const fq = fold(query);
      const seg = s.segments.find((x) => !x.private && fold(x.text).includes(fq));
      if (seg) snippet = seg.text;
      else if (report) {
        const line = report.markdown.split('\n').find((l) => fold(l).includes(fq));
        snippet = line ? line.replace(/^[#\-|*\s]+/, '') : null;
      }
    } else if (report) {
      const m = report.markdown.split('## En bref')[1]?.split('\n').find((l) => l.startsWith('- '));
      snippet = m ? m.slice(2).replace(/[`*]/g, '') : null;
    } else if (s.segments.length) {
      snippet = s.segments.filter((x) => !x.private).at(-1)?.text ?? null;
    }
    return {
      ...clone(s.meeting),
      correlated: !!report?.correlated,
      open_actions: s.actions.filter((a) => a.status === 'open').length,
      segment_count: s.segments.length,
      snippet,
    };
  }

  function matches(s: MeetingStore, q: string): boolean {
    const fq = fold(q);
    return (
      fold(s.meeting.title ?? '').includes(fq) ||
      s.segments.some((x) => !x.private && fold(x.text).includes(fq)) ||
      fold(s.reports.at(-1)?.markdown ?? '').includes(fq)
    );
  }

  function actionWithMeeting(a: Action): Action {
    const s = meetings.find((m) => m.meeting.id === a.meeting_id);
    return { ...clone(a), meeting_title: s?.meeting.title ?? null, meeting_started_at: s?.meeting.started_at };
  }

  function syllables(word: string): number {
    const groups = fold(word).replace(/e$/, '').match(/[aeiouy]+/g);
    return Math.max(1, groups?.length ?? 1);
  }

  function checkVariants(word: string, variants: string[]): VariantCheck {
    const res: VariantCheck = { accepted: [], rejected: [] };
    const vocab = settings.audio.vocabulary.map(fold);
    for (const raw of variants) {
      const v = raw.trim().toLowerCase();
      if (!v) continue;
      const f = fold(v);
      const banned = BANNED_VARIANTS[v] ?? BANNED_VARIANTS[f];
      if (banned) res.rejected.push({ variant: v, reason: banned });
      else if (vocab.includes(f)) res.rejected.push({ variant: v, reason: 'Fait partie du vocabulaire de l’équipe.' });
      else if (f.length < 3) res.rejected.push({ variant: v, reason: 'Trop court : déclencherait à tort.' });
      else if (!res.accepted.includes(v)) res.accepted.push(v);
    }
    void word;
    return res;
  }

  // --- Mini TOML (import/export des réglages) ---
  function toToml(s: Settings): string {
    const out: string[] = ['# Réglages Sténo — sans chemins ni données personnelles', ''];
    const val = (v: unknown): string =>
      Array.isArray(v) ? `[${v.map(val).join(', ')}]` : typeof v === 'string' ? JSON.stringify(v) : String(v);
    const isTableArray = (v: unknown) => Array.isArray(v) && v.length > 0 && v.every((x) => x !== null && typeof x === 'object');
    const walk = (obj: Record<string, unknown>, path: string, header = path ? `[${path}]` : '') => {
      const scalars = Object.entries(obj).filter(([, v]) => v === null || typeof v !== 'object' || (Array.isArray(v) && !isTableArray(v)));
      const tables = Object.entries(obj).filter(([, v]) => v !== null && typeof v === 'object' && !Array.isArray(v));
      const arrays = Object.entries(obj).filter(([, v]) => isTableArray(v));
      if (header) out.push(header);
      for (const [k, v] of scalars) if (v !== null) out.push(`${k} = ${val(v)}`);
      if (header) out.push('');
      for (const [k, v] of tables) walk(v as Record<string, unknown>, path ? `${path}.${k}` : k);
      for (const [k, v] of arrays) {
        const p = path ? `${path}.${k}` : k;
        for (const item of v as Record<string, unknown>[]) walk(item, p, `[[${p}]]`);
      }
    };
    const copy = clone(s) as unknown as Record<string, Record<string, unknown>>;
    delete copy.profile.name;
    delete copy.privacy.auto_export_dir;
    walk(copy, '');
    return out.join('\n');
  }

  function fromToml(text: string): Settings {
    const next = clone(settings) as unknown as Record<string, unknown>;
    let table: Record<string, unknown> = next;
    const seenArrays = new Set<string>();
    for (const rawLine of text.split('\n')) {
      const line = rawLine.trim();
      if (!line || line.startsWith('#')) continue;
      const arr = line.match(/^\[\[([\w.]+)\]\]$/);
      if (arr) {
        const parts = arr[1].split('.');
        const last = parts.pop()!;
        table = next;
        for (const part of parts) {
          if (typeof table[part] !== 'object' || table[part] === null) table[part] = {};
          table = table[part] as Record<string, unknown>;
        }
        if (!seenArrays.has(arr[1])) {
          seenArrays.add(arr[1]);
          table[last] = [];
        }
        const item: Record<string, unknown> = {};
        (table[last] as Record<string, unknown>[]).push(item);
        table = item;
        continue;
      }
      const sec = line.match(/^\[([\w.]+)\]$/);
      if (sec) {
        table = next;
        for (const part of sec[1].split('.')) {
          if (typeof table[part] !== 'object' || table[part] === null) table[part] = {};
          table = table[part] as Record<string, unknown>;
        }
        continue;
      }
      const kv = line.match(/^([\w-]+)\s*=\s*(.+)$/);
      if (!kv) throw `Ligne invalide : ${line}`;
      try {
        table[kv[1]] = JSON.parse(kv[2]);
      } catch {
        throw `Valeur invalide pour « ${kv[1]} ».`;
      }
    }
    return next as unknown as Settings;
  }

  // --- Contexte des réunions ---
  const contexts = new Map<string, MeetingContext>();
  let nextFileId = 1;
  const ctxOf = (id: string): MeetingContext => {
    find(id);
    if (!contexts.has(id)) contexts.set(id, { meeting_id: id, text: '', template: null, files: [] });
    return contexts.get(id)!;
  };
  const templatesList = (): ReportTemplate[] => [
    ...MOCK_BUILTIN_TEMPLATES.map((t) => ({ ...t, instructions: '', builtin: true })),
    ...(settings.report?.templates ?? []).map((t) => ({ id: t.id, name: t.name, description: t.instructions.slice(0, 90), instructions: t.instructions, builtin: false })),
  ];
  const KINDS: Record<string, string> = { pdf: 'pdf', xlsx: 'tableur', xls: 'tableur', ods: 'tableur', csv: 'csv', docx: 'document', doc: 'document', rtf: 'document', md: 'texte', txt: 'texte', json: 'texte' };

  // --- Préparation des réunions : calendrier, réunions prévues, participants ---
  const participants = new Map<string, Participant[]>();
  const eventLinks = new Map<string, string>();
  const at = (days: number, h: number, m = 0) => {
    const d = new Date();
    d.setDate(d.getDate() + days);
    d.setHours(h, m, 0, 0);
    return d;
  };
  const calendarEvents = (): Omit<UpcomingEvent, 'meeting_id'>[] => [
    {
      id: 'evt-revue-nova', title: 'Revue Nova — v2', start: iso(at(1, 10)), end: iso(at(1, 11)), all_day: false,
      location: 'Teams', url: null, notes: 'Ordre du jour : périmètre de la v2, date de sortie, risques.', calendar: 'Travail',
      attendees: [
        { name: 'Inès Martin', email: 'ines.martin@exemple.fr', status: 'accepted', organizer: true, me: false },
        { name: null, email: 'paul.durand@exemple.fr', status: 'pending', organizer: false, me: false },
        { name: 'Camille', email: 'camille@exemple.fr', status: 'accepted', organizer: false, me: true },
      ],
    },
    {
      id: 'evt-point-client', title: 'Point client Brindille', start: iso(at(2, 14, 30)), end: iso(at(2, 15)), all_day: false,
      location: null, url: null, notes: null, calendar: 'Travail',
      attendees: [{ name: 'Sophie Bernard', email: 'sophie@brindille.example', status: 'accepted', organizer: false, me: false }],
    },
  ];
  function planned(): Meeting[] {
    return meetings.filter((x) => x.meeting.status === 'planned').map((x) => clone(x.meeting))
      .sort((a, b) => a.started_at.localeCompare(b.started_at));
  }
  function planMeeting(title: string, scheduledAt: string, eventId?: string): Meeting {
    if (eventId && eventLinks.has(eventId)) return clone(find(eventLinks.get(eventId)!).meeting);
    const ev = calendarEvents().find((e) => e.id === eventId);
    const start = new Date(scheduledAt);
    if (Number.isNaN(start.getTime())) throw 'Date prévue invalide.';
    const meeting: Meeting = {
      id: `prevue-${start.getTime()}`, title: title || ev?.title || 'Réunion prévue', title_source: ev ? 'calendar' : 'user',
      started_at: iso(start), ended_at: null, status: 'planned', confidential: false,
      agent_provider: settings.agent.provider, correlation: settings.correlation.mode, call_app: null,
    };
    meetings.unshift({ meeting, segments: [], speakers: [{ label: 'Moi', display_name: null }], notes: [], requests: [], reports: [], refs: [], decisions: [], actions: [], drafts: [] });
    if (ev) {
      eventLinks.set(ev.id, meeting.id);
      participants.set(meeting.id, ev.attendees.filter((x) => !x.me && x.status !== 'declined').map((x) => ({
        name: x.name ?? x.email!.split('@')[0].split('.').map((p) => p[0].toUpperCase() + p.slice(1)).join(' '), email: x.email, source: 'calendar',
      })));
      if (ev.notes) ctxOf(meeting.id).text = `Invitation du calendrier :\n${ev.notes}`;
    }
    emit('steno://meetings', null);
    return clone(meeting);
  }

  // --- Mises à jour (version 0.2.0 publiée) ---
  const updateStatus: UpdateStatus = {
    current: '0.1.0', latest: '0.2.0', available: true, url: 'https://github.com/NakileSolutions/Steno/releases/tag/v0.2.0',
    notes: '## Nouveautés\n- Contexte et documents dans le compte rendu\n- Formats de sortie personnalisés', published_at: iso(new Date()),
    checked_at: iso(new Date()), state: 'idle', message: null, source_dir: '/Users/camille/dev/steno', can_install: true, log_path: null,
  };
  const emitUpdate = () => emit('steno://update', clone(updateStatus));

  // --- Commandes ---
  type Args = Record<string, unknown>;
  const handlers: Record<string, (a: Args) => unknown> = {
    live_state: () => clone(live),
    start_meeting: (a) => {
      if (liveStore()) throw 'Une réunion est déjà en cours.';
      const start = new Date();
      // Réunion prévue dans la fenêtre ±30 min : reprise (comme le cœur).
      const due = meetings.find((x) => x.meeting.status === 'planned' && Math.abs(new Date(x.meeting.started_at).getTime() - start.getTime()) <= 30 * 60_000);
      if (due) {
        due.meeting.status = 'live';
        due.meeting.started_at = iso(start);
        due.meeting.call_app = a.inPerson ? 'room' : live.call.active ? live.call.app : null;
        scriptPos = 0;
        live.state = 'listening';
        emitLive();
        emit('steno://meetings', null);
        return clone(due.meeting);
      }
      const id = `reunion-${start.getTime()}`;
      const meeting: Meeting = {
        id, title: (a.title as string) || (!a.inPerson && live.call.active && live.call.title) || (a.inPerson ? 'Réunion en présentiel' : 'Réunion'),
        title_source: a.title ? 'user' : !a.inPerson && live.call.active && live.call.title ? 'call_window' : null,
        started_at: iso(start), ended_at: null, status: 'live', confidential: false,
        agent_provider: settings.agent.provider, correlation: settings.correlation.mode,
        call_app: a.inPerson ? 'room' : live.call.active ? live.call.app : null,
      };
      meetings.unshift({ meeting, segments: [], speakers: [{ label: 'Moi', display_name: null }], notes: [], requests: [], reports: [], refs: [], decisions: [], actions: [], drafts: [] });
      scriptPos = 0;
      live.state = 'listening';
      emitLive();
      emit('steno://meetings', null);
      return clone(meeting);
    },
    stop_meeting: () => {
      const s = liveStore();
      if (!s) throw 'Aucune réunion en cours.';
      finishMeeting(s);
      return null;
    },
    set_mic_muted: (a) => {
      live.mic_muted = !!a.on;
      emitLive();
      return null;
    },
    set_aside: (a) => {
      live.aside = !!a.on;
      emitLive();
      return null;
    },
    set_live_confidential: (a) => {
      const s = liveStore();
      if (!s) throw 'Aucune réunion en cours.';
      s.meeting.confidential = !!a.on;
      emitLive();
      emit('steno://meetings', null);
      return null;
    },
    ask: (a) => createRequest(a.query as string, (a.meetingId as string) ?? null, 'text'),
    ask_history: (a) => createRequest(a.query as string, null, 'text', 'history'),
    cancel_request: (a) => {
      const r = allRequests().find((x) => x.id === a.id);
      if (r && (r.status === 'queued' || r.status === 'running')) {
        r.status = 'cancelled';
        r.finished_at = iso(new Date());
        emit('steno://request', r);
        emitLive();
      }
      return null;
    },
    recent_requests: (a) =>
      clone(allRequests().sort((x, y) => y.created_at.localeCompare(x.created_at)).slice(0, (a.limit as number) ?? 5)),
    list_meetings: (a) => {
      const f = (a.filter ?? {}) as MeetingFilter;
      let list = meetings.slice();
      if (f.query?.trim()) list = list.filter((s) => matches(s, f.query!.trim()));
      if (f.from) list = list.filter((s) => s.meeting.started_at.slice(0, 10) >= f.from!);
      if (f.to) list = list.filter((s) => s.meeting.started_at.slice(0, 10) <= f.to!);
      if (f.correlated !== undefined) list = list.filter((s) => !!s.reports.at(-1)?.correlated === f.correlated);
      if (f.openActions) list = list.filter((s) => s.actions.some((x) => x.status === 'open'));
      list.sort((x, y) => y.meeting.started_at.localeCompare(x.meeting.started_at));
      const off = f.offset ?? 0;
      return list.slice(off, off + (f.limit ?? 200)).map((s) => summary(s, f.query?.trim()));
    },
    get_meeting: (a) => {
      const s = find(a.id as string);
      const detail: MeetingDetail = {
        meeting: s.meeting, speakers: s.speakers, notes: s.notes, requests: s.requests,
        report: s.reports.at(-1) ?? null,
        report_versions: s.reports.map((r) => ({ version: r.version, generated_at: r.generated_at, correlated: r.correlated })),
        refs: s.refs, decisions: s.decisions, actions: s.actions.map(actionWithMeeting), drafts: s.drafts,
      };
      return clone(detail);
    },
    get_transcript: (a) => clone(find(a.id as string).segments.filter((x) => a.includePrivate || !x.private)),
    get_report_version: (a) => {
      const r = find(a.id as string).reports.find((x) => x.version === a.version);
      if (!r) throw 'Version introuvable.';
      return clone(r);
    },
    rename_meeting: (a) => {
      const s = find(a.id as string);
      s.meeting.title = String(a.title);
      s.meeting.title_source = 'user';
      emit('steno://meetings', null);
      emitLive();
      return null;
    },
    set_confidential: (a) => {
      find(a.id as string).meeting.confidential = !!a.on;
      emit('steno://meetings', null);
      return null;
    },
    delete_meeting: (a) => {
      find(a.id as string);
      meetings = meetings.filter((m) => m.meeting.id !== a.id);
      emit('steno://meetings', null);
      emitLive();
      return null;
    },
    rename_speaker: (a) => {
      const s = find(a.meetingId as string);
      const sp = s.speakers.find((x) => x.label === a.label);
      const name = String(a.displayName).trim() || null;
      if (sp) sp.display_name = name;
      else s.speakers.push({ label: String(a.label), display_name: name });
      return null;
    },
    add_note: (a) => {
      const s = find(a.meetingId as string);
      const n: Note = { id: nextIds.note(), meeting_id: s.meeting.id, ts: iso(new Date()), text: String(a.text), source: 'user' };
      s.notes.push(n);
      return clone(n);
    },
    update_note: (a) => {
      for (const s of meetings) {
        const n = s.notes.find((x) => x.id === a.id);
        if (n) n.text = String(a.text);
      }
      return null;
    },
    delete_note: (a) => {
      for (const s of meetings) s.notes = s.notes.filter((x) => x.id !== a.id);
      return null;
    },
    regenerate_report: (a) => {
      const s = find(a.id as string);
      if (s.meeting.confidential) throw 'Réunion confidentielle : aucun compte rendu n’est généré.';
      const prevStatus = s.meeting.status;
      s.meeting.status = 'processing';
      emit('steno://meetings', null);
      setTimeout(() => {
        const last = s.reports.at(-1);
        const md = last ? last.markdown.split('\n## Liens avec le code')[0] : `# ${s.meeting.title}\n\n## En bref\n- Compte rendu régénéré.`;
        s.reports.push({ meeting_id: s.meeting.id, markdown: md, generated_at: iso(new Date()), provider: settings.agent.provider, model: settings.agent.models.summary || null, correlated: false, version: s.reports.length + 1 });
        s.meeting.status = prevStatus === 'processing' ? 'done' : prevStatus === 'ended' ? 'done' : prevStatus;
        emit('steno://report', { meeting_id: s.meeting.id, version: s.reports.length });
        emit('steno://meetings', null);
      }, 2500);
      return null;
    },
    correlate: (a) => {
      const s = find(a.id as string);
      if (s.meeting.confidential) throw 'Réunion confidentielle : corrélation impossible.';
      if (s.meeting.correlation === 'off') throw 'Corrélation désactivée pour cette réunion.';
      const last = s.reports.at(-1);
      if (!last) throw 'Aucun compte rendu à corréler.';
      setTimeout(() => {
        const base = last.markdown.split('\n## Liens avec le code')[0];
        s.reports.push({ ...last, markdown: `${base}\n\n## Liens avec le code et les outils\n- Rien de contradictoire trouvé dans les dépôts cochés.\n- Tickets cités vérifiés dans Jira.`, generated_at: iso(new Date()), correlated: true, version: s.reports.length + 1 });
        emit('steno://report', { meeting_id: s.meeting.id, version: s.reports.length });
        emit('steno://meetings', null);
      }, 3000);
      return null;
    },
    export_meeting: (a) => {
      const s = find(a.id as string);
      const report = s.reports.at(-1)?.markdown ?? `# ${s.meeting.title ?? s.meeting.id}`;
      const segs = s.segments.filter((x) => !x.private && !x.is_echo);
      const who = (l: string) => s.speakers.find((p) => p.label === l)?.display_name ?? l;
      const st = (t: number, sep: string) => {
        const ms = Math.round(Math.max(0, t) * 1000);
        const p = (n: number, w = 2) => String(n).padStart(w, '0');
        return `${p(Math.floor(ms / 3600000))}:${p(Math.floor(ms / 60000) % 60)}:${p(Math.floor(ms / 1000) % 60)}${sep}${p(ms % 1000, 3)}`;
      };
      if (a.format === 'txt') return `${s.meeting.title ?? 'Réunion'}\n\n${segs.map((x) => `[${x.ts.slice(11, 19)}] ${who(x.speaker)} : ${x.text}`).join('\n')}\n`;
      if (a.format === 'srt') return segs.map((x, i) => `${i + 1}\n${st(x.t0 ?? 0, ',')} --> ${st(x.t1 ?? (x.t0 ?? 0) + 2, ',')}\n${who(x.speaker)} : ${x.text}\n`).join('\n');
      if (a.format === 'vtt') return `WEBVTT\n\n${segs.map((x) => `${st(x.t0 ?? 0, '.')} --> ${st(x.t1 ?? (x.t0 ?? 0) + 2, '.')}\n<v ${who(x.speaker)}>${x.text}\n`).join('\n')}`;
      const tr = segs.map((x) => `- **${who(x.speaker)}** (${x.ts.slice(11, 19)}) : ${x.text}`);
      return `${report}\n\n## Transcription\n\n${tr.join('\n')}\n`;
    },
    save_export: () => null,
    print_window: () => null,
    open_main: () => null,
    pause_meeting: () => {
      if (live.state !== 'listening') throw 'Aucune écoute en cours.';
      live.state = 'paused';
      emitLive();
      return null;
    },
    resume_meeting: () => {
      if (live.state !== 'paused') throw 'L’écoute n’est pas en pause.';
      live.state = 'listening';
      emitLive();
      return null;
    },
    search: (a) => {
      const q = String(a.query ?? '').trim();
      if (!q) return [];
      const fq = fold(q);
      const hits: SearchHit[] = [];
      for (const s of meetings) {
        for (const seg of s.segments) {
          if (!seg.private && !seg.is_echo && fold(seg.text).includes(fq))
            hits.push({ meeting_id: s.meeting.id, title: s.meeting.title, started_at: s.meeting.started_at, kind: 'segment', snippet: seg.text, ts: seg.ts });
        }
        const md = s.reports.at(-1)?.markdown;
        if (md) {
          for (const line of md.split('\n')) {
            if (fold(line).includes(fq) && !line.startsWith('# '))
              hits.push({ meeting_id: s.meeting.id, title: s.meeting.title, started_at: s.meeting.started_at, kind: 'report', snippet: line.replace(/^[#\-|*\s]+/, '').replace(/[`*]/g, '').replace(/\s*\|\s*/g, ' · ').replace(/ · $/, ''), ts: null });
          }
        }
      }
      return hits.slice(0, 100);
    },
    list_actions: (a) => {
      const f = (a.filter ?? {}) as ActionFilter;
      return meetings
        .flatMap((s) => s.actions.map(actionWithMeeting))
        .filter((x) => (!f.status || x.status === f.status) && (!f.owner || x.owner === f.owner))
        .filter((x) => (!f.from || (x.meeting_started_at ?? '').slice(0, 10) >= f.from) && (!f.to || (x.meeting_started_at ?? '').slice(0, 10) <= f.to))
        .sort((x, y) => (y.meeting_started_at ?? '').localeCompare(x.meeting_started_at ?? ''));
    },
    set_action_status: (a) => {
      for (const s of meetings) {
        const x = s.actions.find((y) => y.id === a.id);
        if (x) x.status = a.status as Action['status'];
      }
      emit('steno://meetings', null);
      return null;
    },
    list_decisions: (a) =>
      clone(meetings.flatMap((s) => s.decisions).filter((d) => !a.since || d.created_at >= (a.since as string)).sort((x, y) => y.created_at.localeCompare(x.created_at))),
    get_settings: () => clone(settings),
    save_settings: (a) => {
      settings = clone(a.settings as Settings);
      // Comme le cœur : formats perso nommés, identifiant unique dérivé du nom.
      if (settings.report) {
        const ids = new Set(MOCK_BUILTIN_TEMPLATES.map((t) => t.id));
        settings.report.templates = settings.report.templates
          .filter((t) => t.name.trim())
          .map((t) => {
            let id = t.id && !MOCK_BUILTIN_TEMPLATES.some((b) => b.id === t.id) ? t.id : `perso-${fold(t.name.trim()).replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')}`;
            const base = id;
            for (let n = 2; ids.has(id); n++) id = `${base}-${n}`;
            ids.add(id);
            return { ...t, id, name: t.name.trim() };
          });
      }
      storeSettings(settings);
      forgetDisabledCall();
      emit('steno://settings', clone(settings));
      return clone(settings);
    },
    export_settings: () => toToml(settings),
    import_settings: (a) => {
      settings = fromToml(String(a.toml));
      storeSettings(settings);
      forgetDisabledCall();
      emit('steno://settings', clone(settings));
      return clone(settings);
    },
    detect_agents: () => clone(agents),
    list_audio_devices: () => ({
      inputs: audioDevices.inputs.filter((d) => !unplugged.has(d.id)),
      outputs: audioDevices.outputs.filter((d) => !unplugged.has(d.id)),
    }),
    list_models: async (a) => {
      await sleep(250);
      return seedModels(String(a.provider));
    },
    test_agent: async (a) => {
      await sleep(1200);
      const ag = agents.find((x) => x.id === a.provider);
      if (!ag?.installed) return { ok: false, message: `${ag?.name ?? a.provider} n'est pas installé.`, duration_ms: 40 };
      const effort = typeof a.effort === 'string' ? a.effort : '';
      if (effort && !ag.efforts.includes(effort)) return { ok: false, message: `${ag.name} n'accepte pas le niveau de réflexion « ${effort} ».`, duration_ms: 60 };
      const extra = [typeof a.model === 'string' && a.model ? `modèle ${a.model}` : '', effort ? `réflexion ${effortLabel(effort).toLowerCase()}` : ''].filter(Boolean).join(', ');
      return { ok: true, message: `${ag.name} ${ag.version ?? ''} a répondu « OK » en mode non interactif${extra ? ` (${extra})` : ''}. Écriture bloquée : vérifié.`.replace('  ', ' '), duration_ms: 1184 };
    },
    list_connectors: () => clone(connectors),
    refresh_connectors: async () => {
      await sleep(900);
      connectors = connectors.map((c) => ({ ...c, detected_at: iso(new Date()) }));
      return clone(connectors);
    },
    update_connector: (a) => {
      const c = connectors.find((x) => x.id === a.id);
      if (!c) throw 'Connecteur introuvable.';
      if (!c.write_blockable && (a.useLive || a.useCorrelation))
        throw 'Ce connecteur ne peut pas être activé : l’agent ne sait pas bloquer ses outils d’écriture.';
      c.use_live = !!a.useLive;
      c.use_correlation = !!a.useCorrelation;
      return clone(c);
    },
    list_repos: () => clone(repos),
    add_repo: (a) => {
      const path = String(a.path);
      if (repos.some((r) => r.path === path)) throw 'Ce dossier est déjà dans la liste.';
      const r: Repo = { id: Math.max(0, ...repos.map((x) => x.id)) + 1, label: (a.label as string) || path.split('/').filter(Boolean).at(-1) || path, path, use_live: true, use_correlation: true, exclude_globs: '' };
      repos.push(r);
      return clone(r);
    },
    update_repo: (a) => {
      const r = a.repo as Repo;
      repos = repos.map((x) => (x.id === r.id ? clone(r) : x));
      return clone(r);
    },
    remove_repo: (a) => {
      repos = repos.filter((x) => x.id !== a.id);
      return null;
    },
    pick_folder: async () => {
      await sleep(300);
      const n = ['ms-stations', 'front-portail', 'lib-commons', 'ms-notify'];
      return `/Users/camille/dev/orion/${n[repos.length % n.length]}`;
    },
    check_wake_word: (a) => {
      const word = String(a.word ?? '').trim();
      const res: WakeCheck = { warnings: [], errors: [], syllables: syllables(word), history_count: 0, meetings_scanned: Math.min(20, meetings.length) };
      if (!word) {
        res.errors.push('Le mot d’appel ne peut pas être vide.');
        return res;
      }
      if (/\s/.test(word) && word.split(/\s+/).length > 2) res.errors.push('Deux mots au maximum.');
      if (res.syllables < 2 && !a.strict) res.warnings.push('Moins de 2 syllabes : risque de faux déclenchements. Le mode strict réduit ce risque.');
      const common = ['bonjour', 'merci', 'alors', 'donc', 'ticket', 'service', 'point'];
      if (common.includes(fold(word))) res.warnings.push('Mot très courant en français : risque élevé de faux déclenchements.');
      const fw = fold(word);
      res.history_count = meetings.reduce((n, s) => n + s.segments.filter((x) => !x.request_id && fold(x.text).includes(fw)).length, 0);
      if (res.history_count > 0)
        res.warnings.push(`Ce mot apparaît ${res.history_count} fois dans vos ${res.meetings_scanned} dernières réunions sans être une commande.`);
      const names = meetings.flatMap((s) => s.speakers.map((x) => x.display_name).filter(Boolean)) as string[];
      if (names.some((n) => fold(n) === fw)) res.warnings.push('C’est le prénom d’un collègue repéré dans vos transcriptions.');
      return res;
    },
    generate_variants: (a) => {
      const w = fold(String(a.word ?? '').trim());
      if (!w) return { accepted: [], rejected: [] };
      const cands = new Set<string>([w, w.replace(/e$/, ''), w.replace(/c/g, 'k'), w + 's', w.replace(/i/g, 'y')]);
      if (w === 'steno') ['stenos', 'esteno', 'stenau', 'stenot', 'creneau', 'scenario', 'stylo'].forEach((x) => cands.add(x));
      return checkVariants(w, [...cands]);
    },
    check_variants: (a) => checkVariants(String(a.word), (a.variants as string[]) ?? []),
    wake_detect: (a) => wakeDetect(String(a.text ?? '')),
    wake_test_record: async (a) => {
      const secs = Number(a.seconds) || 5;
      await sleep(Math.min(secs, 20) * 120 + 600);
      const w = settings.voice.wake_word;
      if (secs >= 10) {
        return { text: 'Le sténographe a noté le scénario du créneau de jeudi. Stéphane a prêté son stylo au ténor de la chorale. Il faudra rester sur le même tempo et étendre le délai. Tout est noté sur le tableau.' };
      }
      wakeTestCount++;
      const miss = wakeTestCount % 5 === 3;
      return { text: miss ? 'Scénario, cherche quelque chose.' : `${w}, cherche quelque chose.` };
    },
    doctor: async () => {
      await sleep(700);
      const system: DoctorItem[] = isLinux
        ? [
            { name: 'Système', status: 'ok', message: 'Ubuntu 24.04.1 LTS', fix: null },
            { name: 'Serveur son', status: 'ok', message: 'PulseAudio (on PipeWire 1.0.5)', fix: null },
            {
              name: 'Session',
              status: 'warn',
              message: 'Wayland : les raccourcis globaux de Sténo sont indisponibles',
              fix: 'Paramètres → Clavier → Raccourcis personnalisés : « /usr/bin/steno-app --toggle » (démarrer/arrêter l’écoute), « /usr/bin/steno-app --aside » (aparté), « /usr/bin/steno-app --note » (note rapide).',
            },
          ]
        : [
            { name: 'Microphone', status: 'ok', message: 'Autorisé.', fix: null },
            { name: 'Audio du système', status: 'warn', message: 'Pas encore demandé.', fix: 'Démarrez une réunion : macOS affichera la demande « Enregistrement audio du système ».' },
            { name: 'Accessibilité', status: 'error', message: 'Refusée : le titre des appels (Teams, Slack, Zoom…) ne sera pas lu.', fix: 'Réglages Système → Confidentialité et sécurité → Accessibilité → cochez Sténo.' },
          ];
      const items: DoctorItem[] = [
        ...system,
        { name: 'Apps de visio', status: 'ok', message: `Surveillées : ${settings.audio.call_apps.filter((x) => x.enabled).map((x) => x.name).join(', ') || 'aucune'}.`, fix: null },
        { name: 'Sidecar audio', status: 'ok', message: 'steno-audio 0.1.0, modèle de transcription présent.', fix: null },
        { name: 'Agent IA', status: 'ok', message: `${agents.find((x) => x.id === settings.agent.provider)?.name ?? settings.agent.provider} détecté.`, fix: null },
        { name: 'Politique lecture seule', status: 'ok', message: 'Test « poste sur Slack » : refusé comme attendu.', fix: null },
      ];
      return items;
    },
    get_permissions: () =>
      isLinux
        ? {
            ...permissions,
            microphone: 'granted',
            system_audio: 'granted',
            available: true,
            platform: 'linux',
            sound_server: 'PulseAudio (on PipeWire 1.0.5)',
            session: 'wayland',
            global_shortcuts: false,
            app_command: '/usr/bin/steno-app',
          }
        : { ...permissions, available: true, platform: 'macos' },
    request_permission: (a) => {
      const k = String(a.kind) as PermissionKind;
      if (!(k in permissions)) throw `Permission inconnue : ${k}`;
      if (permissions[k] === 'unknown') setTimeout(() => (permissions[k] = 'granted'), 600);
      return null;
    },
    open_permission_settings: (a) => {
      const k = String(a.kind) as PermissionKind;
      if (!(k in permissions)) throw `Permission inconnue : ${k}`;
      // Simule l'utilisateur qui coche Sténo dans les Réglages Système.
      setTimeout(() => (permissions[k] = 'granted'), 1200);
      return null;
    },
    complete_onboarding: () => null,
    get_update_status: () => clone(updateStatus),
    upcoming: () => ({
      planned: planned(),
      events: permissions.calendar === 'granted' ? calendarEvents().map((e) => ({ ...e, meeting_id: eventLinks.get(e.id) ?? null })) : [],
      calendar: permissions.calendar,
    }),
    plan_meeting: (a) => planMeeting(String(a.title ?? ''), String(a.scheduledAt), (a.eventId as string) || undefined),
    start_planned: (a) => {
      const s = find(String(a.id));
      if (s.meeting.status !== 'planned') throw 'Cette réunion n’est pas une réunion prévue.';
      s.meeting.started_at = iso(new Date());
      return handlers.start_meeting({ inPerson: !!a.inPerson });
    },
    get_participants: (a) => clone(participants.get(String(a.id)) ?? []),
    set_participants: (a) => {
      const list = (a.participants as Participant[]).filter((p) => p.name.trim());
      participants.set(String(a.id), list);
      return clone(list);
    },
    prepare_questions: () => [
      'Quel est l’objectif principal de cette réunion ?',
      'Quelles décisions doivent être prises ?',
      'Y a-t-il des chiffres ou des dates à surveiller ?',
    ],
    prepare_apply: (a) => {
      const c = ctxOf(String(a.id));
      const lines = (a.answers as { question: string; answer: string }[]).filter((x) => x.answer.trim()).map((x) => `- ${x.answer.trim()}`);
      if (!lines.length) throw 'Aucune réponse.';
      c.text = [c.text.trim(), `Préparation\n${lines.join('\n')}`].filter(Boolean).join('\n\n');
      return clone(c);
    },
    check_update: () => {
      updateStatus.checked_at = iso(new Date());
      emitUpdate();
      return clone(updateStatus);
    },
    install_update: () => {
      if (liveStore()) throw 'Une réunion est en cours : installez la mise à jour après la réunion.';
      if (!updateStatus.available) throw 'Aucune nouvelle version à installer.';
      updateStatus.state = 'installing';
      updateStatus.message = `Installation de Sténo ${updateStatus.latest}… (quelques minutes)`;
      emitUpdate();
      setTimeout(() => {
        updateStatus.state = 'installed';
        updateStatus.message = `Sténo ${updateStatus.latest} est installé : redémarrez pour l'utiliser.`;
        emitUpdate();
      }, 1500);
      return null;
    },
    restart_app: () => {
      Object.assign(updateStatus, { current: updateStatus.latest, available: false, state: 'idle', message: null });
      emitUpdate();
      return null;
    },
    get_meeting_context: (a) => {
      const p = pendingReports.get(String(a.id));
      return { ...clone(ctxOf(String(a.id))), report_at: p && p.at > Date.now() + 1000 ? new Date(p.at).toISOString() : null };
    },
    report_now: (a) => {
      const store = find(String(a.id));
      if (pendingReports.has(store.meeting.id)) scheduleReport(store, Date.now() + 800);
      emit('steno://meetings', null);
      return null;
    },
    set_meeting_context: (a) => {
      const c = ctxOf(String(a.id));
      const t = (a.template as string | null) || null;
      if (t && !templatesList().some((x) => x.id === t)) throw `Format de compte rendu inconnu : ${t}`;
      c.text = String(a.text ?? '').trim();
      c.template = t;
      contextTouched(c.meeting_id);
      return handlers.get_meeting_context({ id: c.meeting_id });
    },
    add_context_file: (a) => {
      const c = ctxOf(String(a.id));
      const name = String(a.name);
      const kind = KINDS[name.split('.').pop()?.toLowerCase() ?? ''];
      if (!kind) throw `Format non pris en charge : « ${name} » (PDF, Excel, CSV, Word, Markdown ou texte)`;
      const size = Math.floor((String(a.data).length * 3) / 4);
      const f: ContextFile = { id: nextFileId++, meeting_id: c.meeting_id, name, kind, size, chars: Math.min(size, 60000), truncated: size > 60000, added_at: iso(new Date()) };
      c.files.push(f);
      contextTouched(c.meeting_id);
      return clone(f);
    },
    delete_context_file: (a) => {
      for (const c of contexts.values()) c.files = c.files.filter((f) => f.id !== Number(a.fileId));
      return null;
    },
    list_report_templates: () => templatesList(),
    steno_skill_status: () => clone(skillStatus),
    install_steno_skill: () => {
      for (const t of skillStatus.targets) t.state = 'current';
      for (const m of skillStatus.mcp) m.registered = true;
      return clone(skillStatus);
    },
    template_from_example: (a) => {
      if (String(a.example ?? '').trim().length < 40) throw 'Exemple trop court : collez un compte rendu complet.';
      return {
        name: 'Note de synthèse',
        instructions:
          'Commence par « ## En bref » (3 puces). Puis « ## Décisions » et « ## Actions » (`**Qui** — quoi — échéance`). Ton direct, 20 lignes au plus.',
      };
    },
    delete_segment: (a) => {
      const id = Number(a.id);
      const s = meetings.find((x) => x.segments.some((g) => g.id === id));
      if (!s) throw 'Phrase introuvable.';
      s.segments = s.segments.filter((g) => g.id !== id);
      emit('steno://segment_deleted', { meeting_id: s.meeting.id, id });
      return null;
    },
    delete_all_data: () => {
      meetings = [];
      historyRequests = [];
      repos = [];
      live.state = 'idle';
      live.meeting = null;
      emitLive();
      emit('steno://meetings', null);
      return null;
    },
  };

  const backend: Backend = {
    async invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
      await sleep(25 + Math.random() * 40);
      const h = handlers[cmd];
      if (!h) throw `Commande inconnue du faux backend : ${cmd}`;
      return (await h(args)) as T;
    },
    async listen(name, cb): Promise<Unlisten> {
      let set = listeners.get(name);
      if (!set) listeners.set(name, (set = new Set()));
      const fn = cb as (p: unknown) => void;
      set.add(fn);
      return () => set!.delete(fn);
    },
    async openMain(route) {
      if (route) location.hash = '#' + route;
      else if (location.hash.startsWith('#/tray')) location.hash = '#/meetings';
    },
    async hideCurrent() {},
    async openExternal(url) {
      console.info('[mock] lien externe ignoré :', url);
    },
  };

  // Exposé pour le débogage dans la console du navigateur (mode faux backend uniquement).
  const mockApi = {
    emit,
    live,
    get meetings() { return meetings; },
    /** Simule une phrase entendue (par défaut sur le micro) pendant l'écoute. */
    say(text: string, speaker = 'Moi') {
      const store = liveStore();
      if (!store || live.state !== 'listening') return false;
      hear(store, speaker, text);
      return true;
    },
    /**
     * Simule le début d'un appel dans une app de visio (`slack`, `zoom`…), ou sa fin avec
     * `app = null`. Aucun effet sur l'écoute ; une app désactivée dans les réglages est ignorée.
     */
    call(app: string | null, title: string | null = null): boolean {
      if (app && !settings.audio.call_apps.some((a) => a.id === app && a.enabled)) return false;
      live.call = app ? { active: true, app, app_name: appName(app), title } : { active: false, app: null, app_name: null, title: null };
      emitLive();
      return true;
    },
    /** Simule le débranchement (ou le rebranchement avec `false`) d'un périphérique audio. */
    unplug(id: string, out = true): void {
      if (out) unplugged.add(id);
      else unplugged.delete(id);
    },
    /** Raccourci : appel Teams (ou fin d'appel avec `false`). */
    teams(active: boolean, title: string | null = TEAMS_TITLE): boolean {
      return mockApi.call(active ? 'teams' : null, active ? title : null);
    },
  };
  (window as unknown as { __stenoMock?: unknown }).__stenoMock = mockApi;
  return backend;
}
