//! Annulation d'écho sur le micro (WebRTC AEC3), l'équivalent du voice processing de macOS :
//! le son de l'app d'appel (canal « them ») sert de référence, retirée du micro quand
//! l'utilisateur n'a pas de casque. Trames de 10 ms à 16 kHz. Désactivable par
//! `STENO_AEC=0` ; absente en présentiel (rien n'est joué, et le traitement atténuerait les
//! personnes éloignées du micro).

#[cfg(feature = "aec")]
mod imp {
    use webrtc_audio_processing::config::{Config, EchoCanceller, HighPassFilter, NoiseSuppression, NoiseSuppressionLevel};
    use webrtc_audio_processing::Processor;

    pub struct EchoCanceller3 {
        processor: Processor,
        frame: usize,
        render: Vec<f32>,
        capture: Vec<f32>,
    }

    impl EchoCanceller3 {
        pub fn new() -> Option<Self> {
            let processor = Processor::new(16_000).ok()?;
            processor.set_config(Config {
                echo_canceller: Some(EchoCanceller::default()),
                high_pass_filter: Some(HighPassFilter::default()),
                noise_suppression: Some(NoiseSuppression { level: NoiseSuppressionLevel::Moderate, ..Default::default() }),
                ..Default::default()
            });
            let frame = processor.num_samples_per_frame();
            Some(EchoCanceller3 { processor, frame, render: vec![], capture: vec![] })
        }

        /// Son joué (référence).
        pub fn render(&mut self, samples: &[f32]) {
            self.render.extend_from_slice(samples);
            let mut off = 0;
            while self.render.len() - off >= self.frame {
                let mut f = self.render[off..off + self.frame].to_vec();
                let _ = self.processor.process_render_frame([f.as_mut_slice()]);
                off += self.frame;
            }
            self.render.drain(..off);
        }

        /// Micro : renvoie les échantillons traités (par trames complètes de 10 ms).
        pub fn capture(&mut self, samples: &[f32]) -> Vec<f32> {
            self.capture.extend_from_slice(samples);
            let mut out = Vec::with_capacity(self.capture.len());
            let mut off = 0;
            while self.capture.len() - off >= self.frame {
                let mut f = self.capture[off..off + self.frame].to_vec();
                if self.processor.process_capture_frame([f.as_mut_slice()]).is_err() {
                    f.copy_from_slice(&self.capture[off..off + self.frame]);
                }
                out.extend_from_slice(&f);
                off += self.frame;
            }
            self.capture.drain(..off);
            out
        }
    }
}

#[cfg(not(feature = "aec"))]
mod imp {
    pub struct EchoCanceller3;
    impl EchoCanceller3 {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn render(&mut self, _: &[f32]) {}
        pub fn capture(&mut self, s: &[f32]) -> Vec<f32> {
            s.to_vec()
        }
    }
}

pub use imp::EchoCanceller3;

pub fn enabled_by_env() -> bool {
    std::env::var("STENO_AEC").map(|v| v != "0").unwrap_or(true)
}

#[cfg(all(test, feature = "aec"))]
mod tests {
    #[test]
    fn retire_l_echo() {
        let mut aec = super::EchoCanceller3::new().unwrap();
        // Référence : bruit pseudo-aléatoire ; micro : la même chose atténuée et retardée de 20 ms.
        let mut x: u32 = 1;
        let mut noise = || {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((x >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.5
        };
        let reference: Vec<f32> = (0..16_000 * 6).map(|_| noise()).collect();
        let delay = 320;
        let mut energy_in = 0.0f64;
        let mut energy_out = 0.0f64;
        for (i, chunk) in reference.chunks(320).enumerate() {
            aec.render(chunk);
            let start = i * 320;
            let mic: Vec<f32> = (start..start + chunk.len()).map(|k| if k >= delay { reference[k - delay] * 0.5 } else { 0.0 }).collect();
            let out = aec.capture(&mic);
            if start > 16_000 * 4 {
                energy_in += mic.iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
                energy_out += out.iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
            }
        }
        // Au moins 10 dB d'atténuation une fois convergé.
        assert!(energy_out < energy_in / 10.0, "entrée {energy_in}, sortie {energy_out}");
    }
}
