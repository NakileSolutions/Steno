//! Modèles de parole (sherpa-onnx, ONNX Runtime sur CPU) : emplacement, téléchargement,
//! chargement paresseux et partage entre les canaux.
//!
//! | Usage | Modèle | Taille |
//! |---|---|---|
//! | Transcription en direct | Parakeet TDT 0.6B v3 int8 (multilingue, FR inclus) | ≈ 490 Mo |
//! | VAD | Silero VAD | < 1 Mo |
//! | Retranscription de fin de réunion (langue imposée : fr, en, es, de) | Canary 180M Flash int8 | ≈ 150 Mo |
//! | Diarisation | pyannote segmentation 3.0 + 3D-Speaker ERes2Net | ≈ 45 Mo |
//!
//! Dossier : `STENO_MODELS_DIR`, sinon `$STENO_HOME/models`, sinon `~/.local/share/Steno/models`.
//! Les modèles sont téléchargés depuis les publications de sherpa-onnx sur GitHub
//! (`make install` les télécharge d'avance avec `--prefetch-models`) ; aucun audio ne sort.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Context, Result};
use sherpa_onnx::{
    OfflineCanaryModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineTransducerModelConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector,
};

const RELEASES: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download";

pub const PARAKEET: &str = "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8";
pub const CANARY: &str = "sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8";
pub const SEGMENTATION: &str = "sherpa-onnx-pyannote-segmentation-3-0";
pub const EMBEDDING: &str = "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx";
pub const SILERO: &str = "silero_vad.onnx";

/// Langues de la retranscription (Canary 180M Flash).
pub const FINAL_LANGUAGES: [&str; 4] = ["fr", "en", "es", "de"];

pub const ASR_NAME: &str = "parakeet-tdt-v3";

pub fn models_dir() -> PathBuf {
    if let Ok(d) = std::env::var("STENO_MODELS_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    if let Ok(h) = std::env::var("STENO_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h).join("models");
        }
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Steno").join("models")
}

fn threads() -> i32 {
    std::thread::available_parallelism().map(|n| n.get().clamp(1, 4) as i32).unwrap_or(2)
}

// MARK: - Téléchargement

#[derive(Debug, Clone, Copy)]
pub enum Asset {
    Parakeet,
    Silero,
    Canary,
    Segmentation,
    Embedding,
}

impl Asset {
    pub const ALL: [Asset; 5] = [Asset::Parakeet, Asset::Silero, Asset::Canary, Asset::Segmentation, Asset::Embedding];

    fn url(self) -> String {
        match self {
            Asset::Parakeet => format!("{RELEASES}/asr-models/{PARAKEET}.tar.bz2"),
            Asset::Silero => format!("{RELEASES}/asr-models/{SILERO}"),
            Asset::Canary => format!("{RELEASES}/asr-models/{CANARY}.tar.bz2"),
            Asset::Segmentation => format!("{RELEASES}/speaker-segmentation-models/{SEGMENTATION}.tar.bz2"),
            Asset::Embedding => format!("{RELEASES}/speaker-recongition-models/{EMBEDDING}"),
        }
    }

    fn target(self, dir: &Path) -> PathBuf {
        dir.join(match self {
            Asset::Parakeet => PARAKEET,
            Asset::Silero => SILERO,
            Asset::Canary => CANARY,
            Asset::Segmentation => SEGMENTATION,
            Asset::Embedding => EMBEDDING,
        })
    }

    fn label(self) -> &'static str {
        match self {
            Asset::Parakeet => "Parakeet TDT v3 (transcription)",
            Asset::Silero => "Silero VAD",
            Asset::Canary => "Canary 180M Flash (retranscription)",
            Asset::Segmentation => "pyannote segmentation (diarisation)",
            Asset::Embedding => "3D-Speaker ERes2Net (diarisation)",
        }
    }

    pub fn present(self, dir: &Path) -> bool {
        let t = self.target(dir);
        match self {
            Asset::Parakeet => t.join("encoder.int8.onnx").is_file() && t.join("tokens.txt").is_file(),
            Asset::Canary => t.join("encoder.int8.onnx").is_file() && t.join("tokens.txt").is_file(),
            Asset::Segmentation => t.join("model.onnx").is_file(),
            Asset::Silero | Asset::Embedding => t.is_file(),
        }
    }
}

