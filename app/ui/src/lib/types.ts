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
  /** Micro coupé dans Sténo : vos paroles ne sont pas transcrites. */
  mic_muted: boolean;
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

/** Périphérique audio (UID Core Audio stable d'un branchement à l'autre). */
export type AudioDevice = {
  id: string;
  name: string;
  /** Périphérique par défaut du système. */
  default: boolean;
  transport: 'builtin' | 'usb' | 'bluetooth' | 'virtual' | 'aggregate' | 'other';
};
export type AudioDevices = { inputs: AudioDevice[]; outputs: AudioDevice[] };

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

/** Droits macOS demandés par Sténo. */
export type PermissionKind = 'microphone' | 'system_audio' | 'accessibility';
export type PermissionValue = 'granted' | 'denied' | 'unknown';
export type Permissions = Record<PermissionKind, PermissionValue> & {
  /** Capture audio démarrée : état à jour, demandes possibles. */
  available: boolean;
};

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
    /** UID de la sortie où jouer le signal ; `null` = sortie par défaut. */
    feedback_device: string | null;
  };
  audio: {
    /** Proposer de démarrer l'écoute quand un appel commence dans une app surveillée (notification seulement). */
    auto_detect_calls: boolean;
    /** UID du micro choisi ; `null` = micro par défaut du système. */
    input_device: string | null;
    /** Nom du micro au moment du choix (affiché s'il est débranché). */
    input_device_name: string | null;
    /** Apps de visio surveillées. */
    call_apps: CallApp[];
    silence_ms: number;
    vocabulary: string[];
    /** Obsolète : la fin d'un appel ne termine plus la réunion. Conservé tel quel s'il est présent. */
    end_grace_s?: number;
    shortcuts: { toggle: string; aside: string; quick_note: string };
    /** Retranscription de fin de réunion, langue imposée (`fr`…) ; `off` = transcription du direct. */
    final_transcription?: string;
  };
  correlation: { mode: 'off' | 'on_demand' | 'auto'; budget: number; exclude_words: string[] };
  privacy: {
    audio_retention_days: number;
    transcript_retention_days: number | null;
    auto_export_dir: string | null;
    excluded_titles: string[];
    /** Rappeler, au début de l'écoute, de prévenir les participants. */
    consent_reminder: boolean;
    /** Message d'information à coller dans le chat de l'appel. */
    consent_message: string;
  };
  profile: { name: string; role: string; team: string; jira_key_pattern: string; theme?: 'auto' | 'light' | 'dark'; language?: 'auto' | 'fr' | 'en' };
  /** Comptes rendus : format par défaut, contexte proposé à l'arrêt, formats perso (enregistrés sur ce Mac). */
  report: ReportSettings;
  /** Mises à jour : vérification (API GitHub publique) et installation automatique hors réunion. */
  updates?: { check: boolean; auto_install: boolean };
};

export type CustomTemplate = { id: string; name: string; instructions: string };
export type ReportSettings = { default_template: string; ask_context_on_stop: boolean; context_delay_s: number; templates: CustomTemplate[] };

/** Format de compte rendu : intégré (`builtin`) ou perso. */
export type ReportTemplate = { id: string; name: string; description: string; instructions: string; builtin: boolean };

export type ContextFile = {
  id: number;
  meeting_id: string;
  name: string;
  kind: 'pdf' | 'tableur' | 'csv' | 'document' | 'texte' | string;
  size: number;
  chars: number;
  truncated: boolean;
  added_at: string;
};

/** Contexte d'une réunion : texte libre, format choisi (`null` = défaut) et documents joints. */
export type MeetingContext = {
  meeting_id: string;
  text: string;
  template: string | null;
  files: ContextFile[];
  /** Heure (UTC) de départ de la rédaction si elle attend encore le délai de contexte. */
  report_at?: string | null;
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

/** État des mises à jour (version publiée, installation depuis les sources). */
export type UpdateStatus = {
  current: string;
  latest: string | null;
  available: boolean;
  url: string | null;
  notes: string | null;
  published_at: string | null;
  checked_at: string | null;
  state: 'idle' | 'checking' | 'installing' | 'installed' | 'error';
  message: string | null;
  source_dir: string | null;
  can_install: boolean;
  log_path: string | null;
};

export type ActionFilter = { status?: 'open' | 'done'; owner?: string; from?: string; to?: string };

// --- Événements ---

export type EventMap = {
  'steno://live': LiveState;
  'steno://segment': Segment;
  'steno://segment_deleted': { meeting_id: string; id: number };
  'steno://request': Request;
  'steno://report': { meeting_id: string; version: number };
  'steno://meetings': null;
  'steno://update': UpdateStatus;
  'steno://navigate': { route: string };
};
export type EventName = keyof EventMap;
