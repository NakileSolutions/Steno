//! Langue des textes produits par le cœur (notifications, comptes rendus, réponses de l'agent).
//! L'interface a sa propre traduction ; ici, seulement ce que le cœur rédige ou affiche.

use crate::core::Inner;

/// Texte dans la langue des réglages (`fr` ou `en`).
pub fn tr(lang: &str, fr: &str, en: &str) -> String {
    if lang == "en" { en.to_string() } else { fr.to_string() }
}

impl Inner {
    /// Langue effective (`fr` ou `en`).
    pub fn lang(&self) -> &'static str {
        self.settings().lang()
    }

    /// Texte dans la langue des réglages.
    pub fn tr(&self, fr: &str, en: &str) -> String {
        tr(self.lang(), fr, en)
    }
}

/// Consigne de langue ajoutée aux demandes à l'agent (réponses en direct, récapitulatifs,
/// historique, corrélation). Vide en français, langue des consignes.
pub fn answer_language(lang: &str) -> &'static str {
    if lang == "en" {
        "\n\n**Langue** : réponds en anglais (la première ligne reste `TL;DR: …`).\n"
    } else {
        ""
    }
}

/// Consigne de langue et titres du compte rendu en anglais.
pub fn report_language(lang: &str) -> &'static str {
    if lang == "en" {
        "\n\n**Langue** : rédige tout le compte rendu en anglais, avec ces titres à la place des titres \
         français de la procédure : `## Summary`, `## ⚠️ Important changes` (si besoin), `## Decisions`, \
         `## Actions — who / what / due` (puces `- **Who** — what — due`), `## Open questions`, \
         `## Technical points`, `## Searches during the meeting`, `## My notes`. Le bloc ```json``` final ne change pas."
    } else {
        ""
    }
}

/// Titres anglais équivalents d'un titre de section français (pour lire un compte rendu
/// rédigé dans l'une ou l'autre langue).
pub fn heading_aliases(fr: &str) -> &'static [&'static str] {
    match crate::text::normalize(fr).as_str() {
        n if n.starts_with("en bref") => &["Summary", "TL;DR"],
        n if n.starts_with("changements importants") => &["Important changes"],
        n if n.starts_with("decisions") => &["Decisions"],
        n if n.starts_with("actions") => &["Actions"],
        n if n.starts_with("questions ouvertes") => &["Open questions"],
        n if n.starts_with("points techniques") => &["Technical points", "Technical topics"],
        n if n.starts_with("recherches faites") => &["Searches during the meeting", "Searches"],
        n if n.starts_with("mes notes") => &["My notes"],
        n if n.starts_with("liens avec le code") => &["Links to code and tools"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titres_equivalents() {
        assert_eq!(heading_aliases("Décisions"), &["Decisions"]);
        assert_eq!(heading_aliases("Points techniques"), &["Technical points", "Technical topics"]);
        assert!(heading_aliases("Autre").is_empty());
        assert_eq!(tr("en", "Bonjour", "Hello"), "Hello");
        assert_eq!(tr("fr", "Bonjour", "Hello"), "Bonjour");
        assert!(report_language("en").contains("## Summary"));
        assert!(answer_language("fr").is_empty());
    }
}
