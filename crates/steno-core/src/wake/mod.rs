//! Mot d'appel : détection des commandes vocales (PLAN §6).
//!
//! - `Detector` : reconnaît « <mot d'appel> <verbe> <demande> » dans un énoncé ;
//! - `assembler` : fusionne les énoncés qui se suivent pour former une commande ;
//! - `variants` : génère et filtre les variantes acceptées ;
//! - `check` : contrôles à la saisie du mot d'appel.

pub mod assembler;
pub mod check;
pub mod variants;

use crate::model::Intent;
use crate::settings::VoiceSettings;
use crate::text::normalize;

/// Formules de politesse tolérées entre le mot d'appel et le verbe.
const FILLERS: &[&str] = &[
    "est-ce que tu peux", "est ce que tu peux", "est-ce que tu pourrais", "tu peux", "peux-tu", "peux tu",
    "tu pourrais", "pourrais-tu", "pourrais tu", "s il te plait", "stp", "please", "alors", "euh", "vas-y", "vas y",
    "can you", "could you", "would you",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub intent: Intent,
    /// Verbe reconnu (forme normalisée), ou `None` si la commande suit une pause.
    pub verb: Option<String>,
    /// La demande, dans sa forme d'origine (casse et accents conservés).
    pub query: String,
}

/// Mot brut et sa forme normalisée.
#[derive(Debug, Clone)]
struct Tok {
    raw: String,
    norm: String,
    /// Le mot brut est suivi d'une ponctuation (virgule, point…) : pause.
    pause_after: bool,
}

fn tokenize(text: &str) -> Vec<Tok> {
    let mut out = vec![];
    for raw in text.split_whitespace() {
        let norm = normalize(raw);
        let pause_after = raw.ends_with([',', '.', ';', ':', '!', '?', '…']);
        if norm.is_empty() {
            if pause_after {
                if let Some(last) = out.last_mut() {
                    let last: &mut Tok = last;
                    last.pause_after = true;
                }
            }
            continue;
        }
        // « l'erreur » → deux mots normalisés ; on garde le brut sur le premier.
        let parts: Vec<&str> = norm.split(' ').collect();
        let n = parts.len();
        for (i, p) in parts.into_iter().enumerate() {
            out.push(Tok {
                raw: if i == 0 { raw.to_string() } else { String::new() },
                norm: p.to_string(),
                pause_after: pause_after && i == n - 1,
            });
        }
    }
    out
}

fn seq(s: &str) -> Vec<String> {
    normalize(s).split(' ').filter(|w| !w.is_empty()).map(String::from).collect()
}

/// Un mot de l'énoncé correspond-il au verbe (avec les terminaisons -s, -r, -z, -ez) ?
fn verb_token_matches(tok: &str, verb: &str) -> bool {
    if tok == verb {
        return true;
    }
    if let Some(rest) = tok.strip_prefix(verb) {
        return matches!(rest, "s" | "r" | "z" | "ez");
    }
    // « vérifiez » pour « vérifie »
    if let Some(stem) = verb.strip_suffix('e') {
        if let Some(rest) = tok.strip_prefix(stem) {
            return matches!(rest, "ez" | "er");
        }
    }
    false
}

/// Graphies d'un préfixe que la transcription produit à sa place (« OK » → « okay »…).
fn prefix_aliases(p: &str) -> &'static [&'static str] {
    match p {
        "ok" => &["okay", "okey", "oke", "o k"],
        "hey" => &["hé", "hei", "hay"],
        "dis" => &["di", "dit"],
        _ => &[],
    }
}

#[derive(Debug, Clone)]
pub struct Detector {
    /// Formes acceptées du mot d'appel (mot + variantes), en suites de mots normalisés.
    forms: Vec<Vec<String>>,
    strict: bool,
    prefixes: Vec<Vec<String>>,
    /// Préfixe et mot d'appel collés en un seul mot par la transcription
    /// (« OKSténo » ; « Describe » pour « Dis Scribe » avec le mot d’appel « Scribe ») : valent « préfixe + mot d'appel ».
    glued: Vec<String>,
    verbs: Vec<(Intent, Vec<String>)>,
    fillers: Vec<Vec<String>>,
}

