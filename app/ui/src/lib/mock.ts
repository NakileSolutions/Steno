// Faux backend en mémoire : utilisé automatiquement hors de Tauri (développement, captures).
// Il imite les commandes et les événements de docs/contracts.md §5.

import type { Backend, Unlisten } from './api';
import type {
  Action, ActionFilter, DoctorItem, EventMap, EventName, LiveState, Meeting, MeetingDetail,
  MeetingFilter, MeetingSummary, Note, Repo, Request, SearchHit, Segment, Settings,
  VariantCheck, WakeCheck, WakeDetect,
} from './types';
import {
  LIVE_SCRIPT, TEAMS_TITLE, defaultSettings, iso, nextIds, seedAgents, seedConnectors,
  seedHistoryRequests, seedMeetings, seedRepos, type MeetingStore,
} from './mock-data';
import { detectWake, fold } from './wake';

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const clone = <T>(v: T): T => structuredClone(v);

const BANNED_VARIANTS: Record<string, string> = {
  script: 'Mot courant en réunion de dev (liste intégrée).',
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
  let settings: Settings = defaultSettings();
  let connectors = seedConnectors();
  let repos = seedRepos();
  const agents = seedAgents();
  let reqSeq = 100;
  let wakeTestCount = 0;

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
    emit('scribe://live', live);
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
    emit('scribe://live', live);
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
      req.error = 'Demande refusée : le Scribe ne publie rien à l’extérieur (lecture seule).';
      req.finished_at = iso(new Date());
      setTimeout(() => { emit('scribe://request', req); emitLive(); }, 50);
      return clone(req);
    }

    if (store?.meeting.confidential && it !== 'note') {
      req.status = 'refused';
      req.error = 'Réunion confidentielle : aucun appel à l’IA.';
      req.finished_at = iso(new Date());
      setTimeout(() => { emit('scribe://request', req); emitLive(); }, 50);
      return clone(req);
    }

    setTimeout(() => { emit('scribe://request', req); emitLive(); }, 50);
    setTimeout(() => {
      if (req.status !== 'queued') return;
      req.status = 'running';
      req.started_at = iso(new Date());
      emit('scribe://request', req);
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
        req.answer_md = `D'après vos réunions des 30 derniers jours :\n\n- **Daily ORION** (aujourd'hui) : pagination par 500 pour le batch de stations (APP-1251).\n- **Revue d'architecture** (hier) : export Brindille par fichier SFTP quotidien.\n\n_Recherche effectuée via le serveur MCP Scribe._`;
        req.summary = 'Deux réunions pertinentes trouvées.';
        req.tools_used = ['scribe.search_meetings', 'scribe.get_report'];
      } else if (/numéro de cadre/i.test(q)) {
        req.answer_md = `La validation est faite dans \`FrameNumberValidator.java:23\` par l'expression \`^\\d{5}$\` : **aucune règle** pour les vélos cargo, les anciens modèles (2A/2B) ni les cadres importés.\n\n- Les cadres importés (971 à 976) passent la regex mais ne sont pas contrôlés contre le référentiel.\n- Aucune table de référence référence dans le dépôt.\n\n**Sources**\n- [FrameNumberValidator.java:23](https://gitlab.example.com/orion/ms-booking/-/blob/main/src/main/java/fr/orion/booking/validation/FrameNumberValidator.java#L23)\n- [APP-1260 — Validation des numéros de cadre](https://jira.example.com/browse/APP-1260)`;
        req.summary = 'Regex ^\\d{5}$ dans FrameNumberValidator.java:23, sans vélos cargo ni anciens modèles.';
        req.tools_used = ['code.grep', 'code.read', 'jira.get_issue'];
      } else {
        req.answer_md = `Voici ce que j'ai trouvé pour « ${q} » :\n\n- Aucun résultat probant dans les dépôts cochés.\n- Deux tickets Jira proches : [APP-1251](https://jira.example.com/browse/APP-1251), [APP-1234](https://jira.example.com/browse/APP-1234).\n\n**Sources**\n- [Recherche Jira](https://jira.example.com/issues/?jql=text~%22${encodeURIComponent(q)}%22)`;
        req.summary = 'Deux tickets proches, rien dans le code.';
        req.tools_used = ['code.grep', 'jira.search'];
      }
      emit('scribe://request', req);
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
      // « Scribe, arrête l'écoute » : seule façon, avec le bouton et le raccourci, de terminer.
      const now = iso(new Date());
      const req: Request = {
        id: `req-${++reqSeq}`, meeting_id: store.meeting.id, created_at: now, origin: 'voice', intent: 'stop',
        query: text.replace(/^[^,]*,\s*/, '').replace(/[.!]+$/, ''), context_seconds: null, tier: 'quick',
        status: 'done', started_at: now, finished_at: now, error: null, summary: 'Écoute arrêtée.',
        answer_md: 'Écoute arrêtée à la voix. Le compte rendu est en préparation.', tools_used: [],
      };
      store.requests.push(req);
      seg.request_id = req.id;
      emit('scribe://segment', seg);
      emit('scribe://request', req);
      setTimeout(() => liveStore() === store && finishMeeting(store), 300);
      return;
    }
    if (det?.detected && det.query) {
      const req = createRequest(det.query, store.meeting.id, 'voice', det.intent as Request['intent']);
      seg.request_id = req.id;
    }
    emit('scribe://segment', seg);
  }

  const segTimer = () => {
    const store = liveStore();
    if (!store || live.state !== 'listening') return;
    const [speaker, text] = LIVE_SCRIPT[scriptPos % LIVE_SCRIPT.length];
    scriptPos++;
    hear(store, speaker, text);
  };
  setInterval(segTimer, 2800);

  function finishMeeting(store: MeetingStore) {
    store.meeting.status = 'processing';
    store.meeting.ended_at = iso(new Date());
    live.state = 'processing';
    live.meeting = clone(store.meeting);
    emitLive();
    emit('scribe://meetings', null);
    setTimeout(() => {
      store.meeting.status = store.meeting.confidential ? 'ended' : 'done';
      if (!store.meeting.confidential) {
        store.reports.push({
          meeting_id: store.meeting.id, version: store.reports.length + 1, correlated: false,
          generated_at: iso(new Date()), provider: settings.agent.provider, model: null,
          markdown: `# ${store.meeting.title ?? 'Réunion'}\n\n## En bref\n- ${store.segments.length} interventions transcrites.\n- Estimation des tickets APP-1255 et APP-1260.\n\n## Décisions\n- APP-1260 estimé à 3 points, avec la table de référence des modèles de vélos.\n\n## Actions — qui / quoi / échéance\n| Qui | Quoi | Échéance |\n|---|---|---|\n| Moi | Analyser APP-1262 (doublons de comptes) | cette semaine |`,
        });
        store.decisions.push({ id: nextIds.decision(), meeting_id: store.meeting.id, text: 'APP-1260 estimé à 3 points, avec la table de référence des modèles de vélos.', created_at: iso(new Date()) });
        store.actions.push({ id: nextIds.action(), meeting_id: store.meeting.id, owner: 'Moi', text: 'Analyser APP-1262 (doublons de comptes)', due: null, status: 'open' });
        emit('scribe://report', { meeting_id: store.meeting.id, version: store.reports.length });
      }
      live.state = 'idle';
      live.meeting = null;
      live.aside = false;
      emitLive();
      emit('scribe://meetings', null);
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
    const out: string[] = ['# Réglages Scribe — sans chemins ni données personnelles', ''];
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

  // --- Commandes ---
  type Args = Record<string, unknown>;
  const handlers: Record<string, (a: Args) => unknown> = {
    live_state: () => clone(live),
    start_meeting: (a) => {
      if (liveStore()) throw 'Une réunion est déjà en cours.';
      const start = new Date();
      const id = `reunion-${start.getTime()}`;
      const meeting: Meeting = {
        id, title: (a.title as string) || (live.call.active && live.call.title) || 'Réunion',
        title_source: a.title ? 'user' : live.call.active && live.call.title ? 'call_window' : null,
        started_at: iso(start), ended_at: null, status: 'live', confidential: false,
        agent_provider: settings.agent.provider, correlation: settings.correlation.mode,
        call_app: live.call.active ? live.call.app : null,
      };
      meetings.unshift({ meeting, segments: [], speakers: [{ label: 'Moi', display_name: null }], notes: [], requests: [], reports: [], refs: [], decisions: [], actions: [], drafts: [] });
      scriptPos = 0;
      live.state = 'listening';
      emitLive();
      emit('scribe://meetings', null);
      return clone(meeting);
    },
    stop_meeting: () => {
      const s = liveStore();
      if (!s) throw 'Aucune réunion en cours.';
      finishMeeting(s);
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
      emit('scribe://meetings', null);
      return null;
    },
    ask: (a) => createRequest(a.query as string, (a.meetingId as string) ?? null, 'text'),
    ask_history: (a) => createRequest(a.query as string, null, 'text', 'history'),
    cancel_request: (a) => {
      const r = allRequests().find((x) => x.id === a.id);
      if (r && (r.status === 'queued' || r.status === 'running')) {
        r.status = 'cancelled';
        r.finished_at = iso(new Date());
        emit('scribe://request', r);
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
      emit('scribe://meetings', null);
      emitLive();
      return null;
    },
    set_confidential: (a) => {
      find(a.id as string).meeting.confidential = !!a.on;
      emit('scribe://meetings', null);
      return null;
    },
    delete_meeting: (a) => {
      find(a.id as string);
      meetings = meetings.filter((m) => m.meeting.id !== a.id);
      emit('scribe://meetings', null);
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
      emit('scribe://meetings', null);
      setTimeout(() => {
        const last = s.reports.at(-1);
        const md = last ? last.markdown.split('\n## Liens avec le code')[0] : `# ${s.meeting.title}\n\n## En bref\n- Compte rendu régénéré.`;
        s.reports.push({ meeting_id: s.meeting.id, markdown: md, generated_at: iso(new Date()), provider: settings.agent.provider, model: settings.agent.models.summary || null, correlated: false, version: s.reports.length + 1 });
        s.meeting.status = prevStatus === 'processing' ? 'done' : prevStatus === 'ended' ? 'done' : prevStatus;
        emit('scribe://report', { meeting_id: s.meeting.id, version: s.reports.length });
        emit('scribe://meetings', null);
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
        emit('scribe://report', { meeting_id: s.meeting.id, version: s.reports.length });
        emit('scribe://meetings', null);
      }, 3000);
      return null;
    },
    export_meeting: (a) => {
      const s = find(a.id as string);
      const report = s.reports.at(-1)?.markdown ?? `# ${s.meeting.title ?? s.meeting.id}`;
      const tr = s.segments.filter((x) => !x.private && !x.is_echo).map((x) => `- **${s.speakers.find((p) => p.label === x.speaker)?.display_name ?? x.speaker}** (${x.ts.slice(11, 19)}) : ${x.text}`);
      return `${report}\n\n## Transcription\n\n${tr.join('\n')}\n`;
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
      emit('scribe://meetings', null);
      return null;
    },
    list_decisions: (a) =>
      clone(meetings.flatMap((s) => s.decisions).filter((d) => !a.since || d.created_at >= (a.since as string)).sort((x, y) => y.created_at.localeCompare(x.created_at))),
    get_settings: () => clone(settings),
    save_settings: (a) => {
      settings = clone(a.settings as Settings);
      forgetDisabledCall();
      return clone(settings);
    },
    export_settings: () => toToml(settings),
    import_settings: (a) => {
      settings = fromToml(String(a.toml));
      forgetDisabledCall();
      return clone(settings);
    },
    detect_agents: () => clone(agents),
    test_agent: async (a) => {
      await sleep(1200);
      const ag = agents.find((x) => x.id === a.provider);
      if (!ag?.installed) return { ok: false, message: `${ag?.name ?? a.provider} n'est pas installé.`, duration_ms: 40 };
      return { ok: true, message: `${ag.name} ${ag.version ?? ''} a répondu « OK » en mode non interactif. Écriture bloquée : vérifié.`.replace('  ', ' '), duration_ms: 1184 };
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
      if (w === 'scribe') ['script', 'inscrit', 'description', 'scribes', 'skribe', 'scrib'].forEach((x) => cands.add(x));
      return checkVariants(w, [...cands]);
    },
    check_variants: (a) => checkVariants(String(a.word), (a.variants as string[]) ?? []),
    wake_detect: (a) => wakeDetect(String(a.text ?? '')),
    wake_test_record: async (a) => {
      const secs = Number(a.seconds) || 5;
      await sleep(Math.min(secs, 20) * 120 + 600);
      const w = settings.voice.wake_word;
      if (secs >= 10) {
        return { text: "Le script de déploiement est décrit dans la description du ticket. Thomas s'est inscrit à la revue. Il faudra transcrire les échanges et prescrire un nouveau délai. Le scripteur a tout noté." };
      }
      wakeTestCount++;
      const miss = wakeTestCount % 5 === 3;
      return { text: miss ? 'Scrabe, cherche quelque chose.' : `${w}, cherche quelque chose.` };
    },
    doctor: async () => {
      await sleep(700);
      const items: DoctorItem[] = [
        { name: 'Microphone', status: 'ok', message: 'Autorisé.', fix: null },
        { name: 'Audio du système', status: 'warn', message: 'Pas encore demandé.', fix: 'Démarrez une réunion : macOS affichera la demande « Enregistrement audio du système ».' },
        { name: 'Accessibilité', status: 'error', message: 'Refusée : le titre des appels (Teams, Slack, Zoom…) ne sera pas lu.', fix: 'Réglages Système → Confidentialité et sécurité → Accessibilité → cochez Scribe.' },
        { name: 'Apps de visio', status: 'ok', message: `Surveillées : ${settings.audio.call_apps.filter((x) => x.enabled).map((x) => x.name).join(', ') || 'aucune'}.`, fix: null },
        { name: 'Sidecar audio', status: 'ok', message: 'scribe-audio 0.1.0, modèle de transcription présent.', fix: null },
        { name: 'Agent IA', status: 'ok', message: `${agents.find((x) => x.id === settings.agent.provider)?.name ?? settings.agent.provider} détecté.`, fix: null },
        { name: 'Politique lecture seule', status: 'ok', message: 'Test « poste sur Slack » : refusé comme attendu.', fix: null },
      ];
      return items;
    },
    delete_all_data: () => {
      meetings = [];
      historyRequests = [];
      repos = [];
      live.state = 'idle';
      live.meeting = null;
      emitLive();
      emit('scribe://meetings', null);
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
  (window as unknown as { __scribeMock?: unknown }).__scribeMock = {
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
    call(app: string | null, title: string | null = null) {
      if (app && !settings.audio.call_apps.some((a) => a.id === app && a.enabled)) return false;
      live.call = app ? { active: true, app, app_name: appName(app), title } : { active: false, app: null, app_name: null, title: null };
      emitLive();
      return true;
    },
    /** Raccourci : appel Teams (ou fin d'appel avec `false`). */
    teams(active: boolean, title: string | null = TEAMS_TITLE) {
      return this.call(active ? 'teams' : null, active ? title : null);
    },
  };
  return backend;
}
