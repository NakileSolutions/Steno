//! `scribe init` : assistant de premier lancement en ligne de commande (PLAN §14).

use anyhow::Result;
use scribe_core::Core;
use std::io::Write;

fn ask(q: &str, default: &str) -> String {
    print!("{q}{} ", if default.is_empty() { String::new() } else { format!(" [{default}]") });
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    let s = s.trim().to_string();
    if s.is_empty() {
        default.to_string()
    } else {
        s
    }
}

fn yes(q: &str, default: bool) -> bool {
    let a = ask(&format!("{q} ({})", if default { "O/n" } else { "o/N" }), "").to_lowercase();
    if a.is_empty() {
        default
    } else {
        a.starts_with('o') || a.starts_with('y')
    }
}

pub async fn run(core: &Core) -> Result<()> {
    println!("Bienvenue dans Scribe. Six étapes, toutes modifiables ensuite dans l'app.\n");

    // 1. Permissions
    println!("1/6 — Permissions macOS");
    println!("   Micro, « Enregistrement de l'écran et de l'audio système » et Accessibilité sont");
    println!("   demandés au premier lancement de Scribe.app. Vérification : `scribe doctor`.\n");

    // 2. Agent
    println!("2/6 — Agent IA");
    let agents = core.detect_agents().await;
    let installed: Vec<_> = agents.iter().filter(|a| a.installed && a.id != "custom").collect();
    for (i, a) in agents.iter().enumerate() {
        println!(
            "   {}. {:<24} {:<12} {}",
            i + 1,
            a.name,
            format!("{:?}", a.support).to_lowercase(),
            if a.installed { a.version.clone().unwrap_or_else(|| "installé".into()) } else { "non installé".into() }
        );
    }
    let mut s = core.settings();
    let default = installed.first().map(|a| a.id.clone()).unwrap_or_else(|| s.agent.provider.clone());
    let choice = ask("   Agent à utiliser (identifiant)", &default);
    if agents.iter().any(|a| a.id == choice) {
        s.agent.provider = choice.clone();
    }
    if choice == "custom" {
        s.agent.custom_command = ask("   Modèle de commande ({prompt_file}, {model}, {run_dir})", &s.agent.custom_command);
    }
    s = core.save_settings(s)?;
    if yes("   Tester l'agent maintenant (un petit appel) ?", true) {
        let (ok, msg, ms) = core.test_agent(&s.agent.provider).await?;
        println!("   {} {msg} ({ms} ms)", if ok { "✅" } else { "❌" });
    }
    println!();

    // 3. Dépôts
    println!("3/6 — Dépôts lisibles par l'agent (lecture seule). Laisser vide pour terminer.");
    loop {
        let p = ask("   Chemin d'un dépôt", "");
        if p.is_empty() {
            break;
        }
        let path = std::path::Path::new(&p);
        if !path.is_dir() {
            println!("   ❌ dossier introuvable");
            continue;
        }
        let abs = path.canonicalize()?;
        let label = abs.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let label = ask("   Libellé", &label);
        core.db().add_repo(&abs.display().to_string(), &label)?;
        println!("   ✅ ajouté");
    }
    println!();

    // 4. Connecteurs
    println!("4/6 — Connecteurs (MCP de l'agent)");
    match core.refresh_connectors().await {
        Ok(list) if list.is_empty() => println!("   Aucun connecteur détecté."),
        Ok(list) => {
            for c in list {
                if !c.write_blockable {
                    println!("   ❌ {} : l'agent ne sait pas bloquer ses outils d'écriture, non activable", c.name);
                    continue;
                }
                let on = yes(&format!("   Activer « {} » ({}) en direct et pour la corrélation ?", c.name, c.status), false);
                core.db().update_connector(c.id, on, on)?;
            }
        }
        Err(e) => println!("   ⚠️ {e}"),
    }
    println!();

    // 5. Mot d'appel
    println!("5/6 — Mot d'appel");
    let mut s = core.settings();
    loop {
        let w = ask("   Mot d'appel", &s.voice.wake_word);
        let c = core.check_wake_word(&w, s.voice.strict)?;
        for e in &c.errors {
            println!("   ❌ {e}");
        }
        for wr in &c.warnings {
            println!("   {wr}");
        }
        if c.errors.is_empty() {
            s.voice.wake_word = w.clone();
            let v = core.generate_variants(&w);
            s.voice.variants = v.accepted.clone();
            println!("   Variantes acceptées : {}", v.accepted.join(", "));
            for r in &v.rejected {
                println!("   Variante refusée « {} » : {}", r.variant, r.reason);
            }
            s.voice.strict = yes("   Mode strict (« OK <mot> ») ?", s.voice.strict);
            break;
        }
    }
    println!("   Le test vocal se fait dans l'app : Réglages → Voix → Tester.\n");

    // 6. Corrélation
    println!("6/6 — Corrélation avec la codebase et les connecteurs");
    let m = ask("   Mode : off / on_demand / auto", &s.correlation.mode);
    if ["off", "on_demand", "auto"].contains(&m.as_str()) {
        s.correlation.mode = m;
    }
    core.save_settings(s)?;
    println!("\n✅ Terminé. Lancez `scribe doctor` pour tout vérifier.");
    Ok(())
}
