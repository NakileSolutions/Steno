// Types de l'API de l'app — copie fidèle de docs/contracts.md §5 (et §4 pour Settings).

export type Channel = 'me' | 'them';

export type Meeting = {
  id: string;
  title: string | null;
  title_source: string | null;
  started_at: string;
  ended_at: string | null;
  status: 'live' | 'ended' | 'processing' | 'done' | 'error';
  confidential: boolean;
  agent_provider: string | null;
  correlation: 'off' | 'on_demand' | 'auto';
  /** Identifiant de l'app de l'appel (teams, slack…) ; `null` si l'écoute a démarré sans appel. */
  call_app: string | null;
};

export type MeetingSummary = Meeting & {
  correlated: boolean;
  open_actions: number;
  segment_count: number;
  snippet: string | null;
};

export type Segment = {
  id: number;
  meeting_id: string;
  seq: number;
  ts: string;
  t0: number | null;
  t1: number | null;
  channel: Channel;
  speaker: string;
  text: string;
  private: boolean;
  is_echo: boolean;
  request_id: string | null;
};

export type Speaker = { label: string; display_name: string | null };

export type Note = {
  id: number;
  meeting_id: string;
  ts: string;
  text: string;
  source: 'user' | 'voice';
};

export type RequestStatus = 'queued' | 'running' | 'done' | 'error' | 'cancelled' | 'refused';
export type Intent = 'search' | 'check' | 'note' | 'recap' | 'cancel' | 'stop' | 'history';
export type Tier = 'quick' | 'search' | 'summary';

export type Request = {
  id: string;
  meeting_id: string | null;
  created_at: string;
  origin: 'voice' | 'text' | 'hotkey' | 'cli';
  intent: Intent;
  query: string;
  context_seconds: number | null;
  tier: Tier;
  status: RequestStatus;
  started_at: string | null;
  finished_at: string | null;
  error: string | null;
  summary: string | null;
  answer_md: string | null;
  tools_used: string[];
};

export type Report = {
  meeting_id: string;
  markdown: string;
  generated_at: string;
  provider: string | null;
  model: string | null;
  correlated: boolean;
  version: number;
};

export type ReportVersion = { version: number; generated_at: string; correlated: boolean };

export type ReportRef = {
  id: number;
  kind: string;
  ref: string;
  title: string | null;
  url: string | null;
  verified: boolean;
  note: string | null;
};

export type Decision = { id: number; meeting_id: string; text: string; created_at: string };

export type Action = {
  id: number;
  meeting_id: string;
  owner: string | null;
  text: string;
  due: string | null;
  status: 'open' | 'done';
  meeting_title?: string | null;
  meeting_started_at?: string;
};

export type Draft = { id: number; kind: string; target: string | null; body: string; created_at: string };

export type MeetingDetail = {
  meeting: Meeting;
  speakers: Speaker[];
  notes: Note[];
  requests: Request[];
  report: Report | null;
  report_versions: ReportVersion[];
  refs: ReportRef[];
  decisions: Decision[];
  actions: Action[];
  drafts: Draft[];
};

export type SearchHit = {
  meeting_id: string;
  title: string | null;
  started_at: string;
  kind: 'segment' | 'report';
  snippet: string;
  ts: string | null;
};

export type LiveState = {
  state: 'idle' | 'listening' | 'paused' | 'processing';
  meeting: Meeting | null;
  levels: { me: number; them: number };
  aside: boolean;
  sidecar: 'running' | 'starting' | 'stopped' | 'missing' | 'error';
  last_error: string | null;
  pending_requests: number;
  /** Appel en cours dans une app de visio surveillée. N'entraîne jamais de démarrage automatique. */
  call: LiveCall;
};

export type LiveCall = { active: boolean; app: string | null; app_name: string | null; title: string | null };

export type AgentInfo = {
  id: string;
  name: string;
  support: 'supported' | 'experimental' | 'generic';
  installed: boolean;
  version: string | null;
  error: string | null;
  capabilities: {
    resume_session: boolean;
    stream_json: boolean;
    read_only_files: boolean;
    tool_deny_list: boolean;
    extra_dirs: boolean;
  };
  /** Niveaux de réflexion acceptés par le CLI (ex. low…max) ; vide = non réglable. */
  efforts: string[];
  /** `true` : `list_models` interroge le CLI ; `false` : suggestions et saisie libre. */
  lists_models: boolean;
};

