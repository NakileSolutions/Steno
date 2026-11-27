//! Chaîne de traitement d'un canal (« me » ou « them ») :
//!   échantillons 16 kHz → (WAV) → niveaux → VAD → segmenteur → ASR → vocabulaire → segment.
//!
//! Deux threads par canal : la boucle VAD (consomme l'audio, détecte les énoncés) et la boucle
//! ASR (transcrit les énoncés dans l'ordre ; le modèle est partagé entre canaux). Les trous du
//! flux (flux recréé, veille…) sont détectés grâce à l'horloge monotone : l'horloge du
//! segmenteur avance d'autant et le WAV est complété par du silence, ce qui garde t0/t1 alignés
//! sur le `start` de la session. Port de `ChannelPipeline.swift`.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::filters::{hallucination, VocabularyCorrector};
use crate::levels::LevelMeter;
use crate::models::{ChannelVad, SpeechModels, VAD_FRAME};
use crate::protocol::{Channel, ErrorCode};
use crate::segmenter::{SegmenterConfig, Utterance, UtteranceSegmenter, SAMPLE_RATE};
use crate::wav::WavWriter;

pub type SegmentFn = Arc<dyn Fn(&Utterance, String) + Send + Sync>;
pub type ErrorFn = Arc<dyn Fn(ErrorCode, String, bool) + Send + Sync>;

struct Chunk {
    samples: Vec<f32>,
    at: Instant,
}

pub struct ChannelPipeline {
    tx: Option<Sender<Chunk>>,
    vad: Option<JoinHandle<()>>,
    asr: Option<JoinHandle<()>>,
}

/// Écart minimal entre l'horloge et l'audio reçu pour considérer qu'il y a un trou.
const GAP_THRESHOLD_SECONDS: f64 = 0.5;

pub struct PipelineConfig {
    pub channel: Channel,
    pub session_start: Instant,
    pub silence_ms: u32,
    pub vocabulary: Vec<String>,
    pub models: SpeechModels,
    pub wav: Option<WavWriter>,
    pub meter: Arc<LevelMeter>,
    pub on_segment: SegmentFn,
    pub on_error: ErrorFn,
}

impl ChannelPipeline {
    pub fn start(cfg: PipelineConfig) -> ChannelPipeline {
        let (tx, rx) = unbounded::<Chunk>();
        let (utx, urx) = unbounded::<Utterance>();
        let channel = cfg.channel;
        let seg_config = SegmenterConfig { silence_ms: cfg.silence_ms as usize, ..Default::default() };
        let (start, wav, meter) = (cfg.session_start, cfg.wav, cfg.meter);
        let vad = std::thread::Builder::new()
            .name(format!("steno.vad.{}", channel.as_str()))
            .spawn(move || vad_loop(channel, start, seg_config, wav, meter, rx, utx))
            .expect("thread VAD");
        let corrector = VocabularyCorrector::new(&cfg.vocabulary);
        let (models, on_segment, on_error) = (cfg.models, cfg.on_segment, cfg.on_error);
        let asr = std::thread::Builder::new()
            .name(format!("steno.asr.{}", channel.as_str()))
            .spawn(move || asr_loop(channel, models, corrector, urx, on_segment, on_error))
            .expect("thread ASR");
        ChannelPipeline { tx: Some(tx), vad: Some(vad), asr: Some(asr) }
    }


    pub fn sender(&self) -> PipelineFeed {
        PipelineFeed { tx: self.tx.clone() }
    }

    /// Termine le flux : vide le segmenteur, transcrit les énoncés restants, ferme le WAV.
    pub fn finish(&mut self) {
        self.tx = None;
        if let Some(h) = self.vad.take() {
            let _ = h.join();
        }
        if let Some(h) = self.asr.take() {
            let _ = h.join();
        }
    }
}

/// Entrée d'un canal, clonable (rappels du serveur son).
#[derive(Clone)]
pub struct PipelineFeed {
    tx: Option<Sender<Chunk>>,
}

impl PipelineFeed {
    pub fn feed(&self, samples: Vec<f32>) {
        if let (Some(tx), false) = (&self.tx, samples.is_empty()) {
            let _ = tx.send(Chunk { samples, at: Instant::now() });
        }
    }
}

