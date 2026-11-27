//! Variantes du mot d'appel : génération automatique (variantes phonétiques, erreurs de
//! transcription courantes) et filtrage par les mots interdits (PLAN §6.2).

use crate::text::{levenshtein, normalize};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

const FORBIDDEN: &str = include_str!("forbidden_fr.txt");
const COMMON: &str = include_str!("common_fr.txt");

fn word_list(src: &str) -> BTreeSet<String> {
    src.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .flat_map(|l| l.split_whitespace())
        .map(normalize)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Mots interdits intégrés.
pub fn builtin_forbidden() -> BTreeSet<String> {
    word_list(FORBIDDEN)
}

/// Mots très courants en français.
pub fn common_words() -> BTreeSet<String> {
    word_list(COMMON)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Rejected {
    pub variant: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct VariantCheck {
    pub accepted: Vec<String>,
    pub rejected: Vec<Rejected>,
}

fn is_vowel(c: char) -> bool {
    "aeiouy".contains(c)
}

/// Variantes plausibles d'un seul mot normalisé.
fn word_variants(w: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let chars: Vec<char> = w.chars().collect();
    if chars.len() < 3 {
        return out;
    }
    let mut push = |s: String| {
        if s != w && s.len() >= 3 && !out.contains(&s) {
            out.push(s);
        }
    };
    // pluriel / e muet
    push(format!("{w}s"));
    if let Some(s) = w.strip_suffix('e') {
        push(s.to_string());
        push(format!("{s}es"));
    } else if !is_vowel(*chars.last().unwrap()) {
        push(format!("{w}e"));
    }
    // c ↔ k, qu → k, i ↔ y, ph ↔ f
    for (a, b) in [("c", "k"), ("k", "c"), ("qu", "k"), ("i", "y"), ("y", "i"), ("ph", "f"), ("f", "ph")] {
        if w.contains(a) {
            push(w.replacen(a, b, 1));
        }
    }
    // assourdissement / sonorisation de la dernière consonne prononcée
    let base = w.strip_suffix('e').unwrap_or(w);
    let bchars: Vec<char> = base.chars().collect();
    if let Some(&last) = bchars.last() {
        let swap = match last {
            'b' => Some('p'),
            'p' => Some('b'),
            'd' => Some('t'),
            't' => Some('d'),
            'g' => Some('k'),
            'v' => Some('f'),
            'z' => Some('s'),
            _ => None,
        };
        if let Some(s) = swap {
            let mut nb: String = bchars[..bchars.len() - 1].iter().collect();
            nb.push(s);
            let suffix = if w.ends_with('e') { "e" } else { "" };
            push(format!("{nb}{suffix}"));
        }
    }
    // « s + consonne » en tête : l'ASR ajoute souvent un e (« escribe »)
    if chars[0] == 's' && chars.len() > 1 && !is_vowel(chars[1]) {
        push(format!("e{w}"));
    }
    // lettre doublée simplifiée
    for i in 1..chars.len() {
        if chars[i] == chars[i - 1] {
            let mut s = chars.clone();
            s.remove(i);
            push(s.into_iter().collect());
            break;
        }
    }
    out
}

/// Génère les variantes d'un mot d'appel (éventuellement de plusieurs mots : seule la
/// dernière partie varie).
pub fn generate(wake_word: &str) -> Vec<String> {
    let n = normalize(wake_word);
    let parts: Vec<&str> = n.split(' ').collect();
    let Some((last, head)) = parts.split_last() else { return vec![] };
    let head = head.join(" ");
    word_variants(last)
        .into_iter()
        .map(|v| if head.is_empty() { v } else { format!("{head} {v}") })
        .collect()
}

/// Raison du refus d'une variante, ou `None` si elle est acceptable.
pub fn conflict(wake_word: &str, variant: &str, team_vocabulary: &[String]) -> Option<String> {
    let v = normalize(variant);
    if v.is_empty() {
        return Some("variante vide".into());
    }
    if v == normalize(wake_word) {
        return Some("identique au mot d'appel".into());
    }
    let forbidden = builtin_forbidden();
    for w in v.split(' ') {
        if forbidden.contains(w) {
            return Some(format!("« {w} » est un mot interdit (trop fréquent en réunion)"));
        }
        if w.chars().count() >= 4 {
            if let Some(f) = forbidden.iter().find(|f| f.chars().count() >= 4 && levenshtein(w, f) <= 1) {
                return Some(format!("trop proche du mot interdit « {f} »"));
            }
        }
        if common_words().contains(w) {
            return Some(format!("« {w} » est un mot courant en français"));
        }
    }
    for t in team_vocabulary {
        let tn = normalize(t);
        if tn.is_empty() {
            continue;
        }
        if tn == v || (v.chars().count() >= 4 && levenshtein(&tn, &v) <= 1) {
            return Some(format!("en conflit avec le vocabulaire de l'équipe (« {t} »)"));
        }
    }
    None
}

/// Filtre une liste de variantes.
pub fn check(wake_word: &str, variants: &[String], team_vocabulary: &[String]) -> VariantCheck {
    let mut out = VariantCheck::default();
    let mut seen = BTreeSet::new();
    for v in variants {
        let n = normalize(v);
        if !seen.insert(n.clone()) {
            continue;
        }
        match conflict(wake_word, v, team_vocabulary) {
            None => out.accepted.push(n),
            Some(reason) => out.rejected.push(Rejected { variant: n, reason }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variantes_de_scribe() {
        let g = generate("Scribe");
        for v in ["scribes", "scrib", "skribe", "scrybe", "scripe", "escribe"] {
            assert!(g.contains(&v.to_string()), "{v} manquant dans {g:?}");
        }
        let c = check("Scribe", &g, &[]);
        assert!(c.accepted.contains(&"scrib".to_string()));
        assert!(c.accepted.contains(&"escribe".to_string()));
        let rej: Vec<_> = c.rejected.iter().map(|r| r.variant.as_str()).collect();
        assert!(rej.contains(&"scripe"), "scripe est à 1 lettre de « script » : {c:?}");
    }

    #[test]
    fn mots_interdits() {
        assert!(conflict("Scribe", "script", &[]).is_some());
        assert!(conflict("Scribe", "description", &[]).is_some());
        assert!(conflict("Scribe", "inscrit", &[]).is_some());
        assert!(conflict("Scribe", "bonjour", &[]).is_some());
        assert!(conflict("Scribe", "scrib", &[]).is_none());
        assert!(conflict("Scribe", "scribo", &["Scriba".into()]).is_some());
    }

    #[test]
    fn mot_compose() {
        let g = generate("Hey Greffier");
        assert!(g.iter().all(|v| v.starts_with("hey ")));
        assert!(g.contains(&"hey greffiers".to_string()));
    }
}
