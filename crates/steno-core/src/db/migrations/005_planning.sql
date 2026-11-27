-- Sténo : réunions prévues (statut « planned », started_at = heure prévue), lien avec
-- l'événement du calendrier, et participants invités.
ALTER TABLE meeting_context ADD COLUMN event_id TEXT;
CREATE TABLE meeting_participants (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  email TEXT,
  source TEXT NOT NULL DEFAULT 'manual'
);
CREATE INDEX meeting_participants_meeting ON meeting_participants(meeting_id, id);
