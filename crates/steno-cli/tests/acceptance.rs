//! Tests d'acceptation (PLAN §5.6, §6.5, §7.3), sans audio ni abonnement : le sidecar est
//! remplacé par le fichier d'exemple et l'agent par `fixtures/fake-bin/claude`, qui appelle
//! réellement le crochet de politique (`steno policy-hook`).

use steno_core::core::CoreOptions;
use steno_core::live::SidecarSpec;
use steno_core::model::Request;
use steno_core::{Core, Db};
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
        let home = std::env::temp_dir().join(format!("steno-acceptance-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let path = format!("{}:{}", root().join("fixtures/fake-bin").display(), std::env::var("PATH").unwrap_or_default());
        // SAFETY : fixé une seule fois, avant toute lecture concurrente par les tests.
        unsafe {
            std::env::set_var("STENO_HOME", &home);
            std::env::set_var("PATH", path);
            std::env::set_var("FAKE_CLAUDE_LOG", home.join("fake-claude.log"));
            std::env::set_var("FAKE_CLAUDE_PROMPTS", home.join("fake-claude-prompts.log"));
            std::env::set_var("FAKE_AGY_LOG", home.join("fake-agy.log"));
            std::env::set_var("FAKE_GROK_LOG", home.join("fake-grok.log"));
            std::env::set_var("FAKE_CODEX_LOG", home.join("fake-codex.log"));
            // Codex models come from `$CODEX_HOME/config.toml`, never from the real `~/.codex`.
            std::env::set_var("CODEX_HOME", home.join("codex-home"));
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
        CoreOptions { hook_exe: Some(PathBuf::from(env!("CARGO_BIN_EXE_steno"))), sidecar, agent_override: None },
    )
    .unwrap();
    // Compte rendu sans délai de contexte (testé à part dans `delai_de_contexte_avant_la_redaction`).
    let mut s = c.settings();
    s.report.context_delay_s = 0;
    // Langue fixe : les tests ne dépendent pas de la langue de la machine.
    s.profile.language = "fr".into();
    c.save_settings(s).unwrap();
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

/// Rejoue la daily de 45 min : l'écoute est démarrée **manuellement** (jamais automatiquement),
/// puis arrêtée manuellement. Renvoie l'identifiant de la réunion.
async fn replay_fixture(c: &Core) -> String {
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root().join("fixtures/expected.json")).unwrap()).unwrap();
    let n = expected["segments"].as_i64().unwrap();
    wait_until("appel Teams annoncé", 20, || c.live_state().call.active).await;
    assert_eq!(c.live_state().call.app.as_deref(), Some("teams"), "ancien format `teams_call` accepté");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(c.db().live_meeting().unwrap().is_none(), "l'écoute ne doit jamais démarrer seule");
    let m = c.start_meeting(None).await.unwrap();
    assert_eq!(m.title.as_deref(), Some("Daily ORION"), "titre repris de la fenêtre Teams");
    assert_eq!(m.call_app.as_deref(), Some("teams"));
    let id = m.id;
    wait_until("segments reçus", 30, || c.db().segments(&id, true, true).unwrap().len() as i64 == n).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    // La fin de l'appel Teams (dernier événement du fichier) n'arrête pas l'écoute.
    assert_eq!(c.db().get_meeting(&id).unwrap().unwrap().status, "live");
    c.stop_meeting().await.unwrap();
    id
}

/// Simulateur du sidecar (`steno replay`), à vitesse maximale.
fn replay_sidecar(file: &std::path::Path) -> SidecarSpec {
    SidecarSpec {
        program: env!("CARGO_BIN_EXE_steno").into(),
        args: vec!["replay".into(), file.display().to_string(), "--speed".into(), "0".into()],
    }
}

fn cat_sidecar() -> SidecarSpec {
    replay_sidecar(&fixture())
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
    let items = steno_core::doctor::run(&c, true).await;
    let live = items.iter().find(|i| i.name.contains("poste sur Slack")).unwrap();
    assert_eq!(live.status, "ok", "{}", live.message);
    c.stop_meeting().await.unwrap();
    c.shutdown();
}

