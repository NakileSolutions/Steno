//! Requêtes SQL. Toutes les méthodes sont synchrones et courtes.

use super::Db;
use crate::model::*;
use crate::now_iso;

/// Maintenant en UTC (RFC 3339, à la seconde) : comparable comme une chaîne.
pub fn utc_now() -> String {
    utc_in(0)
}

/// Dans `secs` secondes, en UTC (RFC 3339).
pub fn utc_in(secs: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(secs)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
use anyhow::Result;
use rusqlite::{params, OptionalExtension, Row};

fn b(v: i64) -> bool {
    v != 0
}

const MEETING_COLS: &str = "id, title, title_source, started_at, ended_at, status, confidential, \
     agent_provider, agent_session_id, correlation, call_app";

fn meeting_row(r: &Row) -> rusqlite::Result<Meeting> {
    Ok(Meeting {
        id: r.get(0)?,
        title: r.get(1)?,
        title_source: r.get(2)?,
        started_at: r.get(3)?,
        ended_at: r.get(4)?,
        status: r.get(5)?,
        confidential: b(r.get(6)?),
        agent_provider: r.get(7)?,
        agent_session_id: r.get(8)?,
        correlation: r.get(9)?,
        call_app: r.get(10)?,
    })
}

const SEGMENT_COLS: &str =
    "id, meeting_id, seq, ts, t0, t1, channel, speaker, text, private, is_echo, request_id";

fn segment_row(r: &Row) -> rusqlite::Result<Segment> {
    Ok(Segment {
        id: r.get(0)?,
        meeting_id: r.get(1)?,
        seq: r.get(2)?,
        ts: r.get(3)?,
        t0: r.get(4)?,
        t1: r.get(5)?,
        channel: r.get(6)?,
        speaker: r.get(7)?,
        text: r.get(8)?,
        private: b(r.get(9)?),
        is_echo: b(r.get(10)?),
        request_id: r.get(11)?,
    })
}

const REQUEST_COLS: &str = "id, meeting_id, created_at, origin, intent, query, context_seconds, tier, \
     status, started_at, finished_at, error, summary, answer_md, tools_used";

fn request_row(r: &Row) -> rusqlite::Result<Request> {
    let tools: Option<String> = r.get(14)?;
    Ok(Request {
        id: r.get(0)?,
        meeting_id: r.get(1)?,
        created_at: r.get(2)?,
        origin: r.get(3)?,
        intent: r.get(4)?,
        query: r.get(5)?,
        context_seconds: r.get(6)?,
        tier: r.get(7)?,
        status: r.get(8)?,
        started_at: r.get(9)?,
        finished_at: r.get(10)?,
        error: r.get(11)?,
        summary: r.get(12)?,
        answer_md: r.get(13)?,
        tools_used: tools
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default(),
    })
}

fn report_row(r: &Row) -> rusqlite::Result<Report> {
    Ok(Report {
        meeting_id: r.get(0)?,
        markdown: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
        generated_at: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        provider: r.get(3)?,
        model: r.get(4)?,
        correlated: b(r.get::<_, Option<i64>>(5)?.unwrap_or(0)),
        version: r.get::<_, Option<i64>>(6)?.unwrap_or(1),
    })
}

fn action_row(r: &Row) -> rusqlite::Result<Action> {
    Ok(Action {
        id: r.get(0)?,
        meeting_id: r.get(1)?,
        owner: r.get(2)?,
        text: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        due: r.get(4)?,
        status: r.get::<_, Option<String>>(5)?.unwrap_or_else(|| "open".into()),
        meeting_title: None,
        meeting_started_at: None,
    })
}

fn repo_row(r: &Row) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: r.get(0)?,
        label: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
        path: r.get(2)?,
        use_live: b(r.get(3)?),
        use_correlation: b(r.get(4)?),
        exclude_globs: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
    })
}

fn connector_row(r: &Row) -> rusqlite::Result<Connector> {
    Ok(Connector {
        id: r.get(0)?,
        provider: r.get(1)?,
        name: r.get(2)?,
        status: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        detected_at: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
        use_live: b(r.get(5)?),
        use_correlation: b(r.get(6)?),
        write_blockable: b(r.get(7)?),
    })
}

fn job_row(r: &Row) -> rusqlite::Result<Job> {
    let payload: Option<String> = r.get(3)?;
    Ok(Job {
        id: r.get(0)?,
        kind: r.get(1)?,
        meeting_id: r.get(2)?,
        payload: payload
            .and_then(|p| serde_json::from_str(&p).ok())
            .unwrap_or(serde_json::Value::Null),
        status: r.get(4)?,
        attempts: r.get(5)?,
        created_at: r.get(6)?,
        error: r.get(7)?,
    })
}

/// `runtime` key of the running core's heartbeat (its pid, rewritten every 5 s).
const HEARTBEAT: &str = "app_heartbeat";

/// A heartbeat less than 15 s old.
fn heartbeat_fresh(at: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(at)
        .map(|t| (chrono::Local::now().fixed_offset() - t).num_seconds() < 15)
        .unwrap_or(false)
}

/// Transforme une saisie libre en requête FTS5 sûre (préfixes, ET implicite).
pub fn fts_query(input: &str) -> Option<String> {
    let toks: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        // "-" and "_" are separators for unicode61: a token made only of them is an empty
        // phrase, and `""*` is an FTS5 syntax error.
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect();
    if toks.is_empty() {
        None
    } else {
        Some(toks.join(" "))
    }
}

/// Requête FTS5 « au moins un des mots significatifs » (questions en langage naturel).
pub fn fts_query_any(input: &str) -> Option<String> {
    const STOP: &[&str] = &[
        "les", "des", "une", "est", "que", "qui", "quoi", "quel", "quelle", "quels", "quelles", "pour", "sur", "dans",
        "avec", "par", "pas", "plus", "aux", "ont", "fait", "faire", "été", "etait", "decide", "décidé", "dit", "comment",
        "pourquoi", "quand", "où", "nous", "vous", "ils", "elle", "cette", "ces", "son", "ses", "leur", "the", "and",
    ];
    let toks: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .filter(|t| {
            t.chars().count() >= 3 && t.chars().any(char::is_alphanumeric) && !STOP.contains(&t.to_lowercase().as_str())
        })
        .map(|t| format!("\"{}\"*", t.replace('"', "")))
        .collect();
    if toks.is_empty() {
        None
    } else {
        Some(toks.join(" OR "))
    }
}

/// Segment à insérer (le `seq` est attribué par la base).
#[derive(Debug, Clone)]
pub struct NewSegment {
    pub meeting_id: String,
    pub ts: String,
    pub t0: Option<f64>,
    pub t1: Option<f64>,
    pub channel: String,
    pub speaker: String,
    pub text: String,
    pub private: bool,
    pub is_echo: bool,
}

impl Db {
    // ---------------------------------------------------------------- réunions

    pub fn insert_meeting(&self, m: &Meeting) -> Result<()> {
        self.with(|c| {
            c.execute(
                &format!("INSERT INTO meetings ({MEETING_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"),
                params![
                    m.id, m.title, m.title_source, m.started_at, m.ended_at, m.status,
                    m.confidential as i64, m.agent_provider, m.agent_session_id, m.correlation, m.call_app
                ],
            )
        })?;
        Ok(())
    }

    pub fn meeting_exists(&self, id: &str) -> Result<bool> {
        self.with(|c| {
            c.query_row("SELECT 1 FROM meetings WHERE id=?1", [id], |_| Ok(()))
                .optional()
                .map(|o| o.is_some())
        })
    }

