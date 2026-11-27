//! Consignes de l'agent : une seule source (`agent/INSTRUCTIONS.md` + `agent/playbooks/`),
//! complétée par le profil du dev, puis écrite au format de chaque CLI par l'adaptateur.
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

/// Assemble les consignes complètes (consignes + procédures).
pub fn build(s: &Settings) -> Instructions {
    let mut md = read_override("INSTRUCTIONS.md", INSTRUCTIONS);
    for (name, builtin) in PLAYBOOKS {
        md.push_str("\n\n---\n\n");
        md.push_str(&read_override(&format!("playbooks/{name}.md"), builtin));
    }
    let md = md
        .replace("{{PROFIL}}", &profile_text(s))
        .replace("{{JIRA}}", &s.profile.jira_key_pattern);
    Instructions { markdown: md }
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
        assert!(i.markdown.contains("recherche croisée"));
        assert!(!i.markdown.contains("{{"));
    }
}
