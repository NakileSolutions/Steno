//! Tests d'acceptation (PLAN §5.6, §6.5, §7.3), sans audio ni abonnement : le sidecar est
//! remplacé par le fichier d'exemple et l'agent par `fixtures/fake-bin/claude`, qui appelle
//! réellement le crochet de politique (`scribe policy-hook`).

use scribe_core::core::CoreOptions;
use scribe_core::live::SidecarSpec;
use scribe_core::model::Request;
use scribe_core::{Core, Db};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn fixture() -> PathBuf {
    root().join("fixtures/daily-45min.jsonl")
}

/// Environnement commun : dossier des données, faux agent dans le PATH.
fn env() -> &'static PathBuf {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let home = std::env::temp_dir().join(format!("scribe-acceptance-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let path = format!("{}:{}", root().join("fixtures/fake-bin").display(), std::env::var("PATH").unwrap_or_default());
        // SAFETY : fixé une seule fois, avant toute lecture concurrente par les tests.
        unsafe {
            std::env::set_var("SCRIBE_HOME", &home);
            std::env::set_var("PATH", path);
            std::env::set_var("FAKE_CLAUDE_LOG", home.join("fake-claude.log"));
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
        home
    })
}

fn core(name: &str, sidecar: Option<SidecarSpec>) -> Core {
    let home = env();
    let db = Db::open(&home.join(format!("{name}.db"))).unwrap();
    let c = Core::new(
        db,
        CoreOptions { hook_exe: Some(PathBuf::from(env!("CARGO_BIN_EXE_scribe"))), sidecar, agent_override: None },
    )
    .unwrap();
    c.start_background();
    c
}

