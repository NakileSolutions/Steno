//! Outils de texte communs (repli des accents et de la casse, comparaison naturelle).

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Minuscules, sans accents, sans espaces aux extrémités.
pub fn fold(s: &str) -> String {
    s.nfd().filter(|c| !is_combining_mark(*c)).collect::<String>().to_lowercase().trim().to_string()
}

/// Comparaison « naturelle » : sans casse ni accents, nombres dans l'ordre numérique.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn chunks(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = vec![];
        for c in fold(s).chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, buf)) if *d == digit => buf.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    }
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(cb.iter()) {
        let ord = match (x.0, y.0) {
            (true, true) => {
                let (nx, ny) = (x.1.trim_start_matches('0'), y.1.trim_start_matches('0'));
                nx.len().cmp(&ny.len()).then_with(|| nx.cmp(ny))
            }
            _ => x.1.cmp(&y.1),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    ca.len().cmp(&cb.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repli_et_ordre_naturel() {
        assert_eq!(fold("  Réunion ÉTÉ "), "reunion ete");
        assert_eq!(natural_cmp("Micro 2", "micro 10"), std::cmp::Ordering::Less);
        assert_eq!(natural_cmp("écouteurs", "Ecouteurs"), std::cmp::Ordering::Equal);
    }
}
