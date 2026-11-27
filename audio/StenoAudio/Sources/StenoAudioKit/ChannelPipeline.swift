// Chaîne de traitement d'un canal (« me » ou « them ») :
//   échantillons 16 kHz → (WAV) → niveaux → VAD → segmenteur → ASR → vocabulaire → segment.
//
// Deux boucles asynchrones par canal :
//   - boucle VAD : consomme l'audio, détecte les énoncés ;
//   - boucle ASR : transcrit les énoncés dans l'ordre (le modèle est partagé entre canaux).
// Les trous du flux (tap absent ou recréé, veille) sont détectés grâce à l'horloge
// monotone : l'horloge du segmenteur avance d'autant et le WAV est complété par du silence,
// ce qui garde t0/t1 alignés sur le `start` de la session.

import Foundation

@available(macOS 14.2, *)
final class ChannelPipeline {
    struct Chunk {
        let samples: [Float]
        /// Horloge monotone (systemUptime) à la réception du bloc.
        let uptime: TimeInterval
    }

    let channel: Channel

    private let sessionStartUptime: TimeInterval
    private let segmenterConfig: SegmenterConfig
    private let models: SpeechModels
    private let corrector: VocabularyCorrector
    private let wav: WavWriter?
    private let meter: LevelMeter
    private let onSegment: (Utterance, String, String?) -> Void
    private let onError: (ErrorCode, String) -> Void

    private let chunkStream: AsyncStream<Chunk>
    private let chunkContinuation: AsyncStream<Chunk>.Continuation
    private let utteranceStream: AsyncStream<Utterance>
    private let utteranceContinuation: AsyncStream<Utterance>.Continuation
    private var vadTask: Task<Void, Never>?
    private var asrTask: Task<Void, Never>?

    /// Écart minimal entre l'horloge et l'audio reçu pour considérer qu'il y a un trou.
    private static let gapThresholdSeconds = 0.5

    init(channel: Channel,
         sessionStartUptime: TimeInterval,
         silenceMs: Int,
         vocabulary: [String],
         models: SpeechModels,
         wav: WavWriter?,
         meter: LevelMeter,
         onSegment: @escaping (Utterance, String, String?) -> Void,
         onError: @escaping (ErrorCode, String) -> Void)
    {
        self.channel = channel
        self.sessionStartUptime = sessionStartUptime
        var config = SegmenterConfig()
        config.silenceMs = silenceMs
        self.segmenterConfig = config
        self.models = models
        self.corrector = VocabularyCorrector(vocabulary: vocabulary)
        self.wav = wav
        self.meter = meter
        self.onSegment = onSegment
        self.onError = onError
        let chunks = AsyncStream<Chunk>.makeStream(bufferingPolicy: .unbounded)
        chunkStream = chunks.stream
        chunkContinuation = chunks.continuation
        let utterances = AsyncStream<Utterance>.makeStream(bufferingPolicy: .unbounded)
        utteranceStream = utterances.stream
        utteranceContinuation = utterances.continuation
    }

    /// Fournit des échantillons 16 kHz mono. Sûr depuis n'importe quel thread.
    func feed(_ samples: [Float]) {
        guard !samples.isEmpty else { return }
        chunkContinuation.yield(Chunk(samples: samples, uptime: ProcessInfo.processInfo.systemUptime))
    }

    func start() {
        vadTask = Task { [self] in await runVADLoop() }
        asrTask = Task { [self] in await runASRLoop() }
    }

    /// Termine le flux : vide le segmenteur, transcrit les énoncés restants, ferme le WAV.
    func finish() async {
        chunkContinuation.finish()
        await vadTask?.value
        await asrTask?.value
    }

    // MARK: Boucle VAD

