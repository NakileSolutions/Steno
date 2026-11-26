//! Contrôles à la saisie du mot d'appel (avertissements, pas des blocages).

use super::variants::{builtin_forbidden, common_words};
use crate::db::Db;
use crate::text::{normalize, syllables_fr};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WakeCheck {
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub syllables: usize,
    pub history_count: i64,
    pub meetings_scanned: i64,
}

/// Analyse un mot d'appel. `db` permet l'analyse de l'historique (facultative).
pub fn check_wake_word(word: &str, strict: bool, db: Option<&Db>) -> Result<WakeCheck> {
    let mut c = WakeCheck::default();
    let n = normalize(word);
    if n.is_empty() {
        c.errors.push("Le mot d'appel ne peut pas être vide.".into());
        return Ok(c);
    }
    let forbidden = builtin_forbidden();
    if n.split(' ').any(|w| forbidden.contains(w)) {
        c.errors.push(format!(
            "« {word} » fait partie des mots interdits : il apparaît trop souvent en réunion."
        ));
    }
    c.syllables = syllables_fr(&n);
    if c.syllables < 2 && !strict {
        c.warnings.push(format!(
            "⚠️ « {word} » n'a qu'une syllabe : risque de faux déclenchements. Le mode strict (« OK {word} ») les réduit."
        ));
    }
    let common = common_words();
    if n.split(' ').all(|w| common.contains(w)) {
        c.warnings.push(format!("⚠️ « {word} » est un mot très courant en français : risque de faux déclenchements."));
    }
    if let Some(db) = db {
        // Analyse de l'historique : chaque mot du mot d'appel.
        let last = n.split(' ').next_back().unwrap_or(&n).to_string();
        let (count, scanned) = db.count_word_in_recent(&last, 20)?;
        c.history_count = count;
        c.meetings_scanned = scanned;
        if count >= 3 {
            c.warnings.push(format!(
                "⚠️ Ce mot apparaît {count} fois dans vos {scanned} dernières réunions sans être une commande."
            ));
        }
        let names = db.all_speaker_names()?;
        if let Some(name) = names.iter().find(|nm| normalize(nm).split(' ').any(|p| n.split(' ').any(|w| w == p))) {
            c.warnings.push(format!("⚠️ « {name} » est le nom d'un participant repéré dans vos réunions."));
        }
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NewSegment;
    use crate::model::Meeting;

    #[test]
    fn avertissements() {
        let c = check_wake_word("Scribe", false, None).unwrap();
        assert_eq!(c.syllables, 1);
        assert_eq!(c.warnings.len(), 1);
        assert!(c.errors.is_empty());
        let c = check_wake_word("Script", false, None).unwrap();
        assert!(!c.errors.is_empty());
        let c = check_wake_word("bonjour", false, None).unwrap();
        assert!(c.warnings.iter().any(|w| w.contains("courant")));
        let c = check_wake_word("Greffier", false, None).unwrap();
        assert!(c.warnings.is_empty());
    }

    #[test]
    fn historique_et_prenoms() {
        let db = Db::open_in_memory().unwrap();
        db.insert_meeting(&Meeting {
            id: "m".into(),
            title: None,
            title_source: None,
            started_at: "2026-11-20T10:00:00+01:00".into(),
            ended_at: None,
            status: "done".into(),
            confidential: false,
            agent_provider: None,
            agent_session_id: None,
            correlation: None,
        })
        .unwrap();
        for i in 0..4 {
            db.insert_segment(&NewSegment {
                meeting_id: "m".into(),
                ts: "2026-11-20T10:00:00+01:00".into(),
                t0: Some(i as f64),
                t1: Some(i as f64 + 1.0),
                channel: "them".into(),
                speaker: "Eux".into(),
                text: "Marie a dit que Jarvis tourne".into(),
                private: false,
                is_echo: false,
            })
            .unwrap();
        }
        db.rename_speaker("m", "Eux-1", "Marie").unwrap();
        let c = check_wake_word("Jarvis", false, Some(&db)).unwrap();
        assert_eq!(c.history_count, 4);
        assert!(c.warnings.iter().any(|w| w.contains("4 fois")));
        let c = check_wake_word("Marie", false, Some(&db)).unwrap();
        assert!(c.warnings.iter().any(|w| w.contains("participant")));
    }
}
