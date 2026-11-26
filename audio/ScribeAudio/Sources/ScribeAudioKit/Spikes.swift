// Modes « spike » en ligne de commande (hors protocole, sortie lisible sur stdout).
//
//   scribe-audio --spike-record <secondes> <dossier>   → me.wav + them.wav (spike S1)
//   scribe-audio --spike-transcribe <fichier.wav>      → transcription + temps par phrase (spike S2)

import AVFoundation
import CoreAudio
import Foundation

@available(macOS 14.2, *)
enum Spikes {
    // MARK: S1 — capture

    static func record(seconds: Double, directory: URL) async -> Int32 {
        print("== Spike S1 : capture micro + Teams pendant \(Int(seconds)) s ==")
        let permissions = PermissionsTracker()
        let mic = await PermissionsTracker.requestMicrophoneIfNeeded()
        let snap = permissions.snapshot()
        print("Permissions : micro=\(mic.rawValue) accessibilité=\(snap.accessibility.rawValue) audio système=inconnu (vérifié à l'usage)")

        let meWriter: WavWriter
        let themWriter: WavWriter
        do {
            meWriter = try WavWriter(url: directory.appendingPathComponent("me.wav"))
            themWriter = try WavWriter(url: directory.appendingPathComponent("them.wav"))
        } catch {
            print("ERREUR : impossible de créer les fichiers dans \(directory.path) : \(error)")
            return 1
        }

        // Les écritures passent par une file unique (les callbacks audio viennent de plusieurs threads).
        let writeQueue = DispatchQueue(label: "scribe.audio.spike.write")
        let meResampler = Resampler()
        let themResampler = Resampler()
        let meter = LevelMeter()
        let peak = PeakTracker()

        let teams = TeamsMonitor.scan()
        let target: ProcessTap.Target
        if teams.processObjects.isEmpty {
            print("Teams : aucun processus audio trouvé → tap global (tout le son du Mac sauf scribe-audio).")
            target = .systemExcluding(HAL.processObject(forPID: getpid()).map { [$0] } ?? [])
        } else {
            print("Teams : \(teams.processObjects.count) processus audio \(teams.processObjects), micro utilisé par Teams : \(teams.inputActive ? "oui" : "non")")
            print("Titre de réunion (accessibilité) : \(TeamsWindowTitle.read(pids: teams.appPIDs) ?? "—")")
            target = .processes(teams.processObjects)
        }

        let micCapture = MicCapture(voiceProcessing: true) { samples, rate in
            let out = meResampler.process(samples, sampleRate: rate)
            meter.add(out, channel: .me)
            peak.update(out, channel: .me)
            writeQueue.async { try? meWriter.append(out) }
        }
        let tap = ProcessTap(target: target) { samples, rate in
            let out = themResampler.process(samples, sampleRate: rate)
            meter.add(out, channel: .them)
            peak.update(out, channel: .them)
            writeQueue.async { try? themWriter.append(out) }
        }

        do {
            try micCapture.start()
            print("Micro : démarré (AEC \(micCapture.voiceProcessingActive ? "actif" : "inactif"))")
        } catch {
            print("Micro : ÉCHEC — \(error)")
        }
        do {
            try tap.start()
            print("Tap : démarré (\(tap.sampleRate.map { "\(Int($0)) Hz" } ?? "?"))")
        } catch {
            print("Tap : ÉCHEC — \(error)")
        }

        let total = max(1, Int(seconds.rounded()))
        for second in 1...total {
            try? await Task.sleep(nanoseconds: 1_000_000_000)
            let l = meter.take()
            print(String(format: "  %3d s   moi %6.1f dBFS   eux %6.1f dBFS", second, l.me, l.them))
        }

        micCapture.stop()
        tap.stop()
        writeQueue.sync {}
        do {
            try meWriter.close()
            try themWriter.close()
        } catch {
            print("ERREUR à la fermeture des WAV : \(error)")
        }

        let peaks = peak.values()
        print("")
        print("me.wav   : \(String(format: "%.1f", meWriter.durationSeconds)) s, crête \(String(format: "%.1f", AudioLevel.dbfs(rms: Double(peaks.me)))) dBFS")
        print("them.wav : \(String(format: "%.1f", themWriter.durationSeconds)) s, crête \(String(format: "%.1f", AudioLevel.dbfs(rms: Double(peaks.them)))) dBFS")
        if peaks.them <= 1e-6 {
            print("ATTENTION : them.wav est silencieux. Teams jouait-il du son ? Permission « Enregistrement audio du système » accordée ?")
        }
        print("Fichiers : \(directory.path)")
        return 0
    }

    // MARK: S2 — transcription