impl Detector {
    pub fn new(v: &VoiceSettings) -> Detector {
        let mut forms = vec![seq(&v.wake_word)];
        for var in &v.variants {
            let s = seq(var);
            if !s.is_empty() && !forms.contains(&s) {
                forms.push(s);
            }
        }
        forms.retain(|f| !f.is_empty());
        // Formes les plus longues d'abord.
        forms.sort_by_key(|f| std::cmp::Reverse(f.len()));
        let mut verbs: Vec<(Intent, Vec<String>)> = vec![];
        for (intent, list) in v.verbs_by_intent() {
            for verb in list {
                let s = seq(&verb);
                if !s.is_empty() {
                    verbs.push((intent, s));
                }
            }
        }
        verbs.sort_by_key(|(_, s)| std::cmp::Reverse(s.len()));
        let mut fillers: Vec<Vec<String>> = FILLERS.iter().map(|f| seq(f)).collect();
        fillers.sort_by_key(|f| std::cmp::Reverse(f.len()));
        let mut prefixes: Vec<Vec<String>> = vec![];
        for p in &v.prefixes {
            let base = seq(p);
            if base.is_empty() {
                continue;
            }
            let key = base.join(" ");
            for alias in std::iter::once(key.as_str()).chain(prefix_aliases(&key).iter().copied()) {
                let a = seq(alias);
                if !a.is_empty() && !prefixes.contains(&a) {
                    prefixes.push(a);
                }
            }
        }
        prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
        // Collages : seulement pour un préfixe et une forme d'un seul mot.
        let mut glued: Vec<String> = vec![];
        let mut add = |g: String| {
            if g.chars().count() >= 5 && !glued.contains(&g) {
                glued.push(g);
            }
        };
        for p in prefixes.iter().filter(|p| p.len() == 1) {
            for f in forms.iter().filter(|f| f.len() == 1) {
                let (p, f) = (&p[0], &f[0]);
                add(format!("{p}{f}"));
                // « dis » + « steno » → « discribe » (consonne de jonction fondue).
                if p.chars().last() == f.chars().next() {
                    add(format!("{p}{}", &f[f.chars().next().map(char::len_utf8).unwrap_or(0)..]));
                }
                // « dis » + « steno » → « describe » : l'ASR anglais entend « de-steno ».
                if p == "dis" || p == "di" {
                    add(format!("de{f}"));
                }
            }
        }
        Detector { forms, strict: v.strict, prefixes, glued, verbs, fillers }
    }

    fn match_at(toks: &[Tok], at: usize, pat: &[String]) -> bool {
        at + pat.len() <= toks.len() && pat.iter().enumerate().all(|(i, p)| toks[at + i].norm == *p)
    }

    /// Fin du mot d'appel commençant au mot `at`, s'il y en a un.
    ///
    /// - « préfixe + mot d'appel » (ou leur collage) : reconnu n'importe où dans la phrase,
    ///   car la transcription colle souvent la commande à ce qui la précède ;
    /// - mot d'appel seul (hors mode strict) : en début de phrase ou juste après une pause.
    fn wake_at(&self, toks: &[Tok], at: usize) -> Option<usize> {
        if self.glued.iter().any(|g| toks[at].norm == *g) {
            return Some(at + 1);
        }
        for p in &self.prefixes {
            if Self::match_at(toks, at, p) {
                let after = at + p.len();
                for f in &self.forms {
                    if Self::match_at(toks, after, f) {
                        return Some(after + f.len());
                    }
                }
            }
        }
        if !self.strict {
            let opening = at < 2 || toks[at - 1].pause_after;
            if opening {
                for f in &self.forms {
                    if Self::match_at(toks, at, f) {
                        return Some(at + f.len());
                    }
                }
            }
        }
        None
    }

    /// Indices du premier mot après chaque mot d'appel de l'énoncé, dans l'ordre.
    fn wake_ends(&self, toks: &[Tok]) -> Vec<usize> {
        let mut out = vec![];
        let mut at = 0;
        while at < toks.len() {
            match self.wake_at(toks, at) {
                Some(end) => {
                    out.push(end);
                    at = end;
                }
                None => at += 1,
            }
        }
        out
    }

    /// Fin du premier mot d'appel de l'énoncé.
    fn wake_end(&self, toks: &[Tok]) -> Option<usize> {
        self.wake_ends(toks).into_iter().next()
    }

    /// L'énoncé contient-il le mot d'appel (au début, ou précédé d'un préfixe) ?
    pub fn contains_wake(&self, text: &str) -> bool {
        self.wake_end(&tokenize(text)).is_some()
    }

