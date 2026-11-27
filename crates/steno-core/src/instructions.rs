//! Consignes de l'agent : une seule source (`agent/INSTRUCTIONS.md` + `agent/playbooks/`),
//! complétée par le profil du dev. Les consignes communes sont écrites au format de chaque CLI
//! par l'adaptateur ; chaque procédure est jointe à la demande qui en a besoin.
//! Un dossier `agent/` dans le dossier des données remplace les fichiers intégrés.

use crate::agents::Instructions;
use crate::settings::Settings;

const INSTRUCTIONS: &str = include_str!("../../../agent/INSTRUCTIONS.md");
const PLAYBOOKS: &[(&str, &str)] = &[
    ("recherche-croisee", include_str!("../../../agent/playbooks/recherche-croisee.md")),
    ("note", include_str!("../../../agent/playbooks/note.md")),
    ("compte-rendu", include_str!("../../../agent/playbooks/compte-rendu.md")),
    ("correlation", include_str!("../../../agent/playbooks/correlation.md")),
];

fn read_override(rel: &str, builtin: &str) -> String {
    let p = crate::paths::home().join("agent").join(rel);
    std::fs::read_to_string(p).unwrap_or_else(|_| builtin.to_string())
}

fn profile_text(s: &Settings) -> String {
    let p = &s.profile;
    let mut parts = vec![];
    if !p.name.trim().is_empty() {
        parts.push(p.name.trim().to_string());
    }
    if !p.role.trim().is_empty() {
        parts.push(p.role.trim().to_string());
    }
    if !p.team.trim().is_empty() {
        parts.push(format!("équipe {}", p.team.trim()));
    }
    if parts.is_empty() {
        "un développeur".into()
    } else {
        parts.join(", ")
    }
}

fn fill(md: String, s: &Settings) -> String {
    md.replace("{{PROFIL}}", &profile_text(s)).replace("{{JIRA}}", &s.profile.jira_key_pattern)
}

/// Consignes communes à toutes les demandes (fichier de consignes du CLI). Les procédures
/// n'y figurent pas : chaque demande joint seulement celle dont elle a besoin (voir `expand`).
pub fn build(s: &Settings) -> Instructions {
    Instructions { markdown: fill(read_override("INSTRUCTIONS.md", INSTRUCTIONS), s) }
}

/// Marqueur à placer dans une demande ; `expand` le remplace par la procédure.
pub fn marker(name: &str) -> String {
    format!("{{{{PROCEDURE:{name}}}}}")
}

/// Remplace les marqueurs `{{PROCEDURE:<nom>}}` d'une demande par le texte des procédures
/// (fichiers intégrés ou remplacés dans le dossier des données). Seules les procédures
/// connues sont remplacées : le texte d'une transcription ne peut rien injecter d'autre.
pub fn expand(prompt: &str, s: &Settings) -> String {
    let mut out = prompt.to_string();
    for (name, builtin) in PLAYBOOKS {
        let m = marker(name);
        if out.contains(&m) {
            let text = fill(read_override(&format!("playbooks/{name}.md"), builtin), s);
            out = out.replacen(&m, text.trim(), 1).replace(&m, "");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn consignes() {
        let mut s = crate::settings::Settings::default();
        s.profile.role = "dev backend".into();
        s.profile.team = "Flotte".into();
        let i = super::build(&s);
        assert!(i.markdown.contains("dev backend, équipe Flotte"));
        assert!(!i.markdown.contains("{{"));
        // Les procédures ne sont plus dans les consignes communes.
        assert!(!i.markdown.contains("Procédure : recherche croisée"));
    }

    #[test]
    fn procedures_a_la_demande() {
        let mut s = crate::settings::Settings::default();
        s.profile.jira_key_pattern = "NOVA-\\d+".into();
        let p = format!("Demande\n{}\nfin {}", super::marker("recherche-croisee"), super::marker("recherche-croisee"));
        let out = super::expand(&p, &s);
        assert!(out.contains("Procédure : recherche croisée") && out.contains("NOVA-\\d+"));
        assert_eq!(out.matches("Procédure : recherche croisée").count(), 1);
        assert!(!out.contains("{{"));
        assert!(!out.contains("compte rendu de fin de réunion"));
        // Marqueur inconnu : laissé tel quel.
        assert_eq!(super::expand("{{PROCEDURE:autre}}", &s), "{{PROCEDURE:autre}}");
    }
}