    private func runVADLoop() async {
        let vad = await models.makeVAD()
        Log.info("canal \(channel.rawValue) : VAD \(vad.name)")
        var segmenter = UtteranceSegmenter(config: segmenterConfig)
        var pending: [Float] = []
        let frameSize = vad.frameSize
        let rate = Double(segmenterConfig.sampleRate)
        let gapThreshold = Int(Self.gapThresholdSeconds * rate)

        for await chunk in chunkStream {
            // 1. Trou dans le flux ?
            let expectedStart = Int(((chunk.uptime - sessionStartUptime) * rate).rounded()) - chunk.samples.count
            var gap = expectedStart - (segmenter.position + pending.count)
            if gap > gapThreshold {
                if !pending.isEmpty {
                    // Complète la trame en cours avec du silence avant de sauter.
                    let pad = frameSize - pending.count
                    pending += [Float](repeating: 0, count: pad)
                    gap -= pad
                    await process(frame: pending, vad: vad, segmenter: &segmenter)
                    pending.removeAll(keepingCapacity: true)
                }
                Log.debug("canal \(channel.rawValue) : trou de \(String(format: "%.2f", Double(gap) / rate)) s")
                if let u = segmenter.skip(samples: gap) { utteranceContinuation.yield(u) }
                writeWav { try $0.appendSilence(gap) }
            }

            // 2. Audio réel.
            writeWav { try $0.append(chunk.samples) }
            meter.add(chunk.samples, channel: channel)
            pending += chunk.samples
            while pending.count >= frameSize {
                let frame = Array(pending.prefix(frameSize))
                pending.removeFirst(frameSize)
                await process(frame: frame, vad: vad, segmenter: &segmenter)
            }
        }

        // Fin de flux : dernière trame partielle puis vidage.
        if !pending.isEmpty {
            pending += [Float](repeating: 0, count: frameSize - pending.count)
            await process(frame: pending, vad: vad, segmenter: &segmenter)
        }
        if let u = segmenter.flush() { utteranceContinuation.yield(u) }
        utteranceContinuation.finish()
        if let wav {
            do { try wav.close() } catch { Log.warn("fermeture de \(wav.url.lastPathComponent) : \(error)") }
        }
    }

    private func process(frame: [Float], vad: VoiceActivityDetector, segmenter: inout UtteranceSegmenter) async {
        let probability: Float
        do {
            probability = try await vad.speechProbability(frame)
        } catch {
            Log.debug("VAD : \(error)")
            probability = 0
        }
        for u in segmenter.push(frame, probability: probability) {
            utteranceContinuation.yield(u)
        }
    }

    private var wavFailed = false

    private func writeWav(_ body: (WavWriter) throws -> Void) {
        guard let wav, !wavFailed else { return }
        do {
            try body(wav)
        } catch {
            wavFailed = true
            Log.error("écriture de \(wav.url.lastPathComponent) impossible : \(error)")
        }
    }

    // MARK: Boucle ASR

    private func runASRLoop() async {
        for await utterance in utteranceStream {
            guard let recognizer = await models.recognizer() else { continue } // model_missing déjà signalé
            let started = Date()
            do {
                let (raw, confidence) = try await recognizer.transcribeDetailed(utterance.samples)
                let text = corrector.correct(raw.trimmingCharacters(in: .whitespacesAndNewlines))
                if !text.isEmpty, let why = HallucinationFilter.rejection(
                    text: text, confidence: confidence, duration: utterance.duration,
                    rmsDb: HallucinationFilter.rmsDb(utterance.samples)
                ) {
                    Log.info("canal \(channel.rawValue) : phrase écartée « \(text) » (\(why))")
                    continue
                }
                let elapsed = Date().timeIntervalSince(started)
                Log.debug("canal \(channel.rawValue) : \(String(format: "%.1f", utterance.duration)) s transcrits en \(String(format: "%.2f", elapsed)) s")
                guard !text.isEmpty else { continue }
                // Intervenants : pas d'identification en direct (« Eux »), seulement la
                // diarisation de fin de réunion.
                onSegment(utterance, text, nil)
            } catch {
                onError(.asrFailed, "Transcription impossible : \(error)")
            }
        }
    }
}