async fn wait_request(c: &Core, id: &str) -> Request {
    let t = Instant::now();
    loop {
        let r = c.db().get_request(id).unwrap().unwrap();
        if !matches!(r.status.as_str(), "queued" | "running") {
            return r;
        }
        assert!(t.elapsed() < Duration::from_secs(90), "demande trop longue (> 90 s)");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_until(what: &str, secs: u64, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(secs), "délai dépassé : {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Rejoue la daily de 45 min dans la session en direct et renvoie l'identifiant de la réunion.
async fn replay_fixture(c: &Core) -> String {
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root().join("fixtures/expected.json")).unwrap()).unwrap();
    let n = expected["segments"].as_i64().unwrap();
    let mut id = None;
    wait_until("réunion créée", 20, || {
        id = c.db().live_meeting().unwrap().map(|m| m.id);
        id.is_some()
    })
    .await;
    let id = id.unwrap();
    wait_until("segments reçus", 30, || c.db().segments(&id, true, true).unwrap().len() as i64 == n).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    c.stop_meeting().await.unwrap();
    id
}

fn cat_sidecar() -> SidecarSpec {
    SidecarSpec {
        program: "sh".into(),
        args: vec!["-c".into(), format!("cat '{}'; sleep 600", fixture().display())],
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn demande_texte_sources_session_et_refus() {
    let c = core("demandes", None);
    let repo = env().join("depot");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/SlotLock.kt"), "class SlotLock").unwrap();
    c.db().add_repo(&repo.display().to_string(), "depot").unwrap();
    let cons = c.refresh_connectors().await.unwrap();
    assert_eq!(cons.len(), 3);
    for con in &cons {
        // On active tout, y compris Slack : la politique doit bloquer l'écriture.
        c.db().update_connector(con.id, true, true).unwrap();
    }
    let m = c.start_meeting(Some("Daily ORION".into())).await.unwrap();

    // §5.6 : réponse sourcée en moins de 90 s.
    let t = Instant::now();
    let r = c.ask("où est géré le verrou de créneau et quel ticket en parle ?", None, "text").unwrap();
    let r = wait_request(&c, &r.id).await;
    assert!(t.elapsed() < Duration::from_secs(90));
    assert_eq!(r.status, "done", "{:?}", r.error);
    let answer = r.answer_md.clone().unwrap();
    assert!(regex::Regex::new(r"APP-\d+").unwrap().is_match(&answer));
    assert!(regex::Regex::new(r"[\w/]+\.\w+:\d+").unwrap().is_match(&answer));
    assert!(r.tools_used.iter().any(|t| t.contains("Jira")), "{:?}", r.tools_used);
    // Aucun outil d'écriture exécuté (Slack, git push refusés par le crochet).
    assert!(!r.tools_used.iter().any(|t| t.contains("send")), "{:?}", r.tools_used);
    assert_eq!(r.tools_used.iter().filter(|t| *t == "Bash").count(), 1, "seul `rg` passe : {:?}", r.tools_used);
    assert!(r.summary.unwrap().contains("SlotLock"));

    // Une relance réutilise la session de la réunion.
    let s1 = c.db().get_meeting(&m.id).unwrap().unwrap().agent_session_id.unwrap();
    let r2 = c.ask("et dans quel fichier exactement ?", None, "text").unwrap();
    wait_request(&c, &r2.id).await;
    let s2 = c.db().get_meeting(&m.id).unwrap().unwrap().agent_session_id.unwrap();
    assert_eq!(s1, s2);
    let log = std::fs::read_to_string(env().join("fake-claude.log")).unwrap();
    assert!(log.contains(&format!("\"--resume\", \"{s1}\"")));

    // « Poste ça sur Slack » est refusé, sans appel d'outil.
    let r3 = c.ask("Poste ça sur Slack", None, "text").unwrap();
    assert_eq!(r3.status, "refused");
    assert!(r3.tools_used.is_empty());

    // Test réel de la politique par `doctor`.
    let items = scribe_core::doctor::run(&c, true).await;
    let live = items.iter().find(|i| i.name.contains("poste sur Slack")).unwrap();
    assert_eq!(live.status, "ok", "{}", live.message);
    c.stop_meeting().await.unwrap();
    c.shutdown();
}

#[test]
fn mot_d_appel_detection_et_pieges() {
    let _ = env();
    let det = scribe_core::wake::Detector::new(&scribe_core::settings::VoiceSettings::default());
    let cmds = [
        ("Scribe, cherche les erreurs Datadog sur ms-booking depuis hier.", "search"),
        ("Scribe cherche le ticket APP-1234", "search"),
        ("Scribe, recherche la page Confluence sur l'export Brindille", "search"),
        ("Scribe, regarde les logs de ms-pricing cette nuit", "search"),
        ("Scribe, trouve la MR qui a touché au verrou de créneau", "search"),
        ("Scribe, vérifie que le APP-1302 est bien en recette", "check"),
        ("Scribe, confirme que le monitor de latence est vert", "check"),
        ("Scribe, note que Paul prend la démo", "note"),
        ("Scribe, ajoute une action pour Marie : corriger le APP-1234", "note"),
        ("Scribe, retiens qu'on livre jeudi", "note"),
        ("Scribe, résume", "recap"),
        ("Scribe, récap de la réunion", "recap"),
        ("Scribe, annule", "cancel"),
        ("Scribe, cherche qui a créé le ticket APP-77", "search"),
        ("Bon, Scribe, cherche le runbook de ms-parts", "search"),
        ("OK Scribe, vérifie la config de prod de ms-booking", "check"),
        ("Scribe, tu peux chercher les incidents de la semaine ?", "search"),
        ("Scribe, les erreurs 500 sur le gateway hier soir", "search"),
        ("Scribe, cherches les tickets ouverts sur le module tarification", "search"),
        ("Scribe, note : relancer le support Brindille", "note"),
    ];
    let ok = cmds
        .iter()
        .filter(|(t, i)| det.detect(t).map(|d| d.intent.as_str() == *i).unwrap_or(false))
        .count();
    assert!(ok >= 18, "seulement {ok}/20 commandes détectées");

    // Texte piège : 0 déclenchement.
    let trap = [
        "Le script de migration cherche les doublons dans la table.",
        "Dans la description du ticket, on cherche encore la cause.",
        "Il s'est inscrit à la formation, cherche pas.",
        "Le script tourne en recette depuis hier soir.",
        "La description de la MR est vide, tu peux la compléter ?",
        "Je me suis inscrit sur le canal d'astreinte.",
        "On prescrit une revue de code systématique.",
        "Script, description, inscrit : trois mots pièges.",
        "Le scribe de l'équipe, c'est toi cette semaine, cherche les notes.",
        "Tu as lu la transcription d'hier ? Cherche dedans.",
        "Écris un script pour vérifier les créneau.",
        "Scripte ça proprement, on vérifie après.",
    ];
    for t in trap {
        assert!(det.detect(t).is_none(), "faux déclenchement : {t}");
    }
    // Toutes les phrases de la daily dites par les participants : 0 déclenchement.
    for line in std::fs::read_to_string(fixture()).unwrap().lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["channel"] == "me" {
            continue;
        }
        if let Some(text) = v["text"].as_str() {
            // (le canal `them` n'est jamais analysé ; on vérifie ici que les mots pièges ne
            // déclencheraient pas même s'ils l'étaient, sauf la commande volontaire)
            if !text.starts_with("Scribe,") {
                assert!(det.detect(text).is_none(), "faux déclenchement : {text}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn daily_complete_compte_rendu_et_correlation() {
    let c = core("daily", Some(cat_sidecar()));
    let repo = env().join("depot2");
    std::fs::create_dir_all(&repo).unwrap();
    c.db().add_repo(&repo.display().to_string(), "depot2").unwrap();
    for con in c.refresh_connectors().await.unwrap() {
        c.db().update_connector(con.id, true, true).unwrap();
    }
    let t = Instant::now();
    let id = replay_fixture(&c).await;

    // §6.5 : 2 commandes (la commande dite par un participant et l'écho ne déclenchent rien).
    let reqs = c.db().requests(&id).unwrap();
    assert_eq!(reqs.len(), 2, "{reqs:?}");
    assert!(reqs.iter().any(|r| r.intent == "search" && r.query.contains("erreurs Datadog sur ms-booking")));
    assert!(reqs.iter().any(|r| r.intent == "note" && r.query.contains("Marie relance le support Brindille")));
    let segs = c.db().segments(&id, true, true).unwrap();
    assert_eq!(segs.iter().filter(|s| s.is_echo).count(), 1);
    assert_eq!(segs.iter().filter(|s| s.private).count(), 2);

    // §7.3 : compte rendu en moins de 2 min, décision, actions attribuées, rien de privé.
    wait_until("compte rendu", 120, || c.db().get_meeting(&id).unwrap().unwrap().status == "done").await;
    assert!(t.elapsed() < Duration::from_secs(120));
    let report = c.db().get_report(&id).unwrap().unwrap();
    assert!(!report.markdown.contains("entretien annuel"));
    assert!(!report.markdown.contains("```json"));
    let dec = c.db().decisions(&id).unwrap();
    assert!(dec.iter().any(|d| d.text.contains("verrou de créneau")));
    let actions = c.db().actions(&id).unwrap();
    assert!(actions.iter().any(|a| a.owner.as_deref() == Some("Marie") && a.text.contains("APP-1234")));
    assert!(actions.len() >= 2);

    // Corrélation « à la demande » (défaut) : aucune tâche sans clic.
    assert!(!c.db().has_job("correlate", &id, &["pending", "running", "done", "error"]).unwrap());
    c.correlate(&id).unwrap();
    wait_until("corrélation", 60, || c.db().get_report(&id).unwrap().unwrap().version == 2).await;
    let r2 = c.db().get_report(&id).unwrap().unwrap();
    assert!(r2.correlated);
    assert!(r2.markdown.contains("## Liens avec le code et les outils"));
    assert_eq!(c.db().refs(&id).unwrap().len(), 2);
    assert!(c.db().get_report_version(&id, 1).unwrap().is_some(), "version précédente consultable");

    // Corrélation désactivée : refusée, aucune tâche.
    let mut s = c.settings();
    s.correlation.mode = "off".into();
    c.save_settings(s).unwrap();
    assert!(c.correlate(&id).is_err());
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn exclusions_et_confidentialite() {
    let c = core("exclusions", None);
    let mut s = c.settings();
    s.correlation.mode = "auto".into();
    c.save_settings(s).unwrap();

    // Réunion « 1:1 » : compte rendu oui, corrélation automatique non.
    let m = c.start_meeting(Some("1:1 Alex / Sam".into())).await.unwrap();
    c.add_note(&m.id, "Point sur le verrou de créneau").unwrap();
    c.stop_meeting().await.unwrap();
    wait_until("compte rendu 1:1", 60, || c.db().get_meeting(&m.id).unwrap().unwrap().status == "done").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!c.db().has_job("correlate", &m.id, &["pending", "running", "done", "error"]).unwrap());

    // Réunion normale en mode auto : corrélation lancée après le compte rendu.
    let m2 = c.start_meeting(Some("Daily".into())).await.unwrap();
    c.add_note(&m2.id, "Erreurs sur ms-booking").unwrap();
    c.stop_meeting().await.unwrap();
    wait_until("corrélation auto", 60, || {
        c.db().get_report(&m2.id).unwrap().map(|r| r.correlated).unwrap_or(false)
    })
    .await;

    // Réunion confidentielle : aucun appel IA.
    c.set_live_confidential(true).unwrap();
    let m3 = c.start_meeting(Some("Daily".into())).await.unwrap();
    assert!(c.db().get_meeting(&m3.id).unwrap().unwrap().confidential);
    let r = c.ask("cherche les erreurs", None, "text").unwrap();
    assert_eq!(r.status, "refused");
    c.stop_meeting().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(c.db().get_report(&m3.id).unwrap().is_none());
    assert_eq!(c.db().get_meeting(&m3.id).unwrap().unwrap().status, "done");

    // Changer le mot d'appel prend effet sans redémarrage.
    let mut s = c.settings();
    s.voice.wake_word = "Greffier".into();
    c.save_settings(s).unwrap();
    assert!(c.wake_detect("Greffier, cherche le ticket APP-1").is_some());
    assert!(c.wake_detect("Scribe, cherche le ticket APP-1").is_none());
    c.shutdown();
}
