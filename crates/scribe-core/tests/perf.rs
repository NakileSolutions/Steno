//! Critère de performance (PLAN §2) : recherche plein texte sur 500 réunions < 200 ms.
//! Lancement : `cargo test -p scribe-core --release --test perf -- --ignored --nocapture`.

use scribe_core::db::NewSegment;
use scribe_core::model::{Meeting, MeetingFilter};
use scribe_core::Db;
use std::time::Instant;

#[test]
#[ignore]
fn recherche_sur_500_reunions() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("perf.db")).unwrap();
    let words = [
        "verrou", "créneau", "ms-booking", "export", "Brindille", "saga", "gateway", "recette", "latence", "batch",
        "monitor", "ticket", "migration", "pagination", "timeout", "client", "location", "station", "sprint", "démo",
    ];
    let mut seed = 7u64;
    let mut rnd = |n: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % n
    };
    let t = Instant::now();
    for m in 0..500 {
        let id = format!("m{m:03}");
        db.insert_meeting(&Meeting {
            id: id.clone(),
            title: Some(format!("Réunion {m}")),
            title_source: None,
            started_at: format!("2026-{:02}-{:02}T10:00:00+01:00", 1 + m % 12, 1 + m % 28),
            ended_at: None,
            status: "done".into(),
            confidential: false,
            agent_provider: None,
            agent_session_id: None,
            correlation: None,
            call_app: None,
        })
        .unwrap();
        for s in 0..300 {
            // Vocabulaire réaliste : 3 000 mots courants + un terme métier par phrase.
            let mut text: Vec<String> = (0..11).map(|_| format!("mot{}", rnd(3000))).collect();
            text.insert(rnd(11), words[rnd(words.len())].to_string());
            db.insert_segment(&NewSegment {
                meeting_id: id.clone(),
                ts: "2026-01-01T10:00:00+01:00".into(),
                t0: Some(s as f64),
                t1: Some(s as f64 + 2.0),
                channel: if s % 2 == 0 { "me".into() } else { "them".into() },
                speaker: "Eux".into(),
                text: format!("Phrase {s} : {}", text.join(" ")),
                private: false,
                is_echo: false,
            })
            .unwrap();
        }
        db.save_report(&id, &format!("# Réunion {m}\n## Décisions\n- {}", words[m % 20]), None, None, false).unwrap();
    }
    eprintln!("remplissage : {:?} (150 000 phrases)", t.elapsed());
    for q in ["verrou de créneau", "Brindille", "ms-booking timeout", "pagination batch recette"] {
        let t = Instant::now();
        let hits = db.search(q, 100).unwrap();
        let d1 = t.elapsed();
        let t = Instant::now();
        let list = db.list_meetings(&MeetingFilter { query: Some(q.into()), ..Default::default() }).unwrap();
        let d2 = t.elapsed();
        eprintln!("« {q} » : search {} résultats en {d1:?} ; liste filtrée {} réunions en {d2:?}", hits.len(), list.len());
        assert!(d1.as_millis() < 200, "recherche trop lente : {d1:?}");
        assert!(d2.as_millis() < 200, "liste filtrée trop lente : {d2:?}");
    }
}