/** Modèle proposé par `list_models` (la saisie libre reste possible). */
export type ModelInfo = { id: string; name: string; note: string | null };

export type Connector = {
  id: number;
  provider: string;
  name: string;
  status: string;
  detected_at: string;
  use_live: boolean;
  use_correlation: boolean;
  write_blockable: boolean;
};

export type Repo = {
  id: number;
  label: string;
  path: string;
  use_live: boolean;
  use_correlation: boolean;
  exclude_globs: string;
};

export type WakeCheck = {
  warnings: string[];
  errors: string[];
  syllables: number;
  history_count: number;
  meetings_scanned: number;
};

export type VariantCheck = { accepted: string[]; rejected: { variant: string; reason: string }[] };

export type WakeDetect = { detected: boolean; intent: string | null; query: string | null };

export type DoctorItem = {
  name: string;
  status: 'ok' | 'warn' | 'error';
  message: string;
  fix: string | null;
};

export type TestAgentResult = { ok: boolean; message: string; duration_ms: number };

// --- Réglages (contracts.md §4) ---

export type VoiceVerbs = {
  search: string[];
  check: string[];
  note: string[];
  recap: string[];
  cancel: string[];
  stop: string[];
};

/** App de visio surveillée (réglage `audio.call_apps`). */
export type CallApp = {
  id: string;
  name: string;
  /** Préfixes d'identifiant de bundle macOS (auxiliaires compris). */
  bundles: string[];
  enabled: boolean;
  /** Navigateur : tout le son du navigateur est capté, autres onglets compris. */
  browser: boolean;
};

/** Choix pour un usage (contracts.md §4 `agent.tiers`) ; chaînes vides = défaut. */
export type AgentTier = {
  /** Vide = agent par défaut (`agent.provider`). */
  agent: string;
  /** Vide = modèle par défaut du CLI ; texte libre accepté. */
  model: string;
  /** Niveau de réflexion ; vide = défaut du CLI. */
  effort: string;
};
export type AgentTiers = { search: AgentTier; summary: AgentTier; quick: AgentTier };

export type Settings = {
  agent: {
    provider: string;
    /** Choix par usage : recherches, compte rendu, notes et récapitulatifs. */
    tiers: AgentTiers;
    /** Ancien format (modèle par niveau pour l'agent par défaut) : n'est plus affiché, conservé tel quel. */
    models: { quick: string; search: string; summary: string };
    custom_command: string;
    allow_api_key: boolean;
    max_parallel: number;
    timeouts_s: { quick: number; search: number; summary: number };
    live_tool_budget: number;
    live_context_seconds: number;
  };
  voice: {
    wake_word: string;
    strict: boolean;
    prefixes: string[];
    variants: string[];
    verbs: VoiceVerbs;
    feedback: 'none' | 'system' | 'headset';
  };
  audio: {
    /** Proposer de démarrer l'écoute quand un appel commence dans une app surveillée (notification seulement). */
    auto_detect_calls: boolean;
    /** Apps de visio surveillées. */
    call_apps: CallApp[];
    silence_ms: number;
    vocabulary: string[];
    /** Obsolète : la fin d'un appel ne termine plus la réunion. Conservé tel quel s'il est présent. */
    end_grace_s?: number;
    shortcuts: { toggle: string; aside: string; quick_note: string };
  };
  correlation: { mode: 'off' | 'on_demand' | 'auto'; budget: number; exclude_words: string[] };
  privacy: {
    audio_retention_days: number;
    transcript_retention_days: number | null;
    auto_export_dir: string | null;
    excluded_titles: string[];
  };
  profile: { name: string; role: string; team: string; jira_key_pattern: string };
};

export type MeetingFilter = {
  query?: string;
  from?: string;
  to?: string;
  correlated?: boolean;
  openActions?: boolean;
  limit?: number;
  offset?: number;
};

export type ActionFilter = { status?: 'open' | 'done'; owner?: string; from?: string; to?: string };

// --- Événements ---

export type EventMap = {
  'scribe://live': LiveState;
  'scribe://segment': Segment;
  'scribe://request': Request;
  'scribe://report': { meeting_id: string; version: number };
  'scribe://meetings': null;
  'scribe://navigate': { route: string };
};
export type EventName = keyof EventMap;