    static func transcribe(file: URL) async -> Int32 {
        print("== Spike S2 : transcription de \(file.lastPathComponent) ==")
        let samples: [Float]
        do {
            samples = try loadMono16k(file)
        } catch {
            print("ERREUR : lecture de \(file.path) impossible : \(error)")
            return 1
        }
        let duration = Double(samples.count) / AudioFormat.sampleRate
        print(String(format: "Audio : %.1f s (16 kHz mono)", duration))

        let models = SpeechModels(backend: FluidAudioBackend())
        let loadStart = Date()
        guard let recognizer = await models.recognizer() else {
            print("ERREUR : modèle ASR indisponible (voir stderr).")
            return 1
        }
        print(String(format: "Modèle %@ chargé en %.1f s", recognizer.name, Date().timeIntervalSince(loadStart)))
        let vad = await models.makeVAD()
        print("VAD : \(vad.name)")

        var segmenter = UtteranceSegmenter(config: SegmenterConfig())
        var utterances: [Utterance] = []
        var i = 0
        while i < samples.count {
            var frame = Array(samples[i..<min(samples.count, i + vad.frameSize)])
            if frame.count < vad.frameSize { frame += [Float](repeating: 0, count: vad.frameSize - frame.count) }
            let p = (try? await vad.speechProbability(frame)) ?? 0
            utterances += segmenter.push(frame, probability: p)
            i += vad.frameSize
        }
        if let last = segmenter.flush() { utterances.append(last) }
        print("\(utterances.count) énoncés détectés\n")

        var latencies: [Double] = []
        var audioTotal = 0.0
        for u in utterances {
            let t = Date()
            let text: String
            do {
                text = try await recognizer.transcribe(u.samples)
            } catch {
                print("[\(clock(u.t0)) → \(clock(u.t1))] ERREUR : \(error)")
                continue
            }
            let elapsed = Date().timeIntervalSince(t)
            latencies.append(elapsed)
            audioTotal += u.duration
            print(String(format: "[%@ → %@] (%.2f s pour %.1f s) %@", clock(u.t0), clock(u.t1), elapsed, u.duration,
                         text.trimmingCharacters(in: .whitespacesAndNewlines)))
        }

        if !latencies.isEmpty {
            let mean = latencies.reduce(0, +) / Double(latencies.count)
            let maxL = latencies.max() ?? 0
            let rtf = latencies.reduce(0, +) / max(audioTotal, 0.001)
            print("")
            print(String(format: "Temps par phrase : moyen %.2f s, max %.2f s ; facteur temps réel %.3f", mean, maxL, rtf))
            print("Critère S2 : < 1 s par phrase → \(maxL < 1 ? "OK" : "À ÉTUDIER")")
        }
        return 0
    }

    // MARK: Outils de diagnostic

    /// Liste les processus connus du HAL (pour repérer les processus audio de Teams, spike S1).
    static func listAudioProcesses() -> Int32 {
        let objects = (try? HAL.processObjectList()) ?? []
        print("Processus audio (HAL) : \(objects.count)")
        print("objet   pid     entrée sortie  Teams  bundle")
        let teams = Set(TeamsMonitor.scan().processObjects)
        for object in objects {
            let pid = HAL.processPID(object).map { String($0) } ?? "?"
            let bundle = HAL.processBundleID(object) ?? "—"
            let input = HAL.processIsRunningInput(object) ? "oui" : "non"
            let output = HAL.processIsRunningOutput(object) ? "oui" : "non"
            let isTeams = teams.contains(object) ? "oui" : "non"
            print([pad(String(object), 7), pad(pid, 7), pad(input, 6), pad(output, 7), pad(isTeams, 6), bundle]
                .joined(separator: " "))
        }
        return 0
    }

    /// Télécharge et compile les modèles (ASR, VAD, diarisation) : appelé par `make install`.
    static func prefetchModels() async -> Int32 {
        let models = SpeechModels(backend: FluidAudioBackend())
        print("Téléchargement / chargement du modèle de transcription…")
        guard await models.recognizer() != nil else {
            print("ERREUR : modèle de transcription indisponible (voir stderr).")
            return 1
        }
        print("Modèle de transcription : OK")
        let vad = await models.makeVAD()
        print("VAD : \(vad.name)\(vad.name == "silero" ? "" : " (Silero indisponible, repli)")")
        do {
            _ = try await models.diarizer()
            print("Diarisation : OK")
        } catch {
            print("Diarisation : ÉCHEC — \(error)")
            return 1
        }
        return 0
    }

    /// Lit un fichier audio quelconque et le convertit en 16 kHz mono Float32.
    static func loadMono16k(_ url: URL) throws -> [Float] {
        let file = try AVAudioFile(forReading: url)
        let format = file.processingFormat
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(file.length)) else {
            throw CocoaError(.fileReadCorruptFile)
        }
        try file.read(into: buffer)
        let mono = AudioBufferUtils.monoSamples(from: buffer, mix: .average)
        return Resampler.resampleAll(mono, from: format.sampleRate)
    }

    private static func pad(_ s: String, _ width: Int) -> String {
        s.count >= width ? s : s + String(repeating: " ", count: width - s.count)
    }

    private static func clock(_ t: Double) -> String {
        let m = Int(t) / 60
        let s = t - Double(m * 60)
        return String(format: "%02d:%04.1f", m, s)
    }
}

/// Crête absolue par canal (spike S1).
final class PeakTracker {
    private let lock = NSLock()
    private var me: Float = 0
    private var them: Float = 0

    func update(_ samples: [Float], channel: Channel) {
        let p = samples.reduce(Float(0)) { max($0, abs($1)) }
        lock.lock()
        if channel == .me { me = max(me, p) } else { them = max(them, p) }
        lock.unlock()
    }

    func values() -> (me: Float, them: Float) {
        lock.lock()
        defer { lock.unlock() }
        return (me, them)
    }
}
