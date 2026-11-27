-- Scribe : contexte d'une réunion (texte libre, modèle de compte rendu) et documents joints.
CREATE TABLE meeting_context (
  meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
  text TEXT NOT NULL DEFAULT '',
  template TEXT,
  updated_at TEXT
);
CREATE TABLE context_files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  size INTEGER NOT NULL,
  text TEXT NOT NULL,
  truncated INTEGER NOT NULL DEFAULT 0,
  added_at TEXT NOT NULL
);
CREATE INDEX context_files_meeting ON context_files(meeting_id, id);