    fn rest_text(toks: &[Tok], from: usize) -> String {
        // Reconstitue le texte d'origine à partir du mot `from`.
        let mut out = String::new();
        let mut i = from;
        // Si `from` tombe au milieu d'un mot brut découpé (« l'erreur »), on prend la forme normalisée.
        while i < toks.len() && toks[i].raw.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&toks[i].norm);
            i += 1;
        }
        for t in &toks[i..] {
            if t.raw.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&t.raw);
        }
        out.trim_matches(|c: char| c.is_whitespace() || ",.;:!?…-".contains(c)).to_string()
    }

    /// Analyse la suite d'une commande (après le mot d'appel) : formules, verbe, demande.
    fn parse_command(&self, toks: &[Tok], mut i: usize, pause: bool) -> Option<Detection> {
        // formules de politesse
        loop {
            let mut moved = false;
            for f in &self.fillers {
                if Self::match_at(toks, i, f) {
                    i += f.len();
                    moved = true;
                    break;
                }
            }
            if !moved {
                break;
            }
        }
        for (intent, verb) in &self.verbs {
            let ok = i + verb.len() <= toks.len()
                && verb.iter().enumerate().all(|(k, v)| {
                    if k == verb.len() - 1 {
                        verb_token_matches(&toks[i + k].norm, v)
                    } else {
                        toks[i + k].norm == *v
                    }
                });
            if ok {
                let query = Self::rest_text(toks, i + verb.len());
                let needs_query = matches!(intent, Intent::Search | Intent::Check | Intent::Note);
                if needs_query && query.is_empty() {
                    return None;
                }
                return Some(Detection { intent: *intent, verb: Some(verb.join(" ")), query });
            }
        }
        if pause {
            let query = Self::rest_text(toks, i);
            if query.split_whitespace().count() >= 2 {
                return Some(Detection { intent: Intent::Search, verb: None, query });
            }
        }
        None
    }

    /// Détecte une commande dans un énoncé (mode à voix haute).
    /// Le mot d'appel peut arriver en pleine phrase : on essaie chaque occurrence,
    /// la dernière l'emportant (« … okay Sténo, cherche… » après une longue phrase).
    pub fn detect(&self, text: &str) -> Option<Detection> {
        let toks = tokenize(text);
        self.wake_ends(&toks).into_iter().rev().find_map(|end| {
            let pause = end > 0 && toks[end - 1].pause_after;
            self.parse_command(&toks, end, pause)
        })
    }

    /// Commande en aparté : pas de mot d'appel ; verbe facultatif (recherche par défaut).
    pub fn detect_aside(&self, text: &str) -> Option<Detection> {
        let toks = tokenize(text);
        if toks.is_empty() {
            return None;
        }
        // Le mot d'appel est toléré au début.
        let start = (0..2.min(toks.len())).find_map(|at| self.wake_at(&toks, at)).unwrap_or(0);
        if let Some(d) = self.parse_command(&toks, start, true) {
            return Some(d);
        }
        let query = Self::rest_text(&toks, start);
        (!query.is_empty()).then_some(Detection { intent: Intent::Search, verb: None, query })
    }
}

