//! Filtres de texte : fausses phrases (`HallucinationFilter.swift`) et correction par le
//! vocabulaire (`VocabularyCorrector.swift`). Ports fidèles, mêmes seuils.

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

// MARK: - Fausses phrases

/// Sur un bruit (souffle, clic, voix lointaine), le modèle « invente » parfois une courte phrase,
/// souvent en anglais (« Thank you. »). Une phrase est écartée si :
///   1. l'énoncé est presque silencieux ;
///   2. ou c'est une interjection anglaise isolée, brève ou transcrite avec une confiance faible ;
///   3. ou c'est une phrase très courte transcrite avec une confiance très faible.
pub mod hallucination {
    pub const SILENCE_DB: f64 = -55.0;
    pub const FILLER_CONFIDENCE: f32 = 0.8;
    pub const SHORT_CONFIDENCE: f32 = 0.35;

    pub const FILLERS: &[&str] = &[
        "thank you", "thanks", "thank god", "thank you very much", "thanks for watching", "okay", "ok", "oh", "ah", "uh", "um", "hmm", "mm", "mhm",
        "mm hmm", "uh huh", "yeah", "yes", "no", "so", "right", "bye", "bye bye", "you", "shall we", "well", "all right", "alright", "sorry", "hello",
        "hi", "wow", "yep", "nope", "huh",
    ];

    /// Raison de l'écart, ou `None` si la phrase est conservée.
    pub fn rejection(text: &str, confidence: Option<f32>, duration: f64, rms_db: f64) -> Option<String> {
        let norm = normalized(text);
        if norm.is_empty() {
            return Some("texte vide".into());
        }
        if rms_db < SILENCE_DB {
            return Some(format!("énoncé presque silencieux ({rms_db:.0} dBFS)"));
        }
        let words = norm.split(' ').count();
        if FILLERS.contains(&norm.as_str()) && (duration < 1.5 || confidence.unwrap_or(1.0) < FILLER_CONFIDENCE) {
            return Some("interjection isolée typique d'une hallucination".into());
        }
        if words <= 3 {
            if let Some(c) = confidence {
                if c < SHORT_CONFIDENCE {
                    return Some(format!("phrase courte peu fiable (confiance {c:.2})"));
                }
            }
        }
        None
    }

