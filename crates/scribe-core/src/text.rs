//! Outils texte : normalisation (casse, accents, ponctuation), distances, syllabes.

use unicode_normalization::UnicodeNormalization;

/// Minuscules, sans accents, ponctuation remplacée par des espaces, espaces fusionnés.
pub fn normalize(s: &str) -> String {
    normalize_keep(s, "")
}

/// Comme `normalize`, en gardant certains caractères de ponctuation (ex. `":"`).
pub fn normalize_keep(s: &str, keep: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.nfd() {
        if unicode_normalization::char::is_combining_mark(c) {
            continue;
        }
        let c = match c {
            'œ' | 'Œ' => {
                out.push_str("oe");
                last_space = false;
                continue;
            }
            'æ' | 'Æ' => {
                out.push_str("ae");
                last_space = false;
                continue;
            }
            '’' | '\'' => ' ',
            _ => c,
        };
        if c.is_alphanumeric() || keep.contains(c) {
            for l in c.to_lowercase() {
                out.push(l);
            }
            last_space = false;
        } else if c == '-' || c == '_' {
            // garder les identifiants comme ms-booking ou APP-1234
            out.push(c);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    let t = out.trim().to_string();
    // Retirer les tirets isolés.
    t.split(' ')
        .map(|w| w.trim_matches(|c| c == '-' || c == '_'))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Mots normalisés.
pub fn words(s: &str) -> Vec<String> {
    normalize(s).split(' ').filter(|w| !w.is_empty()).map(String::from).collect()
}

/// Distance d'édition (Levenshtein) sur les caractères.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Similarité de Sørensen–Dice sur les bigrammes de caractères (0..1), après normalisation.
pub fn similarity(a: &str, b: &str) -> f64 {
    let a = normalize(a).replace(' ', "");
    let b = normalize(b).replace(' ', "");
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a == b {
        return 1.0;
    }
    let bg = |s: &str| -> Vec<(char, char)> {
        let c: Vec<char> = s.chars().collect();
        c.windows(2).map(|w| (w[0], w[1])).collect()
    };
    let x = bg(&a);
    let mut y = bg(&b);
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    let total = x.len() + y.len();
    let mut inter = 0usize;
    for p in &x {
        if let Some(pos) = y.iter().position(|q| q == p) {
            y.swap_remove(pos);
            inter += 1;
        }
    }
    2.0 * inter as f64 / total as f64
}

/// Estimation du nombre de syllabes d'un mot français (groupes de voyelles, e muet final).
pub fn syllables_fr(word: &str) -> usize {
    let w = normalize(word);
    let mut total = 0;
    for part in w.split(' ') {
        let chars: Vec<char> = part.chars().collect();
        let is_v = |c: char| "aeiouy".contains(c);
        let mut n = 0;
        let mut prev = false;
        for &c in &chars {
            let v = is_v(c);
            if v && !prev {
                n += 1;
            }
            prev = v;
        }
        // e muet final (« scribe » = 1 syllabe), sauf mot d'une seule syllabe
        if n > 1 {
            let s: String = chars.iter().collect();
            if s.ends_with('e') || s.ends_with("es") || s.ends_with("ent") {
                let before_e = if s.ends_with("ent") {
                    chars.len().saturating_sub(4)
                } else if s.ends_with("es") {
                    chars.len().saturating_sub(3)
                } else {
                    chars.len().saturating_sub(2)
                };
                if chars.get(before_e).map(|c| !is_v(*c)).unwrap_or(false) {
                    n -= 1;
                }
            }
        }
        total += n.max(if part.is_empty() { 0 } else { 1 });
    }
    total
}

/// Tronque proprement à `max` caractères en ajoutant « … ».
pub fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out = out.trim_end().to_string();
    out.push('…');
    out
}

/// Transforme un titre en identifiant (`daily-orion`).
pub fn slugify(s: &str) -> String {
    let n = normalize(s);
    let mut out = String::new();
    for c in n.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    out.chars().take(40).collect::<String>().trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation() {
        assert_eq!(normalize("Scribe, cherche l’erreur sur MS-Booking !"), "scribe cherche l erreur sur ms-booking");
        assert_eq!(normalize("Vérifie ça — tout de suite."), "verifie ca tout de suite");
        assert_eq!(normalize("  -  "), "");
        assert_eq!(normalize_keep("1:1 Alex", ":"), "1:1 alex");
    }

    #[test]
    fn distances() {
        assert_eq!(levenshtein("scribe", "script"), 2);
        assert_eq!(levenshtein("scripe", "script"), 1);
        assert!(similarity("on regarde le ticket APP-1234", "On regarde le ticket APP 1234.") > 0.9);
        assert!(similarity("bonjour à tous", "le verrou de créneau") < 0.3);
    }

    #[test]
    fn syllabes() {
        assert_eq!(syllables_fr("Scribe"), 1);
        assert_eq!(syllables_fr("Greffier"), 2);
        assert_eq!(syllables_fr("Jarvis"), 2);
        assert_eq!(syllables_fr("ok"), 1);
        assert_eq!(syllables_fr("assistant"), 3);
    }

    #[test]
    fn slug() {
        assert_eq!(slugify("Daily ORION"), "daily-orion");
        assert_eq!(slugify("Point RH : 1:1 / Équipe"), "point-rh-1-1-equipe");
    }
}
