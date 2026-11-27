-- Scribe : app de visio de l'appel (teams, slack, zoom…) pour chaque réunion.
ALTER TABLE meetings ADD COLUMN call_app TEXT;