    /// Minuscules, sans ponctuation ni espaces superflus.
    pub fn normalized(text: &str) -> String {
        let lowered: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
        lowered.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Niveau moyen (RMS) d'un énoncé, en dBFS (−120 sans signal).
    pub fn rms_db(samples: &[f32]) -> f64 {
        if samples.is_empty() {
            return -120.0;
        }
        let r = crate::levels::rms(samples);
        if r > 0.0 {
            20.0 * r.log10()
        } else {
            -120.0
        }
    }
}

// MARK: - Vocabulaire

/// Correction par dictionnaire, après la transcription : remplace dans le texte les formes
/// approchantes par la forme canonique du vocabulaire. Garde-fous :
/// - distance d'édition maximale selon la longueur (0 jusqu'à 4 lettres, 1 jusqu'à 8, 2 au-delà) ;
/// - pour une correction approchée, la première lettre doit être la même ;
/// - les entrées de moins de 4 caractères normalisés sont ignorées ;
/// - une fenêtre de plusieurs mots ne franchit pas la ponctuation (espaces et tirets seulement).
#[derive(Debug, Clone)]
pub struct VocabularyCorrector {
    entries: Vec<Entry>,
    max_window: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    canonical: String,
    key: Vec<char>,
    token_count: usize,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    start: usize,
    end: usize,
    is_word: bool,
}

impl VocabularyCorrector {
    pub fn new(vocabulary: &[String]) -> Self {
        let mut entries: Vec<Entry> = vec![];
        for raw in vocabulary {
            let canonical = raw.trim().to_string();
            let key: Vec<char> = normalize(&canonical).chars().collect();
            if key.len() < 4 || entries.iter().any(|e| e.key == key) {
                continue;
            }
            let tokens = tokenize(&canonical).iter().filter(|t| t.is_word).count();
            entries.push(Entry { canonical, key, token_count: tokens.max(1) });
        }
        let max_window = (entries.iter().map(|e| e.token_count).max().unwrap_or(1) + 1).min(4);
        VocabularyCorrector { entries, max_window }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn correct(&self, text: &str) -> String {
        if self.entries.is_empty() || text.is_empty() {
            return text.to_string();
        }
        let tokens = tokenize(text);
        let words: Vec<usize> = (0..tokens.len()).filter(|i| tokens[*i].is_word).collect();
        if words.is_empty() {
            return text.to_string();
        }
        let mut result = String::new();
        let mut cursor = 0;
        let mut i = 0;
        while i < words.len() {
            let mut matched = false;
            let longest = self.max_window.min(words.len() - i);
            for n in (1..=longest).rev() {
                let window = &words[i..i + n];
                if !joinable(window, &tokens, text) {
                    continue;
                }
                let joined: String = window.iter().map(|w| normalize(&text[tokens[*w].start..tokens[*w].end])).collect();
                let Some(entry) = self.best_match(&joined, n) else { continue };
                let start = tokens[window[0]].start;
                let end = tokens[window[n - 1]].end;
                result.push_str(&text[cursor..start]);
                result.push_str(&entry.canonical);
                cursor = end;
                i += n;
                matched = true;
                break;
            }
            if !matched {
                i += 1;
            }
        }
        result.push_str(&text[cursor..]);
        result
    }

    fn best_match(&self, joined: &str, window_size: usize) -> Option<&Entry> {
        if joined.is_empty() {
            return None;
        }
        let a: Vec<char> = joined.chars().collect();
        let mut best: Option<(&Entry, usize)> = None;
        for e in &self.entries {
            if a == e.key {
                return Some(e);
            }
            if window_size > e.token_count + 1 || a.len() < 4 {
                continue;
            }
            let limit = max_distance(e.key.len());
            if limit == 0 || a.len().abs_diff(e.key.len()) > limit || a.first() != e.key.first() {
                continue;
            }
            let d = levenshtein(&a, &e.key, limit);
            if d <= limit && d < best.map(|b| b.1).unwrap_or(usize::MAX) {
                best = Some((e, d));
            }
        }
        best.map(|b| b.0)
    }
}

pub fn max_distance(n: usize) -> usize {
    match n {
        0..=4 => 0,
        5..=8 => 1,
        _ => 2,
    }
}

/// Distance de Levenshtein bornée (renvoie `limit + 1` dès que la borne est dépassée).
pub fn levenshtein(a: &[char], b: &[char], limit: usize) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        current[0] = i;
        let mut row_min = current[0];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            current[j] = (previous[j] + 1).min(current[j - 1] + 1).min(previous[j - 1] + cost);
            row_min = row_min.min(current[j]);
        }
        if row_min > limit {
            return limit + 1;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// Minuscules, sans accents, lettres et chiffres seulement.
pub fn normalize(s: &str) -> String {
    s.nfd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = vec![];
    let mut start = 0;
    let mut current: Option<bool> = None;
    for (idx, ch) in text.char_indices() {
        let is_word = ch.is_alphanumeric();
        if let Some(c) = current {
            if c != is_word {
                tokens.push(Token { start, end: idx, is_word: c });
                start = idx;
            }
        }
        current = Some(is_word);
    }
    if let Some(c) = current {
        tokens.push(Token { start, end: text.len(), is_word: c });
    }
    tokens
}

/// Les mots de la fenêtre ne sont séparés que par des espaces ou des tirets.
fn joinable(window: &[usize], tokens: &[Token], text: &str) -> bool {
    if window.len() <= 1 {
        return true;
    }
    for k in 0..window.len() - 1 {
        let (a, b) = (window[k], window[k + 1]);
        if b != a + 2 {
            return false;
        }
        let sep = &text[tokens[a + 1].start..tokens[a + 1].end];
        if !sep.chars().all(|c| c.is_whitespace() || c == '-') {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::hallucination::*;
    use super::*;

    fn v(words: &[&str]) -> VocabularyCorrector {
        VocabularyCorrector::new(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn vocabulaire() {
        let c = v(&["Sténo", "ms-booking", "Atlas", "AI"]);
        assert_eq!(c.correct("ok steno cherche"), "ok Sténo cherche");
        assert_eq!(c.correct("les erreurs ms booking hier"), "les erreurs ms-booking hier");
        assert_eq!(c.correct("le script tourne"), "le script tourne");
        assert_eq!(c.correct("ai dit"), "ai dit");
        assert_eq!(c.correct("Atlass, puis"), "Atlas, puis");
        assert_eq!(c.correct("ms, booking"), "ms, booking");
        assert!(v(&[]).is_empty());
    }

    #[test]
    fn fausses_phrases() {
        assert!(rejection("Thank you.", None, 0.8, -30.0).is_some());
        assert!(rejection("Thank you.", None, 2.0, -30.0).is_none());
        assert!(rejection("Oui.", None, 0.5, -30.0).is_none());
        assert!(rejection("Bonjour à tous", None, 1.0, -70.0).is_some());
        assert!(rejection("on y va", Some(0.2), 1.0, -30.0).is_some());
        assert_eq!(normalized("  D'accord,   OK! "), "d accord ok");
        assert_eq!(rms_db(&[]), -120.0);
    }

    #[test]
    fn distance() {
        let a: Vec<char> = "steno".chars().collect();
        let b: Vec<char> = "stenoo".chars().collect();
        assert_eq!(levenshtein(&a, &b, 2), 1);
        let c: Vec<char> = "xxxxxxxx".chars().collect();
        assert_eq!(levenshtein(&a, &c, 1), 2);
    }
}