static DOWNLOAD_LOCK: Mutex<()> = Mutex::new(());

/// Télécharge le modèle s'il manque (un seul téléchargement à la fois).
pub fn ensure(asset: Asset) -> Result<()> {
    let dir = models_dir();
    if asset.present(&dir) {
        return Ok(());
    }
    let _guard = DOWNLOAD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if asset.present(&dir) {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("création de {}", dir.display()))?;
    let url = asset.url();
    crate::log::info(&format!("téléchargement de {} ({url})", asset.label()));
    let started = std::time::Instant::now();
    let agent = ureq::AgentBuilder::new().try_proxy_from_env(true).timeout_connect(std::time::Duration::from_secs(20)).build();
    let resp = agent.get(&url).call().map_err(|e| anyhow!("téléchargement de {} impossible : {e}", asset.label()))?;
    let tmp = tempdir_in(&dir)?;
    let reader = resp.into_reader();
    if url.ends_with(".tar.bz2") {
        let mut archive = tar::Archive::new(bzip2::read::BzDecoder::new(reader));
        archive.unpack(&tmp).with_context(|| format!("extraction de {}", asset.label()))?;
        let target = asset.target(&dir);
        let name = target.file_name().unwrap();
        let extracted = tmp.join(name);
        if !extracted.is_dir() {
            bail!("archive inattendue pour {}", asset.label());
        }
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&extracted, &target)?;
    } else {
        let file = tmp.join("download");
        let mut out = std::fs::File::create(&file)?;
        std::io::copy(&mut { reader }, &mut out)?;
        std::fs::rename(&file, asset.target(&dir))?;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if !asset.present(&dir) {
        bail!("{} incomplet après téléchargement", asset.label());
    }
    crate::log::info(&format!("{} prêt ({:.0} s)", asset.label(), started.elapsed().as_secs_f64()));
    Ok(())
}

fn tempdir_in(dir: &Path) -> Result<PathBuf> {
    let p = dir.join(format!(".partiel-{}-{}", std::process::id(), chrono::Local::now().timestamp_millis()));
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

/// `--prefetch-models` : tous les modèles (transcription, VAD, retranscription, diarisation).
pub fn prefetch() -> Result<()> {
    let dir = models_dir();
    println!("Modèles audio dans {}", dir.display());
    for a in Asset::ALL {
        if a.present(&dir) {
            println!("  ✓ {}", a.label());
            continue;
        }
        println!("  … {} (téléchargement)", a.label());
        ensure(a)?;
        println!("  ✓ {}", a.label());
    }
    // Vérifie que le modèle principal se charge.
    let started = std::time::Instant::now();
    let r = Recognizer::load()?;
    let _ = r.transcribe(&vec![0.0; 16_000]);
    println!("Modèle de transcription chargé et testé en {:.1} s", started.elapsed().as_secs_f64());
    Ok(())
}

fn s(p: PathBuf) -> Option<String> {
    Some(p.to_string_lossy().into_owned())
}

// MARK: - Transcription

/// Parakeet TDT v3 (transcription en direct, langue détectée par le modèle).
pub struct Recognizer {
    inner: OfflineRecognizer,
    lock: Mutex<()>,
}

impl Recognizer {
    pub fn load() -> Result<Self> {
        ensure(Asset::Parakeet)?;
        let d = models_dir().join(PARAKEET);
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: s(d.join("encoder.int8.onnx")),
            decoder: s(d.join("decoder.int8.onnx")),
            joiner: s(d.join("joiner.int8.onnx")),
        };
        config.model_config.tokens = s(d.join("tokens.txt"));
        config.model_config.model_type = Some("nemo_transducer".into());
        config.model_config.num_threads = threads();
        config.model_config.provider = Some("cpu".into());
        let inner = OfflineRecognizer::create(&config).ok_or_else(|| anyhow!("chargement de Parakeet impossible (fichiers dans {})", d.display()))?;
        Ok(Recognizer { inner, lock: Mutex::new(()) })
    }

    /// Parakeet tolère mal les énoncés très courts : on complète par du silence jusqu'à 1 s.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String> {
        let mut audio = samples.to_vec();
        if audio.len() < 16_000 {
            audio.resize(16_000, 0.0);
        }
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let stream = self.inner.create_stream();
        stream.accept_waveform(16_000, &audio);
        self.inner.decode(&stream);
        let r = stream.get_result().ok_or_else(|| anyhow!("aucun résultat du modèle"))?;
        Ok(r.text.trim().to_string())
    }
}

