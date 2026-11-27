//! Rééchantillonnage. La capture demande directement du 16 kHz mono au serveur son (qui
//! convertit) ; seule la lecture de fichiers WAV quelconques a besoin d'une conversion ici.

/// Interpolation linéaire avec un filtre passe-bas simple (moyenne glissante) en sous-échantillonnage.
pub fn linear(input: &[f32], from: f64, to: f64) -> Vec<f32> {
    if input.is_empty() || from <= 0.0 || to <= 0.0 || (from - to).abs() < 1e-9 {
        return input.to_vec();
    }
    let ratio = from / to;
    // Anti-repliement grossier : moyenne sur `ratio` échantillons quand on réduit la fréquence.
    let smoothed: Vec<f32> = if ratio > 1.0 {
        let w = ratio.round().max(1.0) as usize;
        let mut out = Vec::with_capacity(input.len());
        let mut acc = 0.0f32;
        for i in 0..input.len() {
            acc += input[i];
            if i >= w {
                acc -= input[i - w];
            }
            out.push(acc / (i + 1).min(w) as f32);
        }
        out
    } else {
        input.to_vec()
    };
    let n = ((input.len() as f64) / ratio).floor() as usize;
    (0..n)
        .map(|i| {
            let x = i as f64 * ratio;
            let i0 = x.floor() as usize;
            let frac = (x - i0 as f64) as f32;
            let a = smoothed[i0.min(smoothed.len() - 1)];
            let b = smoothed[(i0 + 1).min(smoothed.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn longueurs() {
        assert_eq!(super::linear(&[0.0; 48_000], 48_000.0, 16_000.0).len(), 16_000);
        assert_eq!(super::linear(&[0.0; 8_000], 8_000.0, 16_000.0).len(), 16_000);
        assert_eq!(super::linear(&[0.1; 10], 16_000.0, 16_000.0), vec![0.1; 10]);
    }
}