/// Réglages de voix des tests de ce module : mot d'appel « Scribe » (exemples historiques,
/// dont les collages « OKScribe » et « Describe » produits par la transcription).
#[cfg(test)]
pub(crate) fn test_voice() -> VoiceSettings {
    let mut v = VoiceSettings::default();
    v.wake_word = "Scribe".into();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det() -> Detector {
        Detector::new(&crate::wake::test_voice())
    }

    #[test]
    fn commandes_simples() {
        let d = det();
        let r = d.detect("Scribe, cherche les erreurs Datadog sur ms-booking depuis hier.").unwrap();
        assert_eq!(r.intent, Intent::Search);
        assert_eq!(r.query, "les erreurs Datadog sur ms-booking depuis hier");
        assert_eq!(d.detect("scribe vérifie que APP-1234 est bien fermé").unwrap().intent, Intent::Check);
        assert_eq!(d.detect("Scribe, note que Marie relance le support").unwrap().intent, Intent::Note);
        let r = d.detect("Scribe ajoute une action pour Paul : mettre à jour la doc").unwrap();
        assert_eq!(r.intent, Intent::Note);
        assert_eq!(r.verb.as_deref(), Some("ajoute une action"));
        assert_eq!(d.detect("Scribe, résume").unwrap().intent, Intent::Recap);
        assert_eq!(d.detect("Scribe annule").unwrap().intent, Intent::Cancel);
        assert_eq!(d.detect("Scribe, arrête l'écoute.").unwrap().intent, Intent::Stop);
        assert_eq!(d.detect("Scribe, termine la réunion").unwrap().intent, Intent::Stop);
        assert_ne!(d.detect("Scribe, arrête de chercher").map(|x| x.intent), Some(Intent::Stop), "« arrête » seul n'arrête rien");
        assert_eq!(d.detect("Bon, Scribe, regarde le ticket APP-42").unwrap().intent, Intent::Search);
        assert_eq!(d.detect("Scribe, tu peux chercher le runbook de ms-pricing ?").unwrap().intent, Intent::Search);
        assert_eq!(d.detect("Scribe, vérifiez la config de prod").unwrap().intent, Intent::Check);
    }

    #[test]
    fn pause_sans_verbe() {
        let d = det();
        let r = d.detect("Scribe, les erreurs 500 sur le gateway hier soir").unwrap();
        assert_eq!(r.intent, Intent::Search);
        assert!(d.detect("Scribe les erreurs").is_none(), "ni verbe ni pause");
    }

    #[test]
    fn pas_de_faux_declenchement() {
        let d = det();
        for t in [
            "Le script de migration cherche les doublons",
            "Dans la description du ticket, on cherche la cause",
            "Il s'est inscrit, cherche pas",
            "On a parlé du scribe hier, cherche dans tes notes",
            "Je vais lancer le script, cherche pas plus loin",
            "Scribe",
            "Scribe, cherche",
            "Transcription du scribe",
        ] {
            assert!(d.detect(t).is_none(), "faux déclenchement : {t}");
        }
    }

    #[test]
    fn mode_strict_et_variantes() {
        let mut v = crate::wake::test_voice();
        v.strict = true;
        let d = Detector::new(&v);
        assert!(d.detect("Scribe, cherche le ticket APP-1").is_none());
        assert!(d.detect("OK Scribe, cherche le ticket APP-1").is_some());
        assert!(d.detect("Dis Scribe cherche le ticket APP-1").is_some());
        let mut v = crate::wake::test_voice();
        v.variants = vec!["scribes".into(), "escribe".into()];
        let d = Detector::new(&v);
        assert!(d.detect("Escribe, cherche le ticket APP-1").is_some());
        let mut v = crate::wake::test_voice();
        v.wake_word = "Hey Greffier".into();
        let d = Detector::new(&v);
        assert!(d.detect("Hey greffier, cherche le ticket APP-1").is_some());
        assert!(d.detect("Greffier, cherche le ticket APP-1").is_none());
    }

    fn strict() -> Detector {
        let mut v = crate::wake::test_voice();
        v.strict = true;
        Detector::new(&v)
    }

    /// Phrases réelles de la réunion du 27/11 (transcription Parakeet), ratées auparavant.
    #[test]
    fn mot_d_appel_en_pleine_phrase() {
        let d = strict();
        let r = d
            .detect("Like I captured mon micro, he has captain Slack, et donc he's fairly transcripts in direct. Sauf que on plus of it, I also think okay Scribe, regarde dans la code base of millennium s'il y a un package.json à la racine.")
            .unwrap();
        assert_eq!(r.intent, Intent::Search);
        assert_eq!(r.query, "dans la code base of millennium s'il y a un package.json à la racine");
        let r = d
            .detect("Ouais, bah alors on va essayer en anglais ou en français, on va voir. Ok Scribe regarde dans Atlas s'il y a un package JSON à la racine.")
            .unwrap();
        assert_eq!(r.query, "dans Atlas s'il y a un package JSON à la racine");
        // « Dis Scribe » entendu « Describe ».
        let r = d.detect("Mais du coup, attends, j'ai envie de refaire un test. Describe, regarde dans le projet Atlas s'il y a un package Gizen à la racine.").unwrap();
        assert_eq!(r.intent, Intent::Search);
        assert!(r.query.starts_with("dans le projet Atlas"));
        assert!(d.detect("OKScribe, cherche le ticket APP-1").is_some());
        assert!(d.detect("O.K. Scribe, cherche le ticket APP-1").is_some());
        assert!(d.detect("Hé Scribe, vérifie la config de prod").is_some());
    }

    #[test]
    fn pleine_phrase_sans_faux_declenchement() {
        let d = strict();
        for t in [
            "On a parlé du scribe hier, cherche dans tes notes",
            "Tu peux describe the issue please",
            "Il dit que le Scribe cherche tout seul",
            "Ok, le script cherche les doublons",
            "OK Scribe",
        ] {
            assert!(d.detect(t).is_none(), "faux déclenchement : {t}");
        }
        // Hors mode strict : le mot seul compte au début ou après une pause, pas au milieu.
        let d = det();
        assert!(d.detect("Bon alors, Scribe, cherche le ticket APP-42").is_some());
        assert!(d.detect("je pense que Scribe cherche mal").is_none());
    }

    #[test]
    fn aparte() {
        let d = det();
        let r = d.detect_aside("les erreurs de ms-booking cette nuit").unwrap();
        assert_eq!(r.intent, Intent::Search);
        assert_eq!(r.query, "les erreurs de ms-booking cette nuit");
        assert_eq!(d.detect_aside("note relancer Paul").unwrap().intent, Intent::Note);
    }
}