/// Canary 180M Flash : retranscription à langue imposée (jamais gardé en mémoire).
pub struct FinalRecognizer {
    inner: OfflineRecognizer,
    pub name: String,
}

impl FinalRecognizer {
    pub fn load(language: &str) -> Result<Self> {
        if !FINAL_LANGUAGES.contains(&language) {
            bail!("langue non prise en charge par Canary 180M Flash : {language} (fr, en, es, de)");
        }
        ensure(Asset::Canary)?;
        let d = models_dir().join(CANARY);
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.canary = OfflineCanaryModelConfig {
            encoder: s(d.join("encoder.int8.onnx")),
            decoder: s(d.join("decoder.int8.onnx")),
            src_lang: Some(language.into()),
            tgt_lang: Some(language.into()),
            use_pnc: true,
        };
        config.model_config.tokens = s(d.join("tokens.txt"));
        config.model_config.num_threads = threads();
        config.model_config.provider = Some("cpu".into());
        let inner = OfflineRecognizer::create(&config).ok_or_else(|| anyhow!("chargement de Canary impossible (fichiers dans {})", d.display()))?;
        Ok(FinalRecognizer { inner, name: format!("canary-180m-flash-{language}") })
    }

    pub fn transcribe(&self, samples: &[f32]) -> Result<String> {
        let stream = self.inner.create_stream();
        stream.accept_waveform(16_000, samples);
        self.inner.decode(&stream);
        Ok(stream.get_result().ok_or_else(|| anyhow!("aucun résultat du modèle"))?.text.trim().to_string())
    }
}

// MARK: - VAD

/// Silero (par canal, à état). `detected()` après chaque trame de 512 échantillons donne
/// l'état parole / silence, avec une hystérésis courte : le découpage en énoncés reste celui
/// du segmenteur (`silence_ms`), comme sur macOS.
pub struct SileroVad {
    vad: VoiceActivityDetector,
}

pub const VAD_FRAME: usize = 512;

impl SileroVad {
    pub fn new() -> Result<Self> {
        ensure(Asset::Silero)?;
        let config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: s(models_dir().join(SILERO)),
                threshold: 0.5,
                min_silence_duration: 0.1,
                min_speech_duration: 0.05,
                window_size: VAD_FRAME as i32,
                max_speech_duration: 600.0,
            },
            sample_rate: 16_000,
            num_threads: 1,
            provider: Some("cpu".into()),
            ..Default::default()
        };
        let vad = VoiceActivityDetector::create(&config, 30.0).ok_or_else(|| anyhow!("chargement de Silero VAD impossible"))?;
        Ok(SileroVad { vad })
    }

    pub fn probability(&mut self, frame: &[f32]) -> f32 {
        self.vad.accept_waveform(frame);
        // Les segments que la VAD accumule ne servent pas : on les jette.
        while !self.vad.is_empty() {
            self.vad.pop();
        }
        if self.vad.detected() {
            1.0
        } else {
            0.0
        }
    }
}

// MARK: - Diarisation