#[test]
fn mot_d_appel_detection_et_pieges() {
    let _ = env();
    let det = steno_core::wake::Detector::new(&steno_core::settings::VoiceSettings::default());
    let cmds = [
        ("Sténo, cherche les erreurs Datadog sur ms-booking depuis hier.", "search"),
        ("Sténo cherche le ticket APP-1234", "search"),
        ("Sténo, recherche la page Confluence sur l'export Brindille", "search"),
        ("Sténo, regarde les logs de ms-pricing cette nuit", "search"),
        ("Sténo, trouve la MR qui a touché au verrou de créneau", "search"),
        ("Sténo, vérifie que le APP-1302 est bien en recette", "check"),
        ("Sténo, confirme que le monitor de latence est vert", "check"),
        ("Sténo, note que Paul prend la démo", "note"),
        ("Sténo, ajoute une action pour Marie : corriger le APP-1234", "note"),
        ("Sténo, retiens qu'on livre jeudi", "note"),
        ("Sténo, résume", "recap"),
        ("Sténo, récap de la réunion", "recap"),
        ("Sténo, annule", "cancel"),
        ("Sténo, cherche qui a créé le ticket APP-77", "search"),
        ("Bon, Sténo, cherche le runbook de ms-parts", "search"),
        ("OK Sténo, vérifie la config de prod de ms-booking", "check"),
        ("Sténo, tu peux chercher les incidents de la semaine ?", "search"),
        ("Sténo, les erreurs 500 sur le gateway hier soir", "search"),
        ("Sténo, cherches les tickets ouverts sur le module tarification", "search"),
        ("Sténo, note : relancer le support Brindille", "note"),
    ];
    let ok = cmds
        .iter()
        .filter(|(t, i)| det.detect(t).map(|d| d.intent.as_str() == *i).unwrap_or(false))
        .count();
    assert!(ok >= 18, "seulement {ok}/20 commandes détectées");

    // Trap text, words that sound like « Sténo »: 0 triggers.
    let trap = [
        "Le scénario de migration cherche les doublons dans la table.",
        "Sur le tableau, au stylo, on cherche encore la cause.",
        "Le ténor de l'équipe est en formation, cherche pas.",
        "Le scénario tourne en recette depuis hier soir.",
        "La sténographie de la réunion est vide, tu peux la compléter ?",
        "Je me suis mis sur le créneau d'astreinte.",
        "On a branché la stéréo pour la démo, vérifie le son.",
        "Scénario, stylo, sténographie : trois mots pièges.",
        "Le steno de l'équipe, c'est toi cette semaine, cherche les notes.",
        "Tu as noté au stylo hier ? Cherche dedans.",
        "Prends un créneau pour vérifier les scénarios.",
        "Sténographie ça proprement, on vérifie après.",
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
            if !text.starts_with("Sténo,") {
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
    assert!(c.wake_detect("Sténo, cherche le ticket APP-1").is_none());
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn apartes_isoles_de_la_reunion() {
    let c = core("apartes-isoles", None);
    let m = c.start_meeting(Some("Revue budget".into())).await.unwrap();
    let r1 = c.ask("où est géré le verrou de créneau ?", Some(&m.id), "text").unwrap();
    assert_eq!(wait_request(&c, &r1.id).await.status, "done");
    let session = c.db().get_meeting(&m.id).unwrap().unwrap().agent_session_id;
    assert!(session.is_some());

    let mut rx = c.subscribe();
    let r2 = c.ask("où est le salaire de Marie ?", Some(&m.id), "hotkey").unwrap();
    assert_eq!(wait_request(&c, &r2.id).await.status, "done");
    let log = std::fs::read_to_string(env().join("fake-claude.log")).unwrap();
    let mine = format!("/runs/{}/", m.id);
    let last: Vec<String> = serde_json::from_str(log.lines().rfind(|l| l.contains(&mine)).unwrap()).unwrap();
    assert!(last.contains(&"--no-session-persistence".to_string()), "aparté sans la session de la réunion : {last:?}");
    assert!(!last.contains(&"--resume".to_string()));
    assert_eq!(c.db().get_meeting(&m.id).unwrap().unwrap().agent_session_id, session, "session de la réunion intacte");

    // Note dictée en aparté : ni note ni action de la réunion.
    let r3 = c.ask("note que la prime de Marie est de 3000 euros", Some(&m.id), "hotkey").unwrap();
    let r3 = c.db().get_request(&r3.id).unwrap().unwrap();
    assert_eq!(r3.status, "done");
    assert!(r3.summary.unwrap().contains("aparté"));
    assert!(!c.db().notes(&m.id).unwrap().iter().any(|n| n.text.contains("prime")));
    assert!(!c.db().actions(&m.id).unwrap().iter().any(|a| a.text.contains("prime")));

    // Barre de menu : ni apartés ni demandes d'une réunion confidentielle.
    let recent = c.recent_requests(20).unwrap();
    assert!(recent.iter().any(|r| r.id == r1.id));
    assert!(!recent.iter().any(|r| r.origin == "hotkey"), "{recent:?}");
    // Bannières : aucun texte d'aparté.
    while let Ok(ev) = rx.try_recv() {
        if let steno_core::events::CoreEvent::Notify { body, .. } = ev {
            assert!(!body.contains("salaire") && !body.contains("prime"), "texte privé dans une notification : {body}");
        }
    }
    c.stop_meeting().await.unwrap();
    let conf = c.start_meeting(Some("Point RH".into())).await.unwrap();
    c.set_confidential(&conf.id, true).unwrap();
    let r4 = c.ask("cherche le dossier de Paul", Some(&conf.id), "text").unwrap();
    assert!(!c.recent_requests(20).unwrap().iter().any(|r| r.id == r4.id));
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn annulation_et_recap_d_un_aparte_sans_fuite() {
    let c = core("aparte-annulation", None);
    let m = c.start_meeting(Some("Revue primes".into())).await.unwrap();
    let pending = |id: &str, origin: &str, query: &str| Request {
        id: id.into(),
        meeting_id: Some(m.id.clone()),
        created_at: steno_core::now_iso(),
        origin: origin.into(),
        intent: "search".into(),
        query: query.into(),
        context_seconds: None,
        tier: "search".into(),
        status: "queued".into(),
        started_at: None,
        finished_at: None,
        error: None,
        summary: None,
        answer_md: None,
        tools_used: vec![],
    };
    // « Annule » dit publiquement pendant qu'un aparté attend : son texte ne ressort pas.
    c.db().insert_request(&pending("aparte-1", "hotkey", "le salaire de Marie")).unwrap();
    let r = c.ask("annule", Some(&m.id), "voice").unwrap();
    assert_eq!(r.intent, "cancel");
    assert_eq!(r.summary.as_deref(), Some("Demande annulée."));
    assert_eq!(c.db().get_request("aparte-1").unwrap().unwrap().status, "cancelled");
    c.db().insert_request(&pending("publique-1", "voice", "les erreurs Datadog")).unwrap();
    let r = c.ask("annule", Some(&m.id), "voice").unwrap();
    assert!(r.summary.unwrap().contains("les erreurs Datadog"), "demande publique : texte repris");

    // Récapitulatif demandé en aparté : sans la session de la réunion.
    let r = c.ask("récapitule", Some(&m.id), "hotkey").unwrap();
    assert_eq!(r.intent, "recap");
    assert_eq!(wait_request(&c, &r.id).await.status, "done");
    let log = std::fs::read_to_string(env().join("fake-claude.log")).unwrap();
    let mine = format!("/runs/{}/", m.id);
    let last: Vec<String> = serde_json::from_str(log.lines().rfind(|l| l.contains(&mine)).unwrap()).unwrap();
    assert!(last.contains(&"--no-session-persistence".to_string()), "{last:?}");
    assert!(c.db().get_meeting(&m.id).unwrap().unwrap().agent_session_id.is_none(), "aucune session enregistrée pour la réunion");
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn bascule_confidentielle_sans_export_ni_tache() {
    let c = core("bascule-confidentielle", None);
    let out = env().join("exports-confidentiels");
    let mut s = c.settings();
    s.privacy.auto_export_dir = Some(out.display().to_string());
    c.save_settings(s).unwrap();

    // Passée en confidentielle pendant l'écoute : ni compte rendu ni export automatique.
    let m = c.start_meeting(Some("Point salaires".into())).await.unwrap();
    c.add_note(&m.id, "Augmentation de Paul").unwrap();
    c.set_confidential(&m.id, true).unwrap();
    c.stop_meeting().await.unwrap();
    wait_until("réunion terminée", 60, || c.db().get_meeting(&m.id).unwrap().unwrap().status == "done").await;

    // Tâches en attente annulées au passage en confidentielle.
    let m2 = c.start_meeting(Some("Point primes".into())).await.unwrap();
    c.stop_meeting().await.unwrap();
    c.db().enqueue_job_at("export", Some(&m2.id), &serde_json::json!({}), Some("2999-01-01T00:00:00Z")).unwrap();
    assert!(c.db().has_job("export", &m2.id, &["pending"]).unwrap());
    c.set_confidential(&m2.id, true).unwrap();
    assert!(!c.db().has_job("export", &m2.id, &["pending", "running"]).unwrap());
    assert!(c.db().has_job("export", &m2.id, &["cancelled"]).unwrap());
    // Export lancé malgré tout : rien n'est écrit pour une réunion confidentielle.
    c.db().enqueue_job("export", Some(&m2.id), &serde_json::json!({})).unwrap();
    wait_until("export traité", 30, || !c.db().has_job("export", &m2.id, &["pending", "running"]).unwrap()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let written: Vec<_> = std::fs::read_dir(&out).map(|d| d.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
    assert!(written.is_empty(), "export d'une réunion confidentielle : {written:?}");
    assert!(c.db().get_report(&m.id).unwrap().is_none());

    // Compte rendu en préparation : la réunion ne reste pas bloquée « en cours ».
    let m3 = c.start_meeting(Some("Point départs".into())).await.unwrap();
    c.stop_meeting().await.unwrap();
    c.db().set_meeting_status(&m3.id, "processing").unwrap();
    c.set_confidential(&m3.id, true).unwrap();
    assert_eq!(c.db().get_meeting(&m3.id).unwrap().unwrap().status, "done");
    assert!(steno_core::export::auto_export(c.db(), &m3.id, &out).unwrap().is_none());

    // Clic par erreur : repasser en non confidentielle relance le compte rendu annulé.
    let mut s = c.settings();
    s.report.context_delay_s = 600;
    c.save_settings(s).unwrap();
    let m4 = c.start_meeting(Some("Point planning".into())).await.unwrap();
    c.stop_meeting().await.unwrap();
    wait_until("compte rendu planifié", 30, || c.db().has_job("report", &m4.id, &["pending"]).unwrap()).await;
    c.set_confidential(&m4.id, true).unwrap();
    assert!(c.db().has_job("report", &m4.id, &["cancelled"]).unwrap());
    c.set_confidential(&m4.id, false).unwrap();
    assert!(c.db().has_job("report", &m4.id, &["pending", "running", "done", "error"]).unwrap(), "compte rendu relancé");
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn suppression_de_reunion_sans_traversee_de_chemin() {
    let c = core("suppression-traversee", None);
    let marker = env().join("runs").join("temoin.txt");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "x").unwrap();
    for id in ["..", "../..", ".", "../runs", "a/../..", "/tmp", ""] {
        assert!(c.delete_meeting(id).is_err(), "{id:?} doit être refusé");
    }
    assert!(marker.exists() && env().join("steno").parent().unwrap().exists(), "aucun dossier supprimé");
    c.shutdown();
}

/// Dernière invite reçue par le faux agent contenant `needle`.
fn last_prompt_with(needle: &str) -> Option<String> {
    let prompts = std::fs::read_to_string(env().join("fake-claude-prompts.log")).unwrap_or_default();
    prompts.lines().filter_map(|l| serde_json::from_str::<String>(l).ok()).rfind(|p| p.contains(needle))
}

#[tokio::test(flavor = "multi_thread")]
async fn confidentiel_et_apartes_jamais_envoyes_a_l_agent() {
    let c = core("confidentiel-historique", None);
    let segment = |id: &str, text: &str| steno_core::db::NewSegment {
        meeting_id: id.into(),
        ts: steno_core::now_iso(),
        t0: Some(1.0),
        t1: Some(2.0),
        channel: "them".into(),
        speaker: "Eux".into(),
        text: text.into(),
        private: false,
        is_echo: false,
    };
    let request = |mid: &str, origin: &str, query: &str| Request {
        id: format!("{mid}-{origin}"),
        meeting_id: Some(mid.into()),
        created_at: steno_core::now_iso(),
        origin: origin.into(),
        intent: "search".into(),
        query: query.into(),
        context_seconds: None,
        tier: "search".into(),
        status: "done".into(),
        started_at: None,
        finished_at: None,
        error: None,
        summary: Some("trouvé".into()),
        answer_md: None,
        tools_used: vec![],
    };

    // Compte rendu : les demandes posées en aparté n'y figurent pas.
    let m = c.start_meeting(Some("Revue Pégase".into())).await.unwrap();
    c.db().insert_segment(&segment(&m.id, "Le projet Pégase avance, livraison vendredi")).unwrap();
    c.db().insert_request(&request(&m.id, "voice", "où en est la livraison Pégase")).unwrap();
    c.db().insert_request(&request(&m.id, "hotkey", "quel est le salaire de Marie")).unwrap();
    c.stop_meeting().await.unwrap();
    wait_until("compte rendu", 60, || c.db().get_meeting(&m.id).unwrap().unwrap().status == "done").await;
    let p = last_prompt_with("Revue Pégase").expect("invite du compte rendu");
    assert!(p.contains("Compte rendu à rédiger") && p.contains("où en est la livraison Pégase"));
    assert!(!p.contains("salaire de Marie"), "demande en aparté absente du compte rendu");

    // Historique : aucun extrait ni compte rendu d'une réunion confidentielle.
    let conf = c.start_meeting(Some("Point RH Pégase".into())).await.unwrap();
    c.set_confidential(&conf.id, true).unwrap();
    c.db().insert_segment(&segment(&conf.id, "Pégase : rupture conventionnelle de Paul")).unwrap();
    c.stop_meeting().await.unwrap();
    c.db().save_report(&conf.id, "# Point RH\n\nPégase : rupture conventionnelle de Paul", None, None, false).unwrap();
    let r = c.ask_history("que sait-on de Pégase ?", "text").unwrap();
    let r = wait_request(&c, &r.id).await;
    assert_eq!(r.status, "done", "{:?}", r.error);
    let p = last_prompt_with("que sait-on de Pégase").expect("invite de l'historique");
    assert!(p.contains("Revue Pégase"), "{p}");
    assert!(!p.contains("rupture conventionnelle") && !p.contains("Point RH"), "réunion confidentielle envoyée : {p}");
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn appel_teams_propose_sans_demarrer_et_arret_a_la_voix() {
    // Petit rejeu : appel Teams, quelques phrases, puis « Sténo, arrête l'écoute. ».
    let file = env().join("arret-voix.jsonl");
    let lines = [
        r#"{"type":"teams_call","active":true,"title":"Point rapide"}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:03+01:00","t0":1.0,"t1":3.0,"channel":"them","text":"On fait vite aujourd'hui.","private":false}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:06+01:00","t0":4.0,"t1":6.0,"channel":"me","text":"D'accord, on garde le verrou de créneau.","private":false}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:10+01:00","t0":8.0,"t1":10.0,"channel":"me","text":"Sténo, arrête l'écoute.","private":false}"#,
        // 2 s after the stop at speed 4: past the stop command's echo guard (STOP_FLUSH_AFTER).
        r#"{"type":"segment","ts":"2026-11-26T10:00:18+01:00","t0":16.0,"t1":18.0,"channel":"them","text":"Après l'arrêt : ne doit pas être enregistré.","private":false}"#,
    ];
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let c = core("arret-voix", Some(SidecarSpec {
        program: env!("CARGO_BIN_EXE_steno").into(),
        args: vec!["replay".into(), file.display().to_string(), "--speed".into(), "4".into()],
    }));
    let mut rx = c.subscribe();
    wait_until("appel Teams annoncé", 20, || c.live_state().call.active).await;
    // Une notification propose de démarrer ; aucune réunion n'est créée.
    let mut proposed = false;
    while let Ok(ev) = rx.try_recv() {
        if let steno_core::events::CoreEvent::Notify { title, .. } = ev {
            proposed |= title.contains("Appel Teams");
        }
    }
    assert!(proposed, "une notification doit proposer de démarrer l'écoute");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(c.db().live_meeting().unwrap().is_none());

    let m = c.start_meeting(None).await.unwrap();
    assert_eq!(m.title.as_deref(), Some("Point rapide"));
    wait_until("arrêt à la voix", 20, || c.db().get_meeting(&m.id).unwrap().unwrap().status != "live").await;
    let reqs = c.db().requests(&m.id).unwrap();
    assert!(reqs.iter().any(|r| r.intent == "stop" && r.status == "done"), "{reqs:?}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let segs = c.db().segments(&m.id, true, true).unwrap();
    assert!(!segs.iter().any(|s| s.text.contains("ne doit pas")), "rien n'est enregistré après l'arrêt");
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_heard_from_the_loudspeakers_does_not_stop_listening() {
    // A remote participant says « Sténo, arrête l'écoute. »; without hardware echo cancellation
    // the microphone picks it up, and its `them` copy is transcribed a little later.
    let file = env().join("echo-stop.jsonl");
    let lines = [
        r#"{"type":"teams_call","active":true,"title":"Point écho"}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:06+01:00","t0":4.0,"t1":6.0,"channel":"me","text":"D'accord, on garde le verrou de créneau.","private":false}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:10+01:00","t0":8.0,"t1":10.0,"channel":"me","text":"Sténo, arrête l'écoute.","private":false}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:10+01:00","t0":7.9,"t1":10.4,"channel":"them","text":"Sténo, arrête l'écoute.","private":false}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:18+01:00","t0":16.0,"t1":18.0,"channel":"them","text":"On continue, la réunion n'est pas finie.","private":false}"#,
    ];
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let c = core("echo-stop", Some(SidecarSpec {
        program: env!("CARGO_BIN_EXE_steno").into(),
        args: vec!["replay".into(), file.display().to_string(), "--speed".into(), "4".into()],
    }));
    wait_until("appel Teams annoncé", 20, || c.live_state().call.active).await;
    let m = c.start_meeting(None).await.unwrap();
    wait_until("dernière phrase enregistrée", 20, || {
        c.db().segments(&m.id, true, true).unwrap().iter().any(|s| s.text.contains("pas finie"))
    })
    .await;
    assert_eq!(c.db().get_meeting(&m.id).unwrap().unwrap().status, "live", "the echoed stop must not end the meeting");
    let reqs = c.db().requests(&m.id).unwrap();
    assert!(!reqs.iter().any(|r| r.intent == "stop"), "{reqs:?}");
    let segs = c.db().segments(&m.id, true, true).unwrap();
    assert!(segs.iter().any(|s| s.channel == "me" && s.is_echo && s.text.contains("arrête")), "{segs:?}");
    c.stop_meeting().await.unwrap();
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn appel_slack_et_apps_desactivees() {
    // Rejeu : un appel Slack (nouveau format `call`), puis un appel Zoom.
    let file = env().join("slack.jsonl");
    let lines = [
        r#"{"type":"call","active":true,"app":"slack","title":"Huddle dans #equipe-flotte"}"#,
        r#"{"type":"segment","ts":"2026-11-27T10:00:03+01:00","t0":1.0,"t1":3.0,"channel":"them","text":"On regarde le ticket APP-1234 ?","private":false}"#,
        r#"{"type":"segment","ts":"2026-11-27T10:00:06+01:00","t0":4.0,"t1":6.0,"channel":"me","text":"Oui, on garde le verrou de créneau.","private":false}"#,
    ];
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let c = core("slack", Some(replay_sidecar(&file)));
    let mut rx = c.subscribe();
    wait_until("appel Slack annoncé", 20, || c.live_state().call.active).await;
    let st = c.live_state();
    assert_eq!(st.call.app.as_deref(), Some("slack"));
    assert_eq!(st.call.app_name.as_deref(), Some("Slack"));
    assert_eq!(st.call.title.as_deref(), Some("Huddle dans #equipe-flotte"));
    let mut notif = String::new();
    while let Ok(ev) = rx.try_recv() {
        if let steno_core::events::CoreEvent::Notify { title, .. } = ev {
            notif = title;
        }
    }
    assert_eq!(notif, "Appel Slack détecté");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(c.db().live_meeting().unwrap().is_none(), "jamais de démarrage automatique");

    let m = c.start_meeting(None).await.unwrap();
    assert_eq!(m.call_app.as_deref(), Some("slack"));
    assert_eq!(m.title.as_deref(), Some("Huddle dans #equipe-flotte"));
    wait_until("segments", 20, || c.db().segments(&m.id, true, true).unwrap().len() == 2).await;
    c.stop_meeting().await.unwrap();

    // Slack désactivé dans les réglages : l'appel n'est plus proposé.
    let mut s = c.settings();
    s.audio.call_apps.iter_mut().find(|a| a.id == "slack").unwrap().enabled = false;
    c.save_settings(s).unwrap();
    wait_until("appel oublié", 10, || !c.live_state().call.active).await;
    c.shutdown();
}

#[test]
fn liste_des_apps_surveillees() {
    let mut audio = steno_core::settings::AudioSettings::default();
    let ids: Vec<String> = steno_core::live::watch_list(&audio).into_iter().map(|a| a.id).collect();
    // FaceTime n'existe que sur macOS.
    let expected: &[&str] = if cfg!(target_os = "linux") {
        &["teams", "slack", "zoom", "webex", "discord"]
    } else {
        &["teams", "slack", "zoom", "webex", "discord", "facetime"]
    };
    assert_eq!(ids, expected, "navigateur désactivé par défaut");
    audio.call_apps.iter_mut().for_each(|a| a.enabled = a.id == "browser");
    let w = steno_core::live::watch_list(&audio);
    assert_eq!(w.len(), 1);
    assert!(w[0].bundles.iter().any(|b| b == "com.google.Chrome"));
    if cfg!(target_os = "linux") {
        // Linux : nom de l'exécutable (préfixe, sans casse) et identifiants Flatpak.
        assert!(w[0].bundles.iter().any(|b| b == "firefox"));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_modele_et_reflexion_par_usage() {
    use steno_core::settings::TierChoice;
    let c = core("par-usage", None);
    let mut s = c.settings();
    s.agent.tiers.search = TierChoice { agent: "grok".into(), model: "grok-4.7".into(), effort: "medium".into() };
    s.agent.tiers.summary = TierChoice { agent: "antigravity".into(), model: "gemini-4-neon-xhigh".into(), effort: "max".into() };
    c.save_settings(s).unwrap();

    // Listes de modèles lues depuis les CLI (faux) ; suggestions pour Claude Code.
    let agy: Vec<String> = c.list_models("antigravity").await.unwrap().into_iter().map(|m| m.id).collect();
    assert_eq!(agy, ["gemini-4-neon-xhigh", "gemini-3.8-flash-medium"]);
    assert_eq!(c.list_models("grok").await.unwrap()[0].id, "grok-4.7");
    assert!(c.list_models("claude-code").await.unwrap().iter().any(|m| m.id == "opus"));
    let agents = c.detect_agents().await;
    let grok = agents.iter().find(|a| a.id == "grok").unwrap();
    assert!(grok.installed && grok.lists_models && grok.efforts.contains(&"xhigh".to_string()));

    // Recherche → Grok, avec son modèle et sa réflexion.
    let m = c.start_meeting(Some("Point tarifs".into())).await.unwrap();
    let r = c.ask("où est géré le verrou de créneau ?", None, "text").unwrap();
    let r = wait_request(&c, &r.id).await;
    assert_eq!(r.status, "done", "{:?}", r.error);
    assert!(r.answer_md.unwrap().contains("(grok)"));
    let log = std::fs::read_to_string(env().join("fake-grok.log")).unwrap();
    let last: Vec<String> = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    let joined = last.join(" ");
    assert!(joined.contains("-m grok-4.7") && joined.contains("--effort medium"), "{joined}");
    assert!(joined.contains("--disallowed-tools"), "lecture seule : outils d'écriture interdits");

    // Compte rendu → Antigravity, avec son modèle et sa réflexion.
    c.add_note(&m.id, "Marie corrige le APP-1234").unwrap();
    c.stop_meeting().await.unwrap();
    wait_until("compte rendu", 60, || c.db().get_meeting(&m.id).unwrap().unwrap().status == "done").await;
    let rep = c.db().get_report(&m.id).unwrap().unwrap();
    assert!(rep.markdown.contains("Compte rendu factice (agy)"));
    assert_eq!(rep.provider.as_deref(), Some("antigravity"));
    let log = std::fs::read_to_string(env().join("fake-agy.log")).unwrap();
    let last: Vec<String> = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    let joined = last.join(" ");
    assert!(joined.contains("--model gemini-4-neon-xhigh") && joined.contains("--effort max"), "{joined}");
    assert!(c.db().actions(&m.id).unwrap().iter().any(|a| a.owner.as_deref() == Some("Marie")));
    c.shutdown();
}

/// Last argument list logged by the fake `codex` that contains `needle`.
fn last_codex_call(needle: &str) -> String {
    let log = std::fs::read_to_string(env().join("fake-codex.log")).unwrap();
    log.lines()
        .rev()
        .map(|l| serde_json::from_str::<Vec<String>>(l).unwrap().join(" "))
        .find(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no fake codex call with {needle:?} in:\n{log}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn codex_agent_runs_read_only_with_its_model_and_effort() {
    use steno_core::settings::TierChoice;
    let home = env().join("codex-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "model = \"gpt-x\"\n[profiles.fast]\nmodel = \"gpt-y\"\n").unwrap();
    let c = core("codex", None);
    let mut s = c.settings();
    s.agent.tiers.search = TierChoice { agent: "codex".into(), model: "gpt-x".into(), effort: "high".into() };
    s.agent.tiers.summary = TierChoice { agent: "codex".into(), model: "gpt-y".into(), effort: "low".into() };
    c.save_settings(s).unwrap();

    // Detection and models read from the fake CLI and its config.
    let agents = c.detect_agents().await;
    let codex = agents.iter().find(|a| a.id == "codex").unwrap();
    assert!(codex.installed, "{codex:?}");
    let models: Vec<String> = c.list_models("codex").await.unwrap().into_iter().map(|m| m.id).collect();
    assert_eq!(models, ["gpt-x", "gpt-y"]);

    // Search → Codex: read-only sandbox, MCP servers neutralised, model and effort passed.
    let m = c.start_meeting(Some("Point tarifs".into())).await.unwrap();
    let r = c.ask("où est géré le verrou de créneau ?", None, "text").unwrap();
    let r = wait_request(&c, &r.id).await;
    assert_eq!(r.status, "done", "{:?}", r.error);
    assert!(r.answer_md.unwrap().contains("(codex)"));
    let call = last_codex_call("-m gpt-x");
    assert!(call.starts_with("exec "), "{call}");
    assert!(call.contains("--sandbox read-only") && call.contains("mcp_servers={}"), "{call}");
    assert!(call.contains("model_reasoning_effort=\"high\""), "{call}");

    // Report → Codex, parsed from the last message file.
    c.add_note(&m.id, "Marie corrige le APP-1234").unwrap();
    c.stop_meeting().await.unwrap();
    wait_until("compte rendu", 60, || c.db().get_meeting(&m.id).unwrap().unwrap().status == "done").await;
    let rep = c.db().get_report(&m.id).unwrap().unwrap();
    assert!(rep.markdown.contains("Compte rendu factice (codex)"), "{}", rep.markdown);
    assert_eq!(rep.provider.as_deref(), Some("codex"));
    let call = last_codex_call("-m gpt-y");
    assert!(call.contains("model_reasoning_effort=\"low\""), "{call}");
    assert!(c.db().actions(&m.id).unwrap().iter().any(|a| a.owner.as_deref() == Some("Marie")));
    c.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn peripheriques_audio_et_signal() {
    let c = core("peripheriques", Some(cat_sidecar()));
    let mut rx = c.subscribe();
    // Le simulateur annonce ses périphériques après `ready`.
    wait_until("liste des périphériques", 20, || !c.audio_devices().inputs.is_empty()).await;
    let d = c.audio_devices();
    assert_eq!(d.inputs.len(), 3);
    assert!(d.inputs.iter().any(|i| i.default && i.transport == "builtin"));
    assert!(d.outputs.iter().any(|o| o.transport == "bluetooth"));
    // Micro choisi + signal sur une sortie donnée : réglages acceptés et conservés.
    let mut s = c.settings();
    s.audio.input_device = Some("AirPodsPro-UID:input".into());
    s.audio.input_device_name = Some("AirPods Pro".into());
    s.voice.feedback_device = Some("AirPodsPro-UID:output".into());
    let s = c.save_settings(s).unwrap();
    assert_eq!(s.audio.input_device.as_deref(), Some("AirPodsPro-UID:input"));
    // Commande vocale pendant l'écoute → le sidecar joue le signal (pas l'app).
    let id = replay_fixture(&c).await;
    let mut played = None;
    while let Ok(ev) = rx.try_recv() {
        if let steno_core::events::CoreEvent::CommandDetected { played: p } = ev {
            played = Some(p);
        }
    }
    assert_eq!(played, Some(true));
    assert!(!c.db().requests(&id).unwrap().is_empty());
    c.shutdown();
}

/// Phase 2 : contexte, document joint et format de sortie perso arrivent dans la demande de
/// compte rendu ; la régénération produit une nouvelle version.
#[tokio::test(flavor = "multi_thread")]
async fn contexte_documents_et_format_du_compte_rendu() {
    use base64::Engine;
    let c = core("contexte", None);
    let m = steno_core::model::Meeting {
        id: "ctx-1".into(),
        title: Some("Point Nova".into()),
        title_source: Some("user".into()),
        started_at: steno_core::now_iso(),
        ended_at: Some(steno_core::now_iso()),
        status: "done".into(),
        confidential: false,
        agent_provider: None,
        agent_session_id: None,
        correlation: Some("on_demand".into()),
        call_app: None,
    };
    c.db().insert_meeting(&m).unwrap();
    c.db()
        .insert_segment(&steno_core::db::NewSegment {
            meeting_id: m.id.clone(),
            ts: steno_core::now_iso(),
            t0: Some(1.0),
            t1: Some(4.0),
            channel: "them".into(),
            speaker: "Eux".into(),
            text: "On passe la TVA belge à 21 %, c'est validé.".into(),
            private: false,
            is_echo: false,
        })
        .unwrap();

    // Format perso enregistré dans les réglages, réutilisable.
    let mut s = c.settings();
    s.report.templates.push(steno_core::settings::CustomTemplate {
        id: String::new(),
        name: "Note COMEX".into(),
        instructions: "Trois puces maximum, ton exécutif.".into(),
    });
    let s = c.save_settings(s).unwrap();
    let fmt = s.report.templates[0].id.clone();
    assert_eq!(fmt, "perso-note-comex");
    assert!(c.report_templates().iter().any(|t| t.id == "mail" && t.builtin));
    assert!(c.report_templates().iter().any(|t| t.id == fmt && !t.builtin));

    let data = base64::engine::general_purpose::STANDARD.encode("Belgique : TVA à 19 % (règle 2025).\n");
    let f = c.add_context_file(&m.id, "regles-tva.md", &data).unwrap();
    assert_eq!((f.kind.as_str(), f.chars), ("texte", 35));
    assert!(c.add_context_file(&m.id, "photo.png", &data).is_err(), "format refusé");
    let huge = "A".repeat(steno_core::context::MAX_FILE_BYTES / 3 * 4 + 1024);
    let e = c.add_context_file(&m.id, "enorme.md", &huge).unwrap_err().to_string();
    assert!(e.contains("dépasse"), "taille refusée avant le décodage : {e}");
    let ctx = c.set_meeting_context(&m.id, "Réunion Nova : surveiller la TVA.", Some(&fmt)).unwrap();
    assert_eq!(ctx.files.len(), 1);
    assert!(c.set_meeting_context(&m.id, "x", Some("inconnu")).is_err());

    c.regenerate_report(&m.id).unwrap();
    wait_until("compte rendu", 60, || c.db().get_report(&m.id).unwrap().is_some()).await;
    let prompts = std::fs::read_to_string(env().join("fake-claude-prompts.log")).unwrap();
    let last: String = prompts
        .lines()
        .filter_map(|l| serde_json::from_str::<String>(l).ok())
        .filter(|p| p.contains("Point Nova"))
        .last()
        .expect("demande de compte rendu");
    assert!(last.contains("Réunion Nova : surveiller la TVA."));
    assert!(last.contains("Document « regles-tva.md »") && last.contains("TVA à 19 %"));
    assert!(last.contains("Format demandé « Note COMEX »") && last.contains("Trois puces maximum"));
    assert!(last.contains("Changements importants"));

    // Document retiré : il ne figure plus dans la demande suivante.
    c.delete_context_file(f.id).unwrap();
    assert!(c.meeting_context(&m.id).unwrap().files.is_empty());
    c.shutdown();
}

/// Délai laissé après la réunion pour ajouter du contexte : la rédaction attend, une modification
/// la repousse, « Rédiger maintenant » la lance.
#[tokio::test(flavor = "multi_thread")]
async fn delai_de_contexte_avant_la_redaction() {
    let c = core("delai", None);
    let m = steno_core::model::Meeting {
        id: "delai-1".into(),
        title: Some("Point délai".into()),
        title_source: Some("user".into()),
        started_at: steno_core::now_iso(),
        ended_at: Some(steno_core::now_iso()),
        status: "processing".into(),
        confidential: false,
        agent_provider: None,
        agent_session_id: None,
        correlation: Some("on_demand".into()),
        call_app: None,
    };
    c.db().insert_meeting(&m).unwrap();
    c.db()
        .insert_segment(&steno_core::db::NewSegment {
            meeting_id: m.id.clone(),
            ts: steno_core::now_iso(),
            t0: Some(1.0),
            t1: Some(3.0),
            channel: "me".into(),
            speaker: "Moi".into(),
            text: "On valide la refonte.".into(),
            private: false,
            is_echo: false,
        })
        .unwrap();
    let start = steno_core::db::queries_utc_in(20);
    c.db().enqueue_job_at("report", Some(&m.id), &serde_json::json!({}), Some(&start)).unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(c.db().get_report(&m.id).unwrap().is_none(), "la rédaction attend la fin du délai");
    assert_eq!(c.meeting_context(&m.id).unwrap().report_at.as_deref(), Some(start.as_str()));
    // Une modification du contexte repousse la rédaction d'au moins 30 s.
    c.set_meeting_context(&m.id, "Refonte du module de tarification.", None).unwrap();
    let pushed = c.meeting_context(&m.id).unwrap().report_at.unwrap();
    assert!(pushed > start, "{pushed} > {start}");
    // « Rédiger maintenant ».
    c.report_now(&m.id).unwrap();
    assert!(c.meeting_context(&m.id).unwrap().report_at.is_none());
    wait_until("compte rendu", 30, || c.db().get_report(&m.id).unwrap().is_some()).await;
    c.shutdown();
}

/// Phase 3 : réunion prévue avec participants ; l'écoute la reprend, et les participants
/// arrivent dans la demande de compte rendu.
#[tokio::test(flavor = "multi_thread")]
async fn reunion_prevue_reprise_et_participants() {
    let file = env().join("prevue.jsonl");
    let lines = [
        r#"{"type":"teams_call","active":true,"title":"Revue Nova"}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:03+01:00","t0":1.0,"t1":3.0,"channel":"them","text":"Merci Inès pour la démo de la v2.","private":false}"#,
        r#"{"type":"segment","ts":"2026-11-26T10:00:06+01:00","t0":4.0,"t1":6.0,"channel":"me","text":"On valide la sortie de la v2 vendredi.","private":false}"#,
    ];
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let c = core("prevue", Some(replay_sidecar(&file)));
    wait_until("appel annoncé", 20, || c.live_state().call.active).await;

    // Prévue dans 10 min : reprise au démarrage de l'écoute.
    let at = (chrono::Local::now() + chrono::Duration::minutes(10)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let planned = c.plan_meeting("Revue Nova", &at, None).unwrap();
    assert_eq!(planned.status, "planned");
    assert!(c.upcoming(7).await.unwrap().planned.iter().any(|m| m.id == planned.id));
    let people = [("Inès Martin", Some("ines@exemple.fr")), ("Paul Durand", None)]
        .iter()
        .map(|(n, e)| steno_core::planning::Participant { name: n.to_string(), email: e.map(String::from), source: "manual".into() })
        .collect::<Vec<_>>();
    assert_eq!(c.set_participants(&planned.id, &people).unwrap().len(), 2);
    c.set_meeting_context(&planned.id, "Revue de la v2 de Nova.", None).unwrap();

    let m = c.start_meeting(None).await.unwrap();
    assert_eq!(m.id, planned.id, "la réunion prévue est reprise");
    assert_eq!(m.status, "live");
    assert_eq!(m.call_app.as_deref(), Some("teams"));
    wait_until("phrases reçues", 20, || c.db().segments(&m.id, true, true).unwrap().len() == 2).await;
    c.stop_meeting().await.unwrap();
    c.report_now(&m.id).unwrap();
    wait_until("compte rendu", 60, || c.db().get_report(&m.id).unwrap().is_some()).await;
    let prompts = std::fs::read_to_string(env().join("fake-claude-prompts.log")).unwrap();
    let last: String = prompts
        .lines()
        .filter_map(|l| serde_json::from_str::<String>(l).ok())
        .filter(|p| p.contains("Compte rendu à rédiger") && p.contains("Revue Nova"))
        .last()
        .expect("demande de compte rendu");
    assert!(last.contains("## Participants invités") && last.contains("- Inès Martin") && last.contains("- Paul Durand"));
    assert!(last.contains("Revue de la v2 de Nova."));
    // Une réunion qui a démarré n'est plus « à venir ».
    assert!(!c.upcoming(7).await.unwrap().planned.iter().any(|x| x.id == m.id));
    c.shutdown();
}

/// Régression : « Installer » depuis une commande synchrone (fil principal de l'app, sans
/// moteur asynchrone) arrêtait l'app (panique de `tokio::spawn`). Le cœur doit refuser
/// proprement.
#[tokio::test(flavor = "multi_thread")]
async fn installation_hors_moteur_asynchrone_sans_plantage() {
    let c = core("installation", None);
    c.inner().update.lock().unwrap().available = true;
    c.inner().update.lock().unwrap().latest = Some("99.0.0".into());
    let c2 = c.clone();
    let res = std::thread::spawn(move || c2.install_update()).join().expect("pas de panique");
    assert!(res.unwrap_err().to_string().contains("hors du moteur asynchrone"));
    c.shutdown();
}

/// "In-person meeting" from the UI: the `inPerson` argument is camelCase, as the Tauri command
/// (`in_person: Option<bool>`) expects. The HTTP dispatcher must read the same name, otherwise
/// the e2e tests hide a regression in the app.
#[tokio::test(flavor = "multi_thread")]
async fn in_person_meeting_through_dispatcher() {
    let c = core("presentiel", None);
    let m = steno_core::api::dispatch(&c, "start_meeting", serde_json::json!({ "inPerson": true })).await.unwrap();
    assert_eq!(m["call_app"], "room", "{m}");
    c.stop_meeting().await.unwrap();
    c.shutdown();
}