    pub fn get_meeting(&self, id: &str) -> Result<Option<Meeting>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {MEETING_COLS} FROM meetings WHERE id=?1"),
                [id],
                meeting_row,
            )
            .optional()
        })
    }

    /// Réunion en cours (statut `live`), la plus récente.
    pub fn live_meeting(&self) -> Result<Option<Meeting>> {
        self.with(|c| {
            c.query_row(
                &format!(
                    "SELECT {MEETING_COLS} FROM meetings WHERE status='live' ORDER BY started_at DESC LIMIT 1"
                ),
                [],
                meeting_row,
            )
            .optional()
        })
    }

    pub fn list_meetings(&self, f: &MeetingFilter) -> Result<Vec<MeetingSummary>> {
        let limit = f.limit.unwrap_or(200).clamp(1, 5000);
        let offset = f.offset.unwrap_or(0).max(0);
        let fts = f.query.as_deref().and_then(fts_query);
        self.with(|c| {
            let mut sql = String::from(
                "SELECT m.id, m.title, m.title_source, m.started_at, m.ended_at, m.status, m.confidential, \
                 m.agent_provider, m.agent_session_id, m.correlation, m.call_app, \
                 COALESCE((SELECT correlated FROM reports r WHERE r.meeting_id=m.id),0), \
                 (SELECT COUNT(*) FROM actions a WHERE a.meeting_id=m.id AND a.status='open'), \
                 (SELECT COUNT(*) FROM segments s WHERE s.meeting_id=m.id AND s.is_echo=0), ",
            );
            let mut args: Vec<rusqlite::types::Value> = vec![];
            if fts.is_some() {
                // ?1 est utilisé plus bas ; l'extrait est calculé après coup (une requête par réunion
                // affichée, bornée par les identifiants de ses segments).
                args.push(fts.clone().unwrap().into());
            }
            sql.push_str("NULL AS snip ");
            sql.push_str("FROM meetings m WHERE 1=1 ");
            if fts.is_some() {
                sql.push_str(
                    "AND (m.id IN (SELECT s.meeting_id FROM segments_fts JOIN segments s ON s.id = segments_fts.rowid \
                       WHERE segments_fts MATCH ?1 AND s.private = 0) \
                     OR m.id IN (SELECT r.meeting_id FROM reports_fts JOIN reports r ON r.rowid = reports_fts.rowid \
                       WHERE reports_fts MATCH ?1) \
                     OR m.title LIKE '%' || ?2 || '%') ",
                );
                args.push(f.query.clone().unwrap_or_default().into());
            }
            if let Some(from) = &f.from {
                sql.push_str(&format!("AND m.started_at >= ?{} ", args.len() + 1));
                args.push(from.clone().into());
            }
            if let Some(to) = &f.to {
                sql.push_str(&format!("AND substr(m.started_at,1,10) <= ?{} ", args.len() + 1));
                args.push(to.clone().into());
            }
            if let Some(cor) = f.correlated {
                sql.push_str(if cor {
                    "AND EXISTS (SELECT 1 FROM reports r WHERE r.meeting_id=m.id AND r.correlated=1) "
                } else {
                    "AND NOT EXISTS (SELECT 1 FROM reports r WHERE r.meeting_id=m.id AND r.correlated=1) "
                });
            }
            if f.open_actions == Some(true) {
                sql.push_str("AND EXISTS (SELECT 1 FROM actions a WHERE a.meeting_id=m.id AND a.status='open') ");
            }
            sql.push_str(&format!("ORDER BY m.started_at DESC LIMIT {limit} OFFSET {offset}"));
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(MeetingSummary {
                    meeting: meeting_row(r)?,
                    correlated: b(r.get(11)?),
                    open_actions: r.get(12)?,
                    segment_count: r.get(13)?,
                    snippet: r.get(14)?,
                })
            })?;
            let mut list: Vec<MeetingSummary> = rows.collect::<rusqlite::Result<_>>()?;
            if let Some(q) = &fts {
                let mut range = c.prepare("SELECT MIN(id), MAX(id) FROM segments WHERE meeting_id=?1")?;
                let mut snip = c.prepare(
                    "SELECT snippet(segments_fts, 0, '', '', '…', 12) FROM segments_fts \
                     JOIN segments s ON s.id = segments_fts.rowid \
                     WHERE segments_fts MATCH ?1 AND segments_fts.rowid BETWEEN ?2 AND ?3 \
                       AND s.meeting_id = ?4 AND s.private = 0 LIMIT 1",
                )?;
                for m in list.iter_mut() {
                    let (lo, hi): (Option<i64>, Option<i64>) =
                        range.query_row([&m.meeting.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    if let (Some(lo), Some(hi)) = (lo, hi) {
                        m.snippet = snip
                            .query_row(params![q, lo, hi, m.meeting.id], |r| r.get(0))
                            .optional()?;
                    }
                }
            }
            Ok(list)
        })
    }

    pub fn set_meeting_status(&self, id: &str, status: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE meetings SET status=?2 WHERE id=?1", params![id, status]))?;
        Ok(())
    }

    pub fn end_meeting(&self, id: &str, ended_at: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE meetings SET status='ended', ended_at=?2 WHERE id=?1 AND status='live'",
                params![id, ended_at],
            )
        })?;
        Ok(())
    }

    pub fn rename_meeting(&self, id: &str, title: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE meetings SET title=?2, title_source='user' WHERE id=?1",
                params![id, title],
            )
        })?;
        Ok(())
    }

    /// Met à jour le titre seulement s'il ne vient pas de l'utilisateur.
    pub fn set_meeting_title_auto(&self, id: &str, title: &str, source: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE meetings SET title=?2, title_source=?3 WHERE id=?1 AND COALESCE(title_source,'') <> 'user'",
                params![id, title, source],
            )
        })?;
        Ok(())
    }

    pub fn set_confidential(&self, id: &str, on: bool) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE meetings SET confidential=?2 WHERE id=?1",
                params![id, on as i64],
            )
        })?;
        Ok(())
    }

    pub fn set_meeting_correlation(&self, id: &str, mode: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE meetings SET correlation=?2 WHERE id=?1", params![id, mode])
        })?;
        Ok(())
    }

    pub fn set_agent_session(&self, id: &str, provider: &str, session: Option<&str>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE meetings SET agent_provider=?2, agent_session_id=?3 WHERE id=?1",
                params![id, provider, session],
            )
        })?;
        Ok(())
    }

    pub fn delete_meeting(&self, id: &str) -> Result<()> {
        self.tx(|t| {
            t.execute("DELETE FROM jobs WHERE meeting_id=?1", [id])?;
            t.execute("DELETE FROM meetings WHERE id=?1", [id])
        })?;
        Ok(())
    }

    /// Identifiants des N dernières réunions terminées.
    pub fn recent_meeting_ids(&self, n: i64) -> Result<Vec<String>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id FROM meetings ORDER BY started_at DESC LIMIT ?1")?;
            let rows = st.query_map([n], |r| r.get(0))?;
            rows.collect()
        })
    }

    // ---------------------------------------------------------------- segments

    pub fn insert_segment(&self, s: &NewSegment) -> Result<Segment> {
        self.tx(|t| {
            let seq: i64 = t.query_row(
                "SELECT COALESCE(MAX(seq),0)+1 FROM segments WHERE meeting_id=?1",
                [&s.meeting_id],
                |r| r.get(0),
            )?;
            t.execute(
                "INSERT INTO segments (meeting_id, seq, ts, t0, t1, channel, speaker, text, private, is_echo) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    s.meeting_id, seq, s.ts, s.t0, s.t1, s.channel, s.speaker, s.text,
                    s.private as i64, s.is_echo as i64
                ],
            )?;
            let id = t.last_insert_rowid();
            Ok(Segment {
                id,
                meeting_id: s.meeting_id.clone(),
                seq,
                ts: s.ts.clone(),
                t0: s.t0,
                t1: s.t1,
                channel: s.channel.clone(),
                speaker: s.speaker.clone(),
                text: s.text.clone(),
                private: s.private,
                is_echo: s.is_echo,
                request_id: None,
            })
        })
    }

    pub fn get_segment(&self, id: i64) -> Result<Option<Segment>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {SEGMENT_COLS} FROM segments WHERE id=?1"), [id], segment_row)
                .optional()
        })
    }

    pub fn mark_echo(&self, segment_id: i64) -> Result<()> {
        self.with(|c| c.execute("UPDATE segments SET is_echo=1 WHERE id=?1", [segment_id]))?;
        Ok(())
    }

    pub fn link_segments_to_request(&self, ids: &[i64], request_id: &str) -> Result<()> {
        self.tx(|t| {
            for id in ids {
                t.execute(
                    "UPDATE segments SET request_id=?2 WHERE id=?1",
                    params![id, request_id],
                )?;
            }
            Ok(())
        })
    }

    pub fn segments(&self, meeting_id: &str, include_private: bool, include_echo: bool) -> Result<Vec<Segment>> {
        self.with(|c| {
            let mut sql = format!("SELECT {SEGMENT_COLS} FROM segments WHERE meeting_id=?1 ");
            if !include_private {
                sql.push_str("AND private=0 ");
            }
            if !include_echo {
                sql.push_str("AND is_echo=0 ");
            }
            // Ordre chronologique : les deux canaux arrivent avec un décalage (une longue phrase
            // du micro est enregistrée après les réponses courtes de l'appel). Une phrase sans
            // horodatage reste derrière la précédente.
            sql.push_str(
                "ORDER BY COALESCE(t0, (SELECT p.t0 FROM segments p WHERE p.meeting_id=segments.meeting_id \
                 AND p.seq < segments.seq AND p.t0 IS NOT NULL ORDER BY p.seq DESC LIMIT 1), 0), seq",
            );
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map([meeting_id], segment_row)?;
            rows.collect()
        })
    }

    /// Supprime une phrase de la transcription ; renvoie l'identifiant de sa réunion.
    pub fn delete_segment(&self, segment_id: i64) -> Result<Option<String>> {
        self.with(|c| {
            let mid: Option<String> = c
                .query_row("SELECT meeting_id FROM segments WHERE id=?1", [segment_id], |r| r.get(0))
                .optional()?;
            c.execute("DELETE FROM segments WHERE id=?1", [segment_id])?;
            Ok(mid)
        })
    }

    // ---------- Contexte de réunion ----------

    /// Contexte d'une réunion : texte, modèle et liste des documents (sans leur texte).
    pub fn meeting_context(&self, meeting_id: &str) -> Result<crate::context::MeetingContext> {
        self.with(|c| {
            let (text, template): (String, Option<String>) = c
                .query_row("SELECT text, template FROM meeting_context WHERE meeting_id=?1", [meeting_id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?
                .unwrap_or_default();
            let mut st = c.prepare(
                "SELECT id, meeting_id, name, kind, size, length(text), truncated, added_at FROM context_files \
                 WHERE meeting_id=?1 ORDER BY id",
            )?;
            let files = st
                .query_map([meeting_id], |r| {
                    Ok(crate::context::ContextFile {
                        id: r.get(0)?,
                        meeting_id: r.get(1)?,
                        name: r.get(2)?,
                        kind: r.get(3)?,
                        size: r.get(4)?,
                        chars: r.get(5)?,
                        truncated: r.get::<_, i64>(6)? != 0,
                        added_at: r.get(7)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(crate::context::MeetingContext {
                meeting_id: meeting_id.into(),
                text,
                template,
                files,
                report_at: None,
                inherited_template: None,
            })
        })
    }

    // ---------- Réunions prévues et participants ----------

    /// Réunions prévues, de la plus proche à la plus lointaine (celles de plus d'un jour dans le
    /// passé sont ignorées).
    pub fn planned_meetings(&self) -> Result<Vec<Meeting>> {
        let since = (chrono::Local::now() - chrono::Duration::days(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {MEETING_COLS} FROM meetings WHERE status='planned' AND started_at >= ?1 ORDER BY started_at"
            ))?;
            let rows = st.query_map([since], meeting_row)?;
            rows.collect()
        })
    }

    /// Réunion prévue à reprendre maintenant : heure prévue entre `before_min` minutes avant et
    /// `after_min` minutes après `now`, la plus proche.
    pub fn due_planned_meeting(&self, now: chrono::DateTime<chrono::Local>, before_min: i64, after_min: i64) -> Result<Option<Meeting>> {
        let lo = (now - chrono::Duration::minutes(after_min)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        let hi = (now + chrono::Duration::minutes(before_min)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        let list: Vec<Meeting> = self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {MEETING_COLS} FROM meetings WHERE status='planned' AND started_at BETWEEN ?1 AND ?2"
            ))?;
            let rows = st.query_map([lo, hi], meeting_row)?;
            rows.collect()
        })?;
        Ok(list.into_iter().min_by_key(|m| {
            chrono::DateTime::parse_from_rfc3339(&m.started_at).map(|t| (t.timestamp() - now.timestamp()).abs()).unwrap_or(i64::MAX)
        }))
    }

    /// Une réunion prévue démarre : statut `live`, heure réelle de début, app de l'appel.
    pub fn activate_planned(&self, id: &str, started_at: &str, call_app: Option<&str>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE meetings SET status='live', started_at=?2, call_app=COALESCE(?3, call_app) WHERE id=?1 AND status='planned'",
                params![id, started_at, call_app],
            )?;
            Ok(())
        })
    }

    pub fn set_meeting_event(&self, meeting_id: &str, event_id: Option<&str>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO meeting_context (meeting_id, text, event_id, updated_at) VALUES (?1,'',?2,NULL) \
                 ON CONFLICT(meeting_id) DO UPDATE SET event_id=excluded.event_id",
                params![meeting_id, event_id],
            )?;
            Ok(())
        })
    }

    /// Réunion (prévue ou passée) liée à un événement du calendrier.
    pub fn meeting_for_event(&self, event_id: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row(
                "SELECT x.meeting_id FROM meeting_context x JOIN meetings m ON m.id = x.meeting_id WHERE x.event_id=?1 \
                 ORDER BY m.started_at DESC LIMIT 1",
                [event_id],
                |r| r.get(0),
            )
            .optional()
        })
    }

    pub fn participants(&self, meeting_id: &str) -> Result<Vec<crate::planning::Participant>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT name, email, source FROM meeting_participants WHERE meeting_id=?1 ORDER BY id")?;
            let rows = st.query_map([meeting_id], |r| {
                Ok(crate::planning::Participant { name: r.get(0)?, email: r.get(1)?, source: r.get(2)? })
            })?;
            rows.collect()
        })
    }

    pub fn set_participants(&self, meeting_id: &str, list: &[crate::planning::Participant]) -> Result<()> {
        self.tx(|t| {
            t.execute("DELETE FROM meeting_participants WHERE meeting_id=?1", [meeting_id])?;
            for p in list {
                let name = p.name.trim();
                if name.is_empty() {
                    continue;
                }
                t.execute(
                    "INSERT INTO meeting_participants (meeting_id, name, email, source) VALUES (?1,?2,?3,?4)",
                    params![meeting_id, name, p.email.as_deref().map(str::trim).filter(|e| !e.is_empty()), p.source],
                )?;
            }
            Ok(())
        })
    }

    /// Dernière modification du contexte (texte, format ou documents), UTC.
    pub fn touch_meeting_context(&self, meeting_id: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO meeting_context (meeting_id, text, updated_at) VALUES (?1,'',?2) \
                 ON CONFLICT(meeting_id) DO UPDATE SET updated_at=excluded.updated_at",
                params![meeting_id, utc_now()],
            )?;
            Ok(())
        })
    }

    pub fn context_updated_at(&self, meeting_id: &str) -> Result<Option<String>> {
        self.with(|c| {
            Ok(c.query_row("SELECT updated_at FROM meeting_context WHERE meeting_id=?1", [meeting_id], |r| r.get(0))
                .optional()?
                .flatten())
        })
    }

    /// Format choisi explicitement pour la réunion la plus récente du même titre (titre
    /// normalisé : casse, chiffres et ponctuation ignorés ; titres déduits exclus).
    pub fn template_for_title(&self, title: &str, exclude_meeting: &str) -> Result<Option<String>> {
        let key = crate::text::title_key(title);
        if key.is_empty() {
            return Ok(None);
        }
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT m.title, mc.template FROM meetings m JOIN meeting_context mc ON mc.meeting_id = m.id \
                 WHERE mc.template IS NOT NULL AND mc.template <> '' AND m.id <> ?1 AND m.title IS NOT NULL \
                 AND COALESCE(m.title_source, '') IN ('user', 'calendar', 'call_window') \
                 ORDER BY m.started_at DESC LIMIT 500",
            )?;
            let rows = st
                .query_map([exclude_meeting], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows.into_iter().find(|(t, _)| crate::text::title_key(t) == key).map(|(_, tpl)| tpl))
        })
    }

    pub fn set_meeting_context(&self, meeting_id: &str, text: &str, template: Option<&str>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO meeting_context (meeting_id, text, template, updated_at) VALUES (?1,?2,?3,?4) \
                 ON CONFLICT(meeting_id) DO UPDATE SET text=excluded.text, template=excluded.template, updated_at=excluded.updated_at",
                params![meeting_id, text, template, utc_now()],
            )?;
            Ok(())
        })
    }

    pub fn add_context_file(
        &self,
        meeting_id: &str,
        name: &str,
        kind: &str,
        size: i64,
        text: &str,
        truncated: bool,
    ) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO context_files (meeting_id, name, kind, size, text, truncated, added_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![meeting_id, name, kind, size, text, truncated as i64, now_iso()],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    /// Supprime un document ; renvoie l'identifiant de sa réunion.
    pub fn delete_context_file(&self, id: i64) -> Result<Option<String>> {
        self.with(|c| {
            let mid: Option<String> =
                c.query_row("SELECT meeting_id FROM context_files WHERE id=?1", [id], |r| r.get(0)).optional()?;
            c.execute("DELETE FROM context_files WHERE id=?1", [id])?;
            Ok(mid)
        })
    }

    /// Texte des documents d'une réunion : (nom, texte, tronqué).
    pub fn context_texts(&self, meeting_id: &str) -> Result<Vec<(String, String, bool)>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT name, text, truncated FROM context_files WHERE meeting_id=?1 ORDER BY id")?;
            let rows = st.query_map([meeting_id], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)))?;
            rows.collect()
        })
    }

    /// Remplace le texte d'une phrase (retranscription de fin de réunion).
    pub fn set_segment_text(&self, segment_id: i64, text: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE segments SET text=?2 WHERE id=?1", params![segment_id, text])?;
            Ok(())
        })
    }

    pub fn set_segment_speaker(&self, segment_id: i64, speaker: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE segments SET speaker=?2 WHERE id=?1", params![segment_id, speaker])
        })?;
        Ok(())
    }

    // ---------------------------------------------------------------- intervenants

    pub fn upsert_speaker(&self, meeting_id: &str, label: &str, display_name: Option<&str>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO speakers (meeting_id, label, display_name) VALUES (?1,?2,?3) \
                 ON CONFLICT(meeting_id, label) DO UPDATE SET display_name=COALESCE(excluded.display_name, display_name)",
                params![meeting_id, label, display_name],
            )
        })?;
        Ok(())
    }

    pub fn rename_speaker(&self, meeting_id: &str, label: &str, display_name: &str) -> Result<()> {
        let dn = display_name.trim();
        self.with(|c| {
            c.execute(
                "INSERT INTO speakers (meeting_id, label, display_name) VALUES (?1,?2,?3) \
                 ON CONFLICT(meeting_id, label) DO UPDATE SET display_name=excluded.display_name",
                params![meeting_id, label, if dn.is_empty() { None } else { Some(dn) }],
            )
        })?;
        Ok(())
    }

    pub fn speakers(&self, meeting_id: &str) -> Result<Vec<Speaker>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT label, display_name FROM speakers WHERE meeting_id=?1 \
                 UNION SELECT DISTINCT speaker, NULL FROM segments WHERE meeting_id=?1 \
                   AND speaker NOT IN (SELECT label FROM speakers WHERE meeting_id=?1) \
                 ORDER BY 1",
            )?;
            let rows = st.query_map([meeting_id], |r| {
                Ok(Speaker { label: r.get(0)?, display_name: r.get(1)? })
            })?;
            rows.collect()
        })
    }

    /// Tous les noms d'intervenants connus (pour les contrôles du mot d'appel).
    pub fn all_speaker_names(&self) -> Result<Vec<String>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT DISTINCT display_name FROM speakers WHERE display_name IS NOT NULL AND display_name <> ''",
            )?;
            let rows = st.query_map([], |r| r.get(0))?;
            rows.collect()
        })
    }

    // ---------------------------------------------------------------- notes

    pub fn add_note(&self, meeting_id: &str, text: &str, source: &str) -> Result<Note> {
        let ts = now_iso();
        let id = self.with(|c| {
            c.execute(
                "INSERT INTO notes (meeting_id, ts, text, source) VALUES (?1,?2,?3,?4)",
                params![meeting_id, ts, text, source],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        Ok(Note { id, meeting_id: meeting_id.into(), ts, text: text.into(), source: source.into() })
    }

    pub fn update_note(&self, id: i64, text: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE notes SET text=?2 WHERE id=?1", params![id, text]))?;
        Ok(())
    }

    pub fn delete_note(&self, id: i64) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM notes WHERE id=?1", [id]))?;
        Ok(())
    }

    pub fn notes(&self, meeting_id: &str) -> Result<Vec<Note>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, meeting_id, ts, text, source FROM notes WHERE meeting_id=?1 ORDER BY ts, id",
            )?;
            let rows = st.query_map([meeting_id], |r| {
                Ok(Note {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    ts: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    text: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    source: r.get::<_, Option<String>>(4)?.unwrap_or_else(|| "user".into()),
                })
            })?;
            rows.collect()
        })
    }

    // ---------------------------------------------------------------- demandes

    pub fn insert_request(&self, r: &Request) -> Result<()> {
        self.with(|c| {
            c.execute(
                &format!("INSERT INTO requests ({REQUEST_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)"),
                params![
                    r.id, r.meeting_id, r.created_at, r.origin, r.intent, r.query, r.context_seconds,
                    r.tier, r.status, r.started_at, r.finished_at, r.error, r.summary, r.answer_md,
                    serde_json::to_string(&r.tools_used).unwrap_or_else(|_| "[]".into())
                ],
            )
        })?;
        Ok(())
    }

    pub fn get_request(&self, id: &str) -> Result<Option<Request>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {REQUEST_COLS} FROM requests WHERE id=?1"), [id], request_row)
                .optional()
        })
    }

    /// Marks a request as running. False if it is no longer queued or running (cancelled in
    /// the meantime): the caller must not run it.
    pub fn set_request_running(&self, id: &str) -> Result<bool> {
        self.with(|c| {
            c.execute(
                "UPDATE requests SET status='running', started_at=?2 WHERE id=?1 AND status IN ('queued','running')",
                params![id, now_iso()],
            )
        })
        .map(|n| n > 0)
    }

    pub fn finish_request(
        &self,
        id: &str,
        status: &str,
        summary: Option<&str>,
        answer_md: Option<&str>,
        tools_used: &[String],
        error: Option<&str>,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE requests SET status=?2, finished_at=?3, summary=?4, answer_md=?5, tools_used=?6, error=?7 WHERE id=?1",
                params![
                    id, status, now_iso(), summary, answer_md,
                    serde_json::to_string(tools_used).unwrap_or_else(|_| "[]".into()), error
                ],
            )
        })?;
        Ok(())
    }

    pub fn requests(&self, meeting_id: &str) -> Result<Vec<Request>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {REQUEST_COLS} FROM requests WHERE meeting_id=?1 ORDER BY created_at, id"
            ))?;
            let rows = st.query_map([meeting_id], request_row)?;
            rows.collect()
        })
    }

    pub fn recent_requests(&self, limit: i64) -> Result<Vec<Request>> {
        self.with(|c| {
            let mut st = c.prepare(&format!(
                "SELECT {REQUEST_COLS} FROM requests ORDER BY created_at DESC, rowid DESC LIMIT ?1"
            ))?;
            let rows = st.query_map([limit], request_row)?;
            rows.collect()
        })
    }

    /// Dernière demande en attente ou en cours (pour « annule »).
    pub fn last_active_request(&self, meeting_id: Option<&str>) -> Result<Option<Request>> {
        self.with(|c| {
            c.query_row(
                &format!(
                    "SELECT {REQUEST_COLS} FROM requests WHERE status IN ('queued','running') \
                     AND (?1 IS NULL OR meeting_id=?1) AND intent <> 'cancel' \
                     ORDER BY created_at DESC, rowid DESC LIMIT 1"
                ),
                [meeting_id],
                request_row,
            )
            .optional()
        })
    }

    pub fn count_active_requests(&self) -> Result<i64> {
        self.with(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM requests WHERE status IN ('queued','running')",
                [],
                |r| r.get(0),
            )
        })
    }

    // ---------------------------------------------------------------- comptes rendus

    /// Enregistre une nouvelle version du compte rendu et renvoie son numéro.
    pub fn save_report(
        &self,
        meeting_id: &str,
        markdown: &str,
        provider: Option<&str>,
        model: Option<&str>,
        correlated: bool,
    ) -> Result<i64> {
        let now = now_iso();
        self.tx(|t| {
            let version: i64 = t.query_row(
                "SELECT COALESCE(MAX(version),0)+1 FROM report_versions WHERE meeting_id=?1",
                [meeting_id],
                |r| r.get(0),
            )?;
            t.execute(
                "INSERT INTO report_versions (meeting_id, version, markdown, generated_at, provider, model, correlated) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![meeting_id, version, markdown, now, provider, model, correlated as i64],
            )?;
            t.execute(
                "INSERT INTO reports (meeting_id, markdown, generated_at, provider, model, correlated, version) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7) \
                 ON CONFLICT(meeting_id) DO UPDATE SET markdown=excluded.markdown, generated_at=excluded.generated_at, \
                 provider=excluded.provider, model=excluded.model, correlated=excluded.correlated, version=excluded.version",
                params![meeting_id, markdown, now, provider, model, correlated as i64, version],
            )?;
            Ok(version)
        })
    }

    pub fn get_report(&self, meeting_id: &str) -> Result<Option<Report>> {
        self.with(|c| {
            c.query_row(
                "SELECT meeting_id, markdown, generated_at, provider, model, correlated, version FROM reports WHERE meeting_id=?1",
                [meeting_id],
                report_row,
            )
            .optional()
        })
    }

    pub fn get_report_version(&self, meeting_id: &str, version: i64) -> Result<Option<Report>> {
        self.with(|c| {
            c.query_row(
                "SELECT meeting_id, markdown, generated_at, provider, model, correlated, version \
                 FROM report_versions WHERE meeting_id=?1 AND version=?2",
                params![meeting_id, version],
                report_row,
            )
            .optional()
        })
    }

    pub fn report_versions(&self, meeting_id: &str) -> Result<Vec<ReportVersion>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT version, generated_at, correlated FROM report_versions WHERE meeting_id=?1 ORDER BY version DESC",
            )?;
            let rows = st.query_map([meeting_id], |r| {
                Ok(ReportVersion {
                    version: r.get(0)?,
                    generated_at: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    correlated: b(r.get(2)?),
                })
            })?;
            rows.collect()
        })
    }

    pub fn replace_refs(&self, meeting_id: &str, refs: &[ReportRef]) -> Result<()> {
        self.tx(|t| {
            t.execute("DELETE FROM report_refs WHERE meeting_id=?1", [meeting_id])?;
            for r in refs {
                t.execute(
                    "INSERT INTO report_refs (meeting_id, kind, ref, title, url, verified, note) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![meeting_id, r.kind, r.reference, r.title, r.url, r.verified as i64, r.note],
                )?;
            }
            Ok(())
        })
    }

    pub fn refs(&self, meeting_id: &str) -> Result<Vec<ReportRef>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, kind, ref, title, url, verified, note FROM report_refs WHERE meeting_id=?1 ORDER BY id",
            )?;
            let rows = st.query_map([meeting_id], |r| {
                Ok(ReportRef {
                    id: r.get(0)?,
                    kind: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    reference: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    title: r.get(3)?,
                    url: r.get(4)?,
                    verified: b(r.get::<_, Option<i64>>(5)?.unwrap_or(0)),
                    note: r.get(6)?,
                })
            })?;
            rows.collect()
        })
    }

    // ---------------------------------------------------------------- décisions et actions

    /// Remplace décisions et actions d'une réunion en conservant l'état « fait »
    /// des actions dont le texte n'a pas changé.
    pub fn replace_decisions_actions(
        &self,
        meeting_id: &str,
        decisions: &[String],
        actions: &[(Option<String>, String, Option<String>)],
    ) -> Result<()> {
        let now = now_iso();
        self.tx(|t| {
            let done: Vec<String> = {
                let mut st = t.prepare("SELECT text FROM actions WHERE meeting_id=?1 AND status='done'")?;
                let rows = st.query_map([meeting_id], |r| r.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            t.execute("DELETE FROM decisions WHERE meeting_id=?1", [meeting_id])?;
            t.execute("DELETE FROM actions WHERE meeting_id=?1", [meeting_id])?;
            for d in decisions {
                t.execute(
                    "INSERT INTO decisions (meeting_id, text, created_at) VALUES (?1,?2,?3)",
                    params![meeting_id, d, now],
                )?;
            }
            for (owner, text, due) in actions {
                let status = if done.iter().any(|d| d == text) { "done" } else { "open" };
                t.execute(
                    "INSERT INTO actions (meeting_id, owner, text, due, status) VALUES (?1,?2,?3,?4,?5)",
                    params![meeting_id, owner, text, due, status],
                )?;
            }
            Ok(())
        })
    }

    pub fn add_action(&self, meeting_id: &str, owner: Option<&str>, text: &str, due: Option<&str>) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO actions (meeting_id, owner, text, due, status) VALUES (?1,?2,?3,?4,'open')",
                params![meeting_id, owner, text, due],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn decisions(&self, meeting_id: &str) -> Result<Vec<Decision>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, meeting_id, text, created_at FROM decisions WHERE meeting_id=?1 ORDER BY id",
            )?;
            let rows = st.query_map([meeting_id], |r| {
                Ok(Decision {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    text: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    created_at: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                })
            })?;
            rows.collect()
        })
    }

    pub fn list_decisions(&self, since: Option<&str>) -> Result<Vec<Decision>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT d.id, d.meeting_id, d.text, m.started_at FROM decisions d JOIN meetings m ON m.id=d.meeting_id \
                 WHERE (?1 IS NULL OR m.started_at >= ?1) ORDER BY m.started_at DESC, d.id",
            )?;
            let rows = st.query_map([since], |r| {
                Ok(Decision {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    text: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    created_at: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                })
            })?;
            rows.collect()
        })
    }

    pub fn actions(&self, meeting_id: &str) -> Result<Vec<Action>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, meeting_id, owner, text, due, status FROM actions WHERE meeting_id=?1 ORDER BY id",
            )?;
            let rows = st.query_map([meeting_id], action_row)?;
            rows.collect()
        })
    }

    pub fn list_actions(&self, f: &ActionFilter) -> Result<Vec<Action>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT a.id, a.meeting_id, a.owner, a.text, a.due, a.status, m.title, m.started_at \
                 FROM actions a JOIN meetings m ON m.id=a.meeting_id \
                 WHERE (?1 IS NULL OR a.status=?1) \
                   AND (?2 IS NULL OR a.owner LIKE '%' || ?2 || '%') \
                   AND (?3 IS NULL OR m.started_at >= ?3) \
                   AND (?4 IS NULL OR substr(m.started_at,1,10) <= ?4) \
                 ORDER BY a.status='done', m.started_at DESC, a.id",
            )?;
            let rows = st.query_map(params![f.status, f.owner, f.from, f.to], |r| {
                let mut a = action_row(r)?;
                a.meeting_title = r.get(6)?;
                a.meeting_started_at = r.get(7)?;
                Ok(a)
            })?;
            rows.collect()
        })
    }

    pub fn set_action_status(&self, id: i64, status: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE actions SET status=?2 WHERE id=?1", params![id, status]))?;
        Ok(())
    }

    // ---------------------------------------------------------------- brouillons

    pub fn add_draft(&self, meeting_id: &str, kind: &str, target: Option<&str>, body: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO drafts (meeting_id, kind, target, body, created_at) VALUES (?1,?2,?3,?4,?5)",
                params![meeting_id, kind, target, body, now_iso()],
            )
        })?;
        Ok(())
    }

    pub fn clear_drafts(&self, meeting_id: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM drafts WHERE meeting_id=?1", [meeting_id]))?;
        Ok(())
    }

    pub fn drafts(&self, meeting_id: &str) -> Result<Vec<Draft>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, kind, target, body, created_at FROM drafts WHERE meeting_id=?1 ORDER BY id",
            )?;
            let rows = st.query_map([meeting_id], |r| {
                Ok(Draft {
                    id: r.get(0)?,
                    kind: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    target: r.get(2)?,
                    body: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    created_at: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                })
            })?;
            rows.collect()
        })
    }

    pub fn meeting_detail(&self, id: &str) -> Result<Option<MeetingDetail>> {
        let Some(meeting) = self.get_meeting(id)? else { return Ok(None) };
        Ok(Some(MeetingDetail {
            speakers: self.speakers(id)?,
            notes: self.notes(id)?,
            requests: self.requests(id)?,
            report: self.get_report(id)?,
            report_versions: self.report_versions(id)?,
            refs: self.refs(id)?,
            decisions: self.decisions(id)?,
            actions: self.actions(id)?,
            drafts: self.drafts(id)?,
            meeting,
        }))
    }

    // ---------------------------------------------------------------- tâches

    pub fn enqueue_job(&self, kind: &str, meeting_id: Option<&str>, payload: &serde_json::Value) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO jobs (type, meeting_id, payload, status, attempts, created_at) VALUES (?1,?2,?3,'pending',0,?4)",
                params![kind, meeting_id, payload.to_string(), now_iso()],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    /// Tâche qui ne démarrera pas avant `not_before` (UTC, RFC 3339).
    pub fn enqueue_job_at(
        &self,
        kind: &str,
        meeting_id: Option<&str>,
        payload: &serde_json::Value,
        not_before: Option<&str>,
    ) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "INSERT INTO jobs (type, meeting_id, payload, status, attempts, created_at, not_before) \
                 VALUES (?1,?2,?3,'pending',0,?4,?5)",
                params![kind, meeting_id, payload.to_string(), now_iso(), not_before],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    /// Heure de départ d'une tâche en attente de ce type pour la réunion : `Some(None)` = dès que
    /// possible, `None` = aucune tâche en attente.
    pub fn pending_job_start(&self, kind: &str, meeting_id: &str) -> Result<Option<Option<String>>> {
        self.with(|c| {
            c.query_row(
                "SELECT not_before FROM jobs WHERE type=?1 AND meeting_id=?2 AND status='pending' ORDER BY id LIMIT 1",
                params![kind, meeting_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
        })
    }

    /// Change l'heure de départ des tâches en attente de ce type pour la réunion.
    pub fn set_job_start(&self, kind: &str, meeting_id: &str, not_before: Option<&str>) -> Result<usize> {
        self.with(|c| {
            Ok(c.execute(
                "UPDATE jobs SET not_before=?3 WHERE type=?1 AND meeting_id=?2 AND status='pending'",
                params![kind, meeting_id, not_before],
            )?)
        })
    }

    /// Réserve la plus ancienne tâche en attente parmi les types donnés (et dont l'heure est venue).
    pub fn claim_job(&self, kinds: &[&str]) -> Result<Option<Job>> {
        if kinds.is_empty() {
            return Ok(None);
        }
        let placeholders = kinds.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect::<Vec<_>>().join(",");
        self.tx(|t| {
            let job = t
                .query_row(
                    &format!(
                        "SELECT id, type, meeting_id, payload, status, attempts, created_at, error FROM jobs \
                         WHERE status='pending' AND type IN ({placeholders}) \
                         AND (not_before IS NULL OR not_before <= ?{n}) ORDER BY id LIMIT 1",
                        n = kinds.len() + 1
                    ),
                    rusqlite::params_from_iter(kinds.iter().map(|k| k.to_string()).chain(std::iter::once(utc_now()))),
                    job_row,
                )
                .optional()?;
            if let Some(j) = &job {
                t.execute(
                    "UPDATE jobs SET status='running', started_at=?2, attempts=attempts+1 WHERE id=?1",
                    params![j.id, now_iso()],
                )?;
            }
            Ok(job)
        })
    }

    pub fn finish_job(&self, id: i64, status: &str, error: Option<&str>) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE jobs SET status=?2, finished_at=?3, error=?4 WHERE id=?1",
                params![id, status, now_iso(), error],
            )
        })?;
        Ok(())
    }

    pub fn get_job(&self, id: i64) -> Result<Option<Job>> {
        self.with(|c| {
            c.query_row(
                "SELECT id, type, meeting_id, payload, status, attempts, created_at, error FROM jobs WHERE id=?1",
                [id],
                job_row,
            )
            .optional()
        })
    }

    /// Au redémarrage : les tâches interrompues repartent (3 essais au maximum).
    pub fn recover_jobs(&self) -> Result<usize> {
        self.with(|c| {
            c.execute(
                "UPDATE jobs SET status='error', error='abandonnée après 3 essais' WHERE status='running' AND attempts>=3",
                [],
            )?;
            // Their requests would otherwise stay queued or running forever.
            c.execute(
                "UPDATE requests SET status='error', finished_at=?1, error='abandonnée après 3 essais' \
                 WHERE status IN ('queued','running') AND id IN \
                 (SELECT json_extract(payload, '$.request_id') FROM jobs WHERE type='request' AND status='error')",
                [now_iso()],
            )?;
            c.execute("UPDATE jobs SET status='pending' WHERE status='running'", [])
        })
    }

    pub fn pending_jobs(&self) -> Result<Vec<Job>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, type, meeting_id, payload, status, attempts, created_at, error FROM jobs \
                 WHERE status IN ('pending','running') ORDER BY id",
            )?;
            let rows = st.query_map([], job_row)?;
            rows.collect()
        })
    }

    pub fn has_job(&self, kind: &str, meeting_id: &str, statuses: &[&str]) -> Result<bool> {
        let st_list = statuses.iter().map(|s| format!("'{}'", s.replace('\'', ""))).collect::<Vec<_>>().join(",");
        self.with(|c| {
            c.query_row(
                &format!("SELECT 1 FROM jobs WHERE type=?1 AND meeting_id=?2 AND status IN ({st_list}) LIMIT 1"),
                params![kind, meeting_id],
                |_| Ok(()),
            )
            .optional()
            .map(|o| o.is_some())
        })
    }

    // ---------------------------------------------------------------- dépôts

    pub fn list_repos(&self) -> Result<Vec<Repo>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, label, path, use_live, use_correlation, exclude_globs FROM repos ORDER BY label, id",
            )?;
            let rows = st.query_map([], repo_row)?;
            rows.collect()
        })
    }

    pub fn add_repo(&self, path: &str, label: &str) -> Result<Repo> {
        self.with(|c| {
            c.execute(
                "INSERT INTO repos (label, path, use_live, use_correlation, exclude_globs) VALUES (?1,?2,1,1,'') \
                 ON CONFLICT(path) DO UPDATE SET label=excluded.label",
                params![label, path],
            )?;
            c.query_row(
                "SELECT id, label, path, use_live, use_correlation, exclude_globs FROM repos WHERE path=?1",
                [path],
                repo_row,
            )
        })
    }

    pub fn update_repo(&self, r: &Repo) -> Result<Repo> {
        self.with(|c| {
            c.execute(
                "UPDATE repos SET label=?2, path=?3, use_live=?4, use_correlation=?5, exclude_globs=?6 WHERE id=?1",
                params![r.id, r.label, r.path, r.use_live as i64, r.use_correlation as i64, r.exclude_globs],
            )?;
            c.query_row(
                "SELECT id, label, path, use_live, use_correlation, exclude_globs FROM repos WHERE id=?1",
                [r.id],
                repo_row,
            )
        })
    }

    pub fn remove_repo(&self, id: i64) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM repos WHERE id=?1", [id]))?;
        Ok(())
    }

    // ---------------------------------------------------------------- connecteurs

    pub fn list_connectors(&self, provider: Option<&str>) -> Result<Vec<Connector>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT id, provider, name, status, detected_at, use_live, use_correlation, write_blockable \
                 FROM connectors WHERE (?1 IS NULL OR provider=?1) ORDER BY name",
            )?;
            let rows = st.query_map([provider], connector_row)?;
            rows.collect()
        })
    }

    /// Met à jour la liste détectée pour un agent. Les connecteurs disparus sont marqués
    /// `absent`. Un connecteur non bloquable est forcé à « non utilisé ».
    pub fn sync_connectors(&self, provider: &str, servers: &[(String, String)], write_blockable: bool) -> Result<()> {
        let now = now_iso();
        self.tx(|t| {
            t.execute("UPDATE connectors SET status='absent' WHERE provider=?1", [provider])?;
            for (name, status) in servers {
                t.execute(
                    "INSERT INTO connectors (provider, name, status, detected_at, use_live, use_correlation, write_blockable) \
                     VALUES (?1,?2,?3,?4,0,0,?5) \
                     ON CONFLICT(provider, name) DO UPDATE SET status=excluded.status, detected_at=excluded.detected_at, \
                     write_blockable=excluded.write_blockable",
                    params![provider, name, status, now, write_blockable as i64],
                )?;
            }
            t.execute(
                "UPDATE connectors SET use_live=0, use_correlation=0 WHERE provider=?1 AND write_blockable=0",
                [provider],
            )?;
            Ok(())
        })
    }

    pub fn update_connector(&self, id: i64, use_live: bool, use_correlation: bool) -> Result<Connector> {
        self.with(|c| {
            let wb: i64 = c.query_row("SELECT write_blockable FROM connectors WHERE id=?1", [id], |r| r.get(0))?;
            let (l, co) = if wb == 0 { (false, false) } else { (use_live, use_correlation) };
            c.execute(
                "UPDATE connectors SET use_live=?2, use_correlation=?3 WHERE id=?1",
                params![id, l as i64, co as i64],
            )?;
            c.query_row(
                "SELECT id, provider, name, status, detected_at, use_live, use_correlation, write_blockable FROM connectors WHERE id=?1",
                [id],
                connector_row,
            )
        })
    }

    // ---------------------------------------------------------------- réglages / runtime

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0))
                .optional()
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings (key, value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )
        })?;
        Ok(())
    }

    pub fn set_runtime(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO runtime (key, value, updated_at) VALUES (?1,?2,?3) \
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![key, value, now_iso()],
            )
        })?;
        Ok(())
    }

    pub fn get_runtime(&self, key: &str) -> Result<Option<(String, String)>> {
        self.with(|c| {
            c.query_row("SELECT value, updated_at FROM runtime WHERE key=?1", [key], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()
        })
    }

    /// Vrai si l'app a signalé sa présence il y a moins de 15 s.
    pub fn app_is_running(&self) -> Result<bool> {
        Ok(self.get_runtime(HEARTBEAT)?.is_some_and(|(_, at)| heartbeat_fresh(&at)))
    }

    /// Writes this process's heartbeat, unless another process owns a fresh one: a second
    /// core (`steno serve` next to the app) does not take the app's place. It takes over
    /// once the owner's heartbeat is stale. True if this process owns the heartbeat.
    pub fn beat_heartbeat(&self, pid: u32) -> Result<bool> {
        let pid = pid.to_string();
        self.tx(|c| {
            let current: Option<(String, String)> = c
                .query_row("SELECT value, updated_at FROM runtime WHERE key=?1", [HEARTBEAT], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?;
            if current.is_some_and(|(owner, at)| owner != pid && heartbeat_fresh(&at)) {
                return Ok(false);
            }
            c.execute(
                "INSERT INTO runtime (key, value, updated_at) VALUES (?1,?2,?3) \
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
                params![HEARTBEAT, pid, now_iso()],
            )?;
            Ok(true)
        })
    }

    /// On shutdown: marks the heartbeat stopped, only if this process owns it.
    pub fn release_heartbeat(&self, pid: u32) -> Result<()> {
        self.with(|c| {
            c.execute(
                "UPDATE runtime SET value='stopped', updated_at='1970-01-01T00:00:00+00:00' WHERE key=?1 AND value=?2",
                params![HEARTBEAT, pid.to_string()],
            )
        })?;
        Ok(())
    }

    /// True if another process (not this one) signalled its presence less than 15 s ago.
    pub fn other_process_running(&self) -> Result<bool> {
        let own = std::process::id().to_string();
        Ok(self.get_runtime(HEARTBEAT)?.is_some_and(|(pid, at)| pid != own && heartbeat_fresh(&at)))
    }

    // ---------------------------------------------------------------- recherche

    /// Recherche plein texte dans les transcriptions (hors apartés) et les comptes rendus.
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let Some(q) = fts_query(query) else { return Ok(vec![]) };
        self.search_fts(&q, limit)
    }

    /// Recherche « au moins un mot » pour les questions en langage naturel.
    pub fn search_relevant(&self, question: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let Some(q) = fts_query_any(question) else { return Ok(vec![]) };
        self.search_fts(&q, limit)
    }

    fn search_fts(&self, q: &str, limit: i64) -> Result<Vec<SearchHit>> {
        self.with(|c| {
            let mut out = vec![];
            let mut st = c.prepare(
                "SELECT s.meeting_id, m.title, m.started_at, snippet(segments_fts, 0, '', '', '…', 16), s.ts \
                 FROM segments_fts JOIN segments s ON s.id = segments_fts.rowid JOIN meetings m ON m.id = s.meeting_id \
                 WHERE segments_fts MATCH ?1 AND s.private = 0 AND s.is_echo = 0 \
                 ORDER BY bm25(segments_fts) LIMIT ?2",
            )?;
            let rows = st.query_map(params![q, limit], |r| {
                Ok(SearchHit {
                    meeting_id: r.get(0)?,
                    title: r.get(1)?,
                    started_at: r.get(2)?,
                    kind: "segment".into(),
                    snippet: r.get(3)?,
                    ts: r.get(4)?,
                })
            })?;
            for h in rows {
                out.push(h?);
            }
            let mut st = c.prepare(
                "SELECT r.meeting_id, m.title, m.started_at, snippet(reports_fts, 0, '', '', '…', 16) \
                 FROM reports_fts JOIN reports r ON r.rowid = reports_fts.rowid JOIN meetings m ON m.id = r.meeting_id \
                 WHERE reports_fts MATCH ?1 ORDER BY bm25(reports_fts) LIMIT ?2",
            )?;
            let rows = st.query_map(params![q, limit], |r| {
                Ok(SearchHit {
                    meeting_id: r.get(0)?,
                    title: r.get(1)?,
                    started_at: r.get(2)?,
                    kind: "report".into(),
                    snippet: r.get(3)?,
                    ts: None,
                })
            })?;
            for h in rows {
                out.push(h?);
            }
            Ok(out)
        })
    }

    /// Nombre d'occurrences d'un mot dans les segments `me`/`them` des N dernières
    /// réunions, hors commandes. Renvoie (occurrences, réunions examinées).
    pub fn count_word_in_recent(&self, word: &str, n_meetings: i64) -> Result<(i64, i64)> {
        let Some(q) = fts_query(word) else { return Ok((0, 0)) };
        let ids = self.recent_meeting_ids(n_meetings)?;
        if ids.is_empty() {
            return Ok((0, 0));
        }
        let n = ids.len() as i64;
        let count = self.with(|c| {
            let mut total = 0i64;
            let mut st = c.prepare(
                "SELECT COUNT(*) FROM segments_fts JOIN segments s ON s.id = segments_fts.rowid \
                 WHERE segments_fts MATCH ?1 AND s.meeting_id = ?2 AND s.request_id IS NULL AND s.is_echo = 0",
            )?;
            for id in &ids {
                total += st.query_row(params![q, id], |r| r.get::<_, i64>(0))?;
            }
            Ok(total)
        })?;
        Ok((count, n))
    }

    // ---------------------------------------------------------------- maintenance

    /// Supprime les transcriptions des réunions plus anciennes que la date donnée.
    pub fn purge_transcripts_before(&self, before: &str) -> Result<usize> {
        self.with(|c| {
            c.execute(
                "DELETE FROM segments WHERE meeting_id IN (SELECT id FROM meetings WHERE started_at < ?1)",
                [before],
            )
        })
    }

    pub fn delete_all(&self) -> Result<()> {
        self.tx(|t| {
            for table in [
                "segments", "speakers", "notes", "requests", "report_refs", "report_versions", "reports",
                "decisions", "actions", "drafts", "jobs", "meetings",
            ] {
                t.execute(&format!("DELETE FROM {table}"), [])?;
            }
            t.execute("INSERT INTO segments_fts(segments_fts) VALUES ('rebuild')", [])?;
            t.execute("INSERT INTO reports_fts(reports_fts) VALUES ('rebuild')", [])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meeting(id: &str, title: &str) -> Meeting {
        Meeting {
            id: id.into(),
            title: Some(title.into()),
            title_source: Some("user".into()),
            started_at: "2026-11-26T10:30:00+01:00".into(),
            ended_at: None,
            status: "live".into(),
            confidential: false,
            agent_provider: None,
            agent_session_id: None,
            correlation: Some("on_demand".into()),
            call_app: None,
        }
    }

    fn seg(mid: &str, ch: &str, text: &str, private: bool) -> NewSegment {
        NewSegment {
            meeting_id: mid.into(),
            ts: now_iso(),
            t0: Some(1.0),
            t1: Some(2.0),
            channel: ch.into(),
            speaker: if ch == "me" { "Moi".into() } else { "Eux".into() },
            text: text.into(),
            private,
            is_echo: false,
        }
    }

    #[test]
    fn format_repris_des_reunions_du_meme_titre() {
        let db = Db::open_in_memory().unwrap();
        let mut old = meeting("m1", "Daily équipe 12/11");
        old.started_at = "2026-11-12T09:30:00+01:00".into();
        db.insert_meeting(&old).unwrap();
        db.set_meeting_context("m1", "", Some("daily")).unwrap();
        let mut guess = meeting("m2", "Daily équipe");
        guess.title_source = Some("inferred".into());
        guess.started_at = "2026-11-20T09:30:00+01:00".into();
        db.insert_meeting(&guess).unwrap();
        db.set_meeting_context("m2", "", Some("mail")).unwrap();
        db.insert_meeting(&meeting("m3", "DAILY Équipe 19/11 | Microsoft Teams")).unwrap();
        // Le titre déduit (m2) ne compte pas ; m1 donne « daily ».
        assert_eq!(db.template_for_title("DAILY Équipe 19/11 | Microsoft Teams", "m3").unwrap().as_deref(), Some("daily"));
        assert_eq!(db.template_for_title("Point client", "m3").unwrap(), None);
        assert_eq!(db.template_for_title("27/11", "m3").unwrap(), None);
    }

    #[test]
    fn segments_fts_et_apartes() {
        let db = Db::open_in_memory().unwrap();
        db.insert_meeting(&meeting("m1", "Daily ORION")).unwrap();
        let s1 = db.insert_segment(&seg("m1", "them", "Le verrou de créneau bloque ms-booking", false)).unwrap();
        let s2 = db.insert_segment(&seg("m1", "me", "Phrase privée sur l'évaluation", true)).unwrap();
        assert_eq!(s1.seq, 1);
        assert_eq!(s2.seq, 2);
        let hits = db.search("verrou creneau", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("verrou de créneau"));
        assert!(db.search("évaluation", 10).unwrap().is_empty(), "les apartés ne sont pas cherchables");
        assert_eq!(db.segments("m1", false, false).unwrap().len(), 1);
        // accents ignorés
        assert_eq!(db.search("verrou de créneau Ms-Booking", 10).unwrap().len(), 1);
        let list = db
            .list_meetings(&MeetingFilter { query: Some("creneau".into()), ..Default::default() })
            .unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].snippet.as_deref().unwrap_or("").contains("créneau"));
    }

    #[test]
    fn fts_ignores_tokens_without_letters_or_digits() {
        assert_eq!(fts_query("Daily - Équipe").as_deref(), Some("\"Daily\"* \"Équipe\"*"));
        assert_eq!(fts_query("---"), None);
        assert_eq!(fts_query("_ -"), None);
        assert_eq!(fts_query_any("Nova --- ___ planning").as_deref(), Some("\"Nova\"* OR \"planning\"*"));
        let db = Db::open_in_memory().unwrap();
        db.insert_meeting(&meeting("m1", "Daily - Équipe")).unwrap();
        db.insert_segment(&seg("m1", "them", "Le daily de l'équipe Nova", false)).unwrap();
        assert_eq!(db.search("Daily - Équipe", 10).unwrap().len(), 1);
        assert!(db.search("---", 10).unwrap().is_empty());
        assert!(db.search_relevant("quoi de neuf --- sur Nova ?", 10).unwrap().len() == 1);
        let list = db.list_meetings(&MeetingFilter { query: Some("daily - équipe".into()), ..Default::default() }).unwrap();
        assert_eq!(list.len(), 1);
    }

    fn request(id: &str, status: &str) -> Request {
        Request {
            id: id.into(),
            meeting_id: None,
            created_at: now_iso(),
            origin: "cli".into(),
            intent: "history".into(),
            query: "Où en est Nova ?".into(),
            context_seconds: None,
            tier: "search".into(),
            status: status.into(),
            started_at: None,
            finished_at: None,
            error: None,
            summary: None,
            answer_md: None,
            tools_used: vec![],
        }
    }

    #[test]
    fn cancelled_request_is_not_set_running_again() {
        let db = Db::open_in_memory().unwrap();
        db.insert_request(&request("r1", "queued")).unwrap();
        assert!(db.set_request_running("r1").unwrap());
        assert!(db.set_request_running("r1").unwrap(), "a retried job resumes a running request");
        db.finish_request("r1", "cancelled", Some("Demande annulée."), None, &[], None).unwrap();
        assert!(!db.set_request_running("r1").unwrap());
        assert_eq!(db.get_request("r1").unwrap().unwrap().status, "cancelled");
    }

    #[test]
    fn abandoned_request_jobs_fail_their_requests() {
        let db = Db::open_in_memory().unwrap();
        db.insert_request(&request("r1", "running")).unwrap();
        db.insert_request(&request("r2", "queued")).unwrap();
        let j1 = db.enqueue_job("request", None, &serde_json::json!({ "request_id": "r1" })).unwrap();
        let j2 = db.enqueue_job("request", None, &serde_json::json!({ "request_id": "r2" })).unwrap();
        db.with(|c| c.execute("UPDATE jobs SET status='running', attempts=3 WHERE id=?1", [j1])).unwrap();
        db.with(|c| c.execute("UPDATE jobs SET status='running', attempts=1 WHERE id=?1", [j2])).unwrap();
        db.recover_jobs().unwrap();
        assert_eq!(db.get_job(j1).unwrap().unwrap().status, "error");
        assert_eq!(db.get_request("r1").unwrap().unwrap().status, "error");
        assert_eq!(db.get_job(j2).unwrap().unwrap().status, "pending");
        assert_eq!(db.get_request("r2").unwrap().unwrap().status, "queued", "retried job: request untouched");
    }

    #[test]
    fn other_process_heartbeat() {
        let db = Db::open_in_memory().unwrap();
        assert!(!db.other_process_running().unwrap());
        db.set_runtime("app_heartbeat", &std::process::id().to_string()).unwrap();
        assert!(!db.other_process_running().unwrap(), "our own heartbeat");
        db.set_runtime("app_heartbeat", "999999999").unwrap();
        assert!(db.other_process_running().unwrap());
        db.with(|c| c.execute("UPDATE runtime SET updated_at='1970-01-01T00:00:00+00:00' WHERE key='app_heartbeat'", []))
            .unwrap();
        assert!(!db.other_process_running().unwrap(), "stale heartbeat");
    }

    #[test]
    fn heartbeat_has_a_single_owner() {
        let db = Db::open_in_memory().unwrap();
        let owner = |db: &Db| db.get_runtime("app_heartbeat").unwrap().map(|(v, _)| v);
        assert!(db.beat_heartbeat(100).unwrap());
        assert!(db.beat_heartbeat(100).unwrap());
        assert!(!db.beat_heartbeat(200).unwrap(), "fresh heartbeat of another process");
        assert_eq!(owner(&db).as_deref(), Some("100"));
        db.release_heartbeat(200).unwrap();
        assert_eq!(owner(&db).as_deref(), Some("100"), "only the owner releases it");
        db.with(|c| c.execute("UPDATE runtime SET updated_at='2020-01-01T00:00:00+00:00' WHERE key='app_heartbeat'", []))
            .unwrap();
        assert!(db.beat_heartbeat(200).unwrap(), "stale: taken over");
        db.release_heartbeat(200).unwrap();
        assert!(!db.app_is_running().unwrap());
        assert!(db.beat_heartbeat(100).unwrap());
    }

    #[test]
    fn segments_dans_l_ordre_chronologique_et_retranscription() {
        let db = Db::open_in_memory().unwrap();
        db.insert_meeting(&meeting("m1", "Daily")).unwrap();
        let at = |ch: &str, text: &str, t0: Option<f64>| NewSegment { t0, t1: t0.map(|t| t + 1.0), ..seg("m1", ch, text, false) };
        // Une réponse courte de l'appel arrive avant la longue phrase du micro, commencée plus tôt.
        db.insert_segment(&at("them", "Ouais.", Some(35.0))).unwrap();
        let long = db.insert_segment(&at("me", "Longue phrase", Some(2.0))).unwrap();
        db.insert_segment(&at("me", "Sans horodatage", None)).unwrap();
        db.insert_segment(&at("me", "Suite", Some(24.0))).unwrap();
        let order: Vec<String> = db.segments("m1", false, false).unwrap().into_iter().map(|s| s.text).collect();
        assert_eq!(order, ["Longue phrase", "Sans horodatage", "Suite", "Ouais."], "sans horodatage : derrière la phrase insérée avant");
        db.set_segment_text(long.id, "Phrase retranscrite en français").unwrap();
        assert_eq!(db.search("retranscrite", 5).unwrap().len(), 1, "l'index suit le nouveau texte");
        assert!(db.search("Longue", 5).unwrap().is_empty());
    }

    #[test]
    fn versions_de_compte_rendu() {
        let db = Db::open_in_memory().unwrap();
        db.insert_meeting(&meeting("m1", "Daily")).unwrap();
        assert_eq!(db.save_report("m1", "# v1 export Brindille", None, None, false).unwrap(), 1);
        assert_eq!(db.save_report("m1", "# v2 corrélée", None, None, true).unwrap(), 2);
        let r = db.get_report("m1").unwrap().unwrap();
        assert_eq!(r.version, 2);
        assert!(r.correlated);
        assert_eq!(db.report_versions("m1").unwrap().len(), 2);
        assert!(db.get_report_version("m1", 1).unwrap().unwrap().markdown.contains("v1"));
        assert_eq!(db.search("corrélée", 5).unwrap().len(), 1);
        assert!(db.search("Brindille", 5).unwrap().is_empty(), "l'index suit la version courante");
    }

    #[test]
    fn file_de_taches() {
        let db = Db::open_in_memory().unwrap();
        let id = db.enqueue_job("report", Some("m1"), &serde_json::json!({"a":1})).unwrap();
        let j = db.claim_job(&["report"]).unwrap().unwrap();
        assert_eq!(j.id, id);
        assert!(db.claim_job(&["report"]).unwrap().is_none());
        assert_eq!(db.recover_jobs().unwrap(), 1);
        let j = db.claim_job(&["report", "request"]).unwrap().unwrap();
        assert_eq!(j.attempts, 1);
        db.finish_job(j.id, "done", None).unwrap();
        assert!(db.pending_jobs().unwrap().is_empty());
    }

    #[test]
    fn actions_conservent_leur_etat() {
        let db = Db::open_in_memory().unwrap();
        db.insert_meeting(&meeting("m1", "Daily")).unwrap();
        db.replace_decisions_actions(
            "m1",
            &["On garde le verrou".into()],
            &[(Some("Marie".into()), "Corriger APP-1234".into(), None)],
        )
        .unwrap();
        let a = db.actions("m1").unwrap();
        db.set_action_status(a[0].id, "done").unwrap();
        db.replace_decisions_actions(
            "m1",
            &["On garde le verrou".into()],
            &[(Some("Marie".into()), "Corriger APP-1234".into(), None), (None, "Autre".into(), None)],
        )
        .unwrap();
        let a = db.actions("m1").unwrap();
        assert_eq!(a[0].status, "done");
        assert_eq!(a[1].status, "open");
        assert_eq!(db.list_actions(&ActionFilter { status: Some("open".into()), ..Default::default() }).unwrap().len(), 1);
    }

    #[test]
    fn connecteurs_non_bloquables() {
        let db = Db::open_in_memory().unwrap();
        db.sync_connectors("x", &[("atlassian".into(), "connected".into())], false).unwrap();
        let c = &db.list_connectors(Some("x")).unwrap()[0];
        let c = db.update_connector(c.id, true, true).unwrap();
        assert!(!c.use_live && !c.use_correlation);
    }

    #[test]
    fn suppression_en_cascade() {
        let db = Db::open_in_memory().unwrap();
        db.insert_meeting(&meeting("m1", "Daily")).unwrap();
        db.insert_segment(&seg("m1", "me", "bonjour", false)).unwrap();
        db.add_note("m1", "note", "user").unwrap();
        db.delete_meeting("m1").unwrap();
        assert!(db.search("bonjour", 5).unwrap().is_empty());
        assert!(db.notes("m1").unwrap().is_empty());
    }
}