pub fn diarize(samples: &[f32]) -> Result<Vec<crate::post::RawSpeakerTurn>> {
    ensure(Asset::Segmentation)?;
    ensure(Asset::Embedding)?;
    let dir = models_dir();
    let mut config = OfflineSpeakerDiarizationConfig::default();
    config.segmentation.pyannote.model = s(dir.join(SEGMENTATION).join("model.onnx"));
    config.segmentation.num_threads = threads();
    config.embedding.model = s(dir.join(EMBEDDING));
    config.embedding.num_threads = threads();
    config.clustering.num_clusters = -1;
    config.clustering.threshold = 0.5;
    let sd = OfflineSpeakerDiarization::create(&config).ok_or_else(|| anyhow!("chargement des modèles de diarisation impossible"))?;
    let result = sd.process(samples).ok_or_else(|| anyhow!("diarisation impossible"))?;
    Ok(result.sort_by_start_time().into_iter().map(|s| crate::post::RawSpeakerTurn { speaker: s.speaker, start: s.start as f64, end: s.end as f64 }).collect())
}

// MARK: - Partage

/// Charge le modèle de transcription une seule fois, à la demande, et le partage entre les canaux.
#[derive(Clone, Default)]
pub struct SpeechModels {
    inner: Arc<Mutex<ModelsState>>,
}

#[derive(Default)]
struct ModelsState {
    recognizer: Option<Arc<Recognizer>>,
    loading: bool,
    failed: Option<String>,
    failure_reported: bool,
}

impl SpeechModels {
    /// Le reconnaisseur (charge le modèle si besoin, en bloquant l'appelant). `Err` si indisponible ;
    /// le second élément vaut `true` la première fois que l'échec est signalé.
    pub fn recognizer(&self) -> Result<Arc<Recognizer>, (String, bool)> {
        loop {
            {
                let mut st = self.inner.lock().unwrap();
                if let Some(r) = &st.recognizer {
                    return Ok(r.clone());
                }
                if let Some(e) = st.failed.clone() {
                    let first = !st.failure_reported;
                    st.failure_reported = true;
                    return Err((e, first));
                }
                if !st.loading {
                    st.loading = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let started = std::time::Instant::now();
        let result = Recognizer::load();
        let mut st = self.inner.lock().unwrap();
        st.loading = false;
        match result {
            Ok(r) => {
                crate::log::info(&format!("modèle ASR {ASR_NAME} prêt en {:.1} s", started.elapsed().as_secs_f64()));
                let r = Arc::new(r);
                st.recognizer = Some(r.clone());
                Ok(r)
            }
            Err(e) => {
                let msg = format!("{e:#}");
                crate::log::error(&format!("modèle ASR indisponible : {msg}"));
                st.failed = Some(msg.clone());
                st.failure_reported = true;
                Err((msg, true))
            }
        }
    }

    /// Lance le chargement en arrière-plan.
    pub fn preload(&self) {
        let me = self.clone();
        std::thread::spawn(move || {
            let _ = me.recognizer();
        });
    }

    /// Après un échec (ex. pas de réseau au premier téléchargement), permet de réessayer.
    pub fn retry_if_failed(&self) {
        let mut st = self.inner.lock().unwrap();
        if st.failed.is_some() {
            st.failed = None;
            st.failure_reported = false;
        }
    }

    /// Libère le modèle (fin d'écoute) : la mémoire revient au niveau de repos.
    pub fn unload(&self) {
        let mut st = self.inner.lock().unwrap();
        if !st.loading {
            st.recognizer = None;
        }
    }
}

/// VAD d'un canal : Silero si possible, sinon énergie (`STENO_VAD=energy` pour forcer).
pub enum ChannelVad {
    Silero(SileroVad),
    Energy(crate::segmenter::EnergyVad),
}

impl ChannelVad {
    pub fn make() -> ChannelVad {
        if std::env::var("STENO_VAD").as_deref() == Ok("energy") {
            return ChannelVad::Energy(Default::default());
        }
        match SileroVad::new() {
            Ok(v) => ChannelVad::Silero(v),
            Err(e) => {
                crate::log::warn(&format!("VAD Silero indisponible ({e:#}) : repli sur la VAD énergie"));
                ChannelVad::Energy(Default::default())
            }
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ChannelVad::Silero(_) => "silero",
            ChannelVad::Energy(_) => "energy",
        }
    }

    pub fn probability(&mut self, frame: &[f32]) -> f32 {
        match self {
            ChannelVad::Silero(v) => v.probability(frame),
            ChannelVad::Energy(v) => v.probability(frame),
        }
    }
}