fn vad_loop(channel: Channel, start: Instant, config: SegmenterConfig, mut wav: Option<WavWriter>, meter: Arc<LevelMeter>, rx: Receiver<Chunk>, out: Sender<Utterance>) {
    let mut vad = ChannelVad::make();
    crate::log::info(&format!("canal {} : VAD {}", channel.as_str(), vad.name()));
    let mut segmenter = UtteranceSegmenter::new(config);
    let mut pending: Vec<f32> = Vec::with_capacity(VAD_FRAME * 4);
    let rate = SAMPLE_RATE as f64;
    let gap_threshold = (GAP_THRESHOLD_SECONDS * rate) as i64;
    let mut wav_failed = false;
    let mut write_wav = |wav: &mut Option<WavWriter>, f: &mut dyn FnMut(&mut WavWriter) -> anyhow::Result<()>| {
        if let (Some(w), false) = (wav.as_mut(), wav_failed) {
            if let Err(e) = f(w) {
                wav_failed = true;
                crate::log::error(&format!("écriture de {} impossible : {e:#}", w.path.display()));
            }
        }
    };
    let process = |frame: &[f32], segmenter: &mut UtteranceSegmenter, vad: &mut ChannelVad| {
        let p = vad.probability(frame);
        for u in segmenter.push(frame, p) {
            let _ = out.send(u);
        }
    };

    for chunk in rx.iter() {
        // 1. Trou dans le flux ?
        let expected_start = (chunk.at.duration_since(start).as_secs_f64() * rate).round() as i64 - chunk.samples.len() as i64;
        let mut gap = expected_start - (segmenter.position + pending.len()) as i64;
        if gap > gap_threshold {
            if !pending.is_empty() {
                let pad = VAD_FRAME - pending.len();
                pending.resize(VAD_FRAME, 0.0);
                gap -= pad as i64;
                process(&pending, &mut segmenter, &mut vad);
                pending.clear();
            }
            crate::log::debug(&format!("canal {} : trou de {:.2} s", channel.as_str(), gap as f64 / rate));
            if let Some(u) = segmenter.skip(gap.max(0) as usize) {
                let _ = out.send(u);
            }
            let g = gap.max(0) as usize;
            write_wav(&mut wav, &mut |w| w.append_silence(g));
        }
        // 2. Audio réel.
        write_wav(&mut wav, &mut |w| w.append(&chunk.samples));
        meter.add(&chunk.samples, channel);
        pending.extend_from_slice(&chunk.samples);
        let mut offset = 0;
        while pending.len() - offset >= VAD_FRAME {
            let frame = pending[offset..offset + VAD_FRAME].to_vec();
            process(&frame, &mut segmenter, &mut vad);
            offset += VAD_FRAME;
        }
        pending.drain(..offset);
    }

    // Fin de flux : dernière trame partielle puis vidage.
    if !pending.is_empty() {
        pending.resize(VAD_FRAME, 0.0);
        process(&pending, &mut segmenter, &mut vad);
    }
    if let Some(u) = segmenter.flush() {
        let _ = out.send(u);
    }
    if let Some(w) = wav.as_mut() {
        if let Err(e) = w.close() {
            crate::log::warn(&format!("fermeture de {} : {e:#}", w.path.display()));
        }
    }
}

fn asr_loop(channel: Channel, models: SpeechModels, corrector: VocabularyCorrector, rx: Receiver<Utterance>, on_segment: SegmentFn, on_error: ErrorFn) {
    for u in rx.iter() {
        let recognizer = match models.recognizer() {
            Ok(r) => r,
            Err((msg, first)) => {
                if first {
                    on_error(ErrorCode::ModelMissing, format!("Modèle de transcription indisponible : {msg}. Lancez `make install` (téléchargement des modèles) puis réessayez."), false);
                }
                continue;
            }
        };
        let started = Instant::now();
        match recognizer.transcribe(&u.samples) {
            Ok(raw) => {
                let text = corrector.correct(raw.trim());
                if text.is_empty() {
                    continue;
                }
                if let Some(why) = hallucination::rejection(&text, None, u.duration(), hallucination::rms_db(&u.samples)) {
                    crate::log::info(&format!("canal {} : phrase écartée « {text} » ({why})", channel.as_str()));
                    continue;
                }
                crate::log::debug(&format!("canal {} : {:.1} s transcrits en {:.2} s", channel.as_str(), u.duration(), started.elapsed().as_secs_f64()));
                on_segment(&u, text);
            }
            Err(e) => on_error(ErrorCode::AsrFailed, format!("Transcription impossible : {e:#}"), true),
        }
    }
}
