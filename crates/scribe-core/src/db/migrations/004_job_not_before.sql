-- Scribe : une tâche peut attendre une heure de départ (UTC, RFC 3339). Sert au délai laissé
-- après la réunion pour ajouter du contexte avant la rédaction du compte rendu.
ALTER TABLE jobs ADD COLUMN not_before TEXT;
