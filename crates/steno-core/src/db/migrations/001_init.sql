-- Sténo : schéma initial (voir PLAN.md §3 et docs/contracts.md §1)

CREATE TABLE meetings (
  id TEXT PRIMARY KEY,
  title TEXT, title_source TEXT,
  started_at TEXT NOT NULL, ended_at TEXT,
  status TEXT NOT NULL,
  confidential INTEGER NOT NULL DEFAULT 0,
  agent_provider TEXT, agent_session_id TEXT,
  correlation TEXT
);
CREATE INDEX meetings_started ON meetings(started_at);

CREATE TABLE segments (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL, ts TEXT NOT NULL, t0 REAL, t1 REAL,
  channel TEXT NOT NULL,
  speaker TEXT NOT NULL,
  text TEXT NOT NULL,
  private INTEGER NOT NULL DEFAULT 0,
  is_echo INTEGER NOT NULL DEFAULT 0,
  request_id TEXT
);
CREATE INDEX segments_meeting ON segments(meeting_id, seq);

CREATE VIRTUAL TABLE segments_fts USING fts5(
  text, content='segments', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER segments_ai AFTER INSERT ON segments BEGIN
  INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER segments_ad AFTER DELETE ON segments BEGIN
  INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER segments_au AFTER UPDATE OF text ON segments BEGIN
  INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
  INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TABLE speakers (
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  label TEXT NOT NULL, display_name TEXT,
  PRIMARY KEY (meeting_id, label));

CREATE TABLE notes (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  ts TEXT, text TEXT, source TEXT);

CREATE TABLE requests (
  id TEXT PRIMARY KEY,
  meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
  created_at TEXT,
  origin TEXT, intent TEXT,
  query TEXT, context_seconds INTEGER, tier TEXT,
  status TEXT, started_at TEXT, finished_at TEXT, error TEXT,
  summary TEXT, answer_md TEXT, tools_used TEXT);
CREATE INDEX requests_meeting ON requests(meeting_id, created_at);

CREATE TABLE reports (
  meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
  markdown TEXT, generated_at TEXT,
  provider TEXT, model TEXT, correlated INTEGER DEFAULT 0, version INTEGER DEFAULT 1);

CREATE VIRTUAL TABLE reports_fts USING fts5(markdown, content='reports', content_rowid='rowid',
  tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER reports_ai AFTER INSERT ON reports BEGIN
  INSERT INTO reports_fts(rowid, markdown) VALUES (new.rowid, new.markdown);
END;
CREATE TRIGGER reports_ad AFTER DELETE ON reports BEGIN
  INSERT INTO reports_fts(reports_fts, rowid, markdown) VALUES ('delete', old.rowid, old.markdown);
END;
CREATE TRIGGER reports_au AFTER UPDATE ON reports BEGIN
  INSERT INTO reports_fts(reports_fts, rowid, markdown) VALUES ('delete', old.rowid, old.markdown);
  INSERT INTO reports_fts(rowid, markdown) VALUES (new.rowid, new.markdown);
END;

CREATE TABLE report_versions (
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  version INTEGER NOT NULL,
  markdown TEXT NOT NULL, generated_at TEXT, provider TEXT, model TEXT,
  correlated INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (meeting_id, version));

CREATE TABLE report_refs (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  kind TEXT, ref TEXT, title TEXT, url TEXT, verified INTEGER, note TEXT);

CREATE TABLE decisions (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  text TEXT, created_at TEXT);

CREATE TABLE actions (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  owner TEXT, text TEXT, due TEXT,
  status TEXT DEFAULT 'open');
CREATE INDEX actions_status ON actions(status);

CREATE TABLE drafts (
  id INTEGER PRIMARY KEY,
  meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
  kind TEXT, target TEXT, body TEXT, created_at TEXT);

CREATE TABLE jobs (
  id INTEGER PRIMARY KEY, type TEXT,
  meeting_id TEXT, payload TEXT, status TEXT, attempts INTEGER DEFAULT 0,
  created_at TEXT, started_at TEXT, finished_at TEXT, error TEXT);
CREATE INDEX jobs_status ON jobs(status, id);

CREATE TABLE repos (
  id INTEGER PRIMARY KEY, label TEXT, path TEXT UNIQUE,
  use_live INTEGER DEFAULT 1, use_correlation INTEGER DEFAULT 1, exclude_globs TEXT);

CREATE TABLE connectors (
  id INTEGER PRIMARY KEY, provider TEXT, name TEXT, status TEXT, detected_at TEXT,
  use_live INTEGER DEFAULT 0, use_correlation INTEGER DEFAULT 0,
  write_blockable INTEGER NOT NULL,
  UNIQUE (provider, name));

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);

CREATE TABLE runtime (key TEXT PRIMARY KEY, value TEXT, updated_at TEXT);
