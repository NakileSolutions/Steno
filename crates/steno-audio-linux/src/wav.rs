//! Écriture WAV PCM 16 bits mono 16 kHz (fichier en 0600) et lecture en Float32 16 kHz mono.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub struct WavWriter {
    pub path: PathBuf,
    sample_rate: u32,
    file: File,
    data_bytes: u32,
    closed: bool,
}

/// Au-delà, un trou n'est plus comblé (évite d'écrire des heures de silence après une veille).
pub const MAX_GAP_FILL_SECONDS: f64 = 30.0 * 60.0;

impl WavWriter {
    pub fn create(path: &Path, sample_rate: u32) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("création de {}", dir.display()))?;
        }
        let mut file = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path).with_context(|| format!("création de {}", path.display()))?;
        file.write_all(&header(sample_rate, 0))?;
        Ok(WavWriter { path: path.to_path_buf(), sample_rate, file, data_bytes: 0, closed: false })
    }

    /// Ajoute des échantillons Float32 dans [-1, 1].
    pub fn append(&mut self, samples: &[f32]) -> Result<()> {
        if self.closed || samples.is_empty() {
            return Ok(());
        }
        let mut data = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            let c = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
            data.extend_from_slice(&((c * 32767.0) as i16).to_le_bytes());
        }
        self.write(&data)
    }

    /// Ajoute `count` échantillons de silence (trous du flux).
    pub fn append_silence(&mut self, count: usize) -> Result<()> {
        if self.closed || count == 0 {
            return Ok(());
        }
        let mut remaining = count.min((MAX_GAP_FILL_SECONDS * self.sample_rate as f64) as usize);
        let chunk = vec![0u8; self.sample_rate as usize * 2];
        while remaining > 0 {
            let n = remaining.min(self.sample_rate as usize);
            self.write(&chunk[..n * 2])?;
            remaining -= n;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn duration_seconds(&self) -> f64 {
        (self.data_bytes / 2) as f64 / self.sample_rate as f64
    }

    /// Réécrit l'en-tête avec les tailles finales et ferme le fichier.
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header(self.sample_rate, self.data_bytes))?;
        self.file.sync_all()?;
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        // Limite du format WAV (4 Gio) : au-delà on arrête d'écrire plutôt que corrompre.
        if self.data_bytes as u64 + data.len() as u64 >= (u32::MAX - 64) as u64 {
            return Ok(());
        }
        self.file.write_all(data)?;
        self.data_bytes += data.len() as u32;
        Ok(())
    }
}

impl Drop for WavWriter {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub fn header(sample_rate: u32, data_bytes: u32) -> Vec<u8> {
    let mut d = Vec::with_capacity(44);
    let (channels, bits) = (1u16, 16u16);
    d.extend_from_slice(b"RIFF");
    d.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    d.extend_from_slice(b"WAVEfmt ");
    d.extend_from_slice(&16u32.to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&channels.to_le_bytes());
    d.extend_from_slice(&sample_rate.to_le_bytes());
    d.extend_from_slice(&(sample_rate * channels as u32 * (bits / 8) as u32).to_le_bytes());
    d.extend_from_slice(&(channels * bits / 8).to_le_bytes());
    d.extend_from_slice(&bits.to_le_bytes());
    d.extend_from_slice(b"data");
    d.extend_from_slice(&data_bytes.to_le_bytes());
    d
}

/// Lit un WAV PCM (8/16/24/32 bits entiers ou 32 bits flottants, toute fréquence, tout nombre de
/// canaux) et le rend en Float32 mono 16 kHz.
pub fn load_mono_16k(path: &Path) -> Result<Vec<f32>> {
    let mut bytes = vec![];
    File::open(path).with_context(|| format!("ouverture de {}", path.display()))?.read_to_end(&mut bytes)?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("{} n'est pas un fichier WAV", path.display());
    }
    let (mut fmt, mut data): (Option<(u16, u16, u32, u16)>, Option<&[u8]>) = (None, None);
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body_end = (pos + 8 + size).min(bytes.len());
        let body = &bytes[pos + 8..body_end];
        if id == b"fmt " && body.len() >= 16 {
            let le16 = |o: usize| u16::from_le_bytes([body[o], body[o + 1]]);
            fmt = Some((le16(0), le16(2), u32::from_le_bytes(body[4..8].try_into().unwrap()), le16(14)));
        } else if id == b"data" {
            // Un WAV interrompu (taille 0 dans l'en-tête) : on prend tout ce qui suit.
            data = Some(if size == 0 { &bytes[pos + 8..] } else { body });
        }
        pos = pos + 8 + size + (size & 1);
    }
    let (format, channels, rate, bits) = fmt.context("WAV sans bloc fmt")?;
    let data = data.context("WAV sans bloc data")?;
    let channels = channels.max(1) as usize;
    let frame_bytes = channels * (bits as usize / 8);
    if frame_bytes == 0 {
        bail!("format WAV non pris en charge");
    }
    let sample = |b: &[u8]| -> f32 {
        match (format, bits) {
            (3, 32) => f32::from_le_bytes(b[..4].try_into().unwrap()),
            (_, 16) => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
            (_, 24) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            (_, 32) => i32::from_le_bytes(b[..4].try_into().unwrap()) as f32 / 2_147_483_648.0,
            (_, 8) => (b[0] as f32 - 128.0) / 128.0,
            _ => 0.0,
        }
    };
    let step = bits as usize / 8;
    let mono: Vec<f32> = data
        .chunks_exact(frame_bytes)
        .map(|f| (0..channels).map(|c| sample(&f[c * step..(c + 1) * step])).sum::<f32>() / channels as f32)
        .collect();
    Ok(crate::resample::linear(&mono, rate as f64, 16_000.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecriture_puis_lecture() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sous/them.wav");
        let mut w = WavWriter::create(&p, 16_000).unwrap();
        w.append(&[0.5; 1600]).unwrap();
        w.append_silence(1600).unwrap();
        assert!((w.duration_seconds() - 0.2).abs() < 1e-9);
        w.close().unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        let s = load_mono_16k(&p).unwrap();
        assert_eq!(s.len(), 3200);
        assert!((s[10] - 0.5).abs() < 1e-3);
        assert_eq!(s[3000], 0.0);
    }
}
