// Modes « spike » en ligne de commande (hors protocole, sortie lisible sur stdout).
//
//   scribe-audio --spike-record <secondes> <dossier> [--apps teams,slack]
//                                                      → me.wav + them.wav (spike S1)
//   scribe-audio --spike-transcribe <fichier.wav>      → transcription + temps par phrase (spike S2)

import AVFoundation
import CoreAudio
import Foundation

@available(macOS 14.2, *)
enum Spikes {
    // MARK: S1 — capture

    static func record(seconds: Double, directory: URL, apps: [WatchedApp] = WatchedApp.defaults) async -> Int32 {
        let names = apps.map(\.name).joined(separator: ", ")
        print("== Spike S1 : capture micro + \(names) pendant \(Int(seconds)) s ==")
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

        let snapshot = CallMonitor.scan(apps: apps)
        for entry in snapshot.apps {
            guard !entry.processObjects.isEmpty else {
                print("\(entry.app.name) : aucun processus audio trouvé")
                continue
            }
            print("\(entry.app.name) : \(entry.processObjects.count) processus audio \(entry.processObjects), micro utilisé : \(entry.inputActive ? "oui" : "non")")
            print("  titre de l'appel (accessibilité) : \(CallWindowTitle.read(app: entry.app, pids: entry.appPIDs) ?? "—")")
        }
        let processes = snapshot.processObjects(forApps: apps.map(\.id))
        let selfObject = HAL.processObject(forPID: getpid()).map { [$0] } ?? []
        let target: ProcessTap.Target
        if CaptureSession.TapMode.fromEnvironment == .global {
            print("SCRIBE_TAP_MODE=global → tap global (tout le son du Mac sauf scribe-audio).")
            target = .systemExcluding(selfObject)
        } else if processes.isEmpty {
            print("Aucun processus audio pour \(names) → tap global (tout le son du Mac sauf scribe-audio).")
            target = .systemExcluding(selfObject)
        } else {
            target = .processes(processes)
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
            print("ATTENTION : them.wav est silencieux. \(names) jouait-il du son ? Permission « Enregistrement audio du système » accordée ?")
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

    /// Liste les processus connus du HAL et l'app reconnue pour chacun (liste par défaut, ou
    /// `--apps`), pour repérer les processus audio des apps de visio (spike S1).
    static func listAudioProcesses(apps: [WatchedApp] = WatchedApp.defaults) -> Int32 {
        let objects = (try? HAL.processObjectList()) ?? []
        print("Processus audio (HAL) : \(objects.count) — apps reconnues : \(apps.map(\.id).joined(separator: ", "))")
        print("objet   pid     entrée sortie  app        bundle")
        let owners = CallMonitor.scan(apps: apps).appByProcess
        for object in objects {
            let pid = HAL.processPID(object).map { String($0) } ?? "?"
            let bundle = HAL.processBundleID(object) ?? "—"
            let input = HAL.processIsRunningInput(object) ? "oui" : "non"
            let output = HAL.processIsRunningOutput(object) ? "oui" : "non"
            let app = owners[object]?.id ?? "—"
            print([pad(String(object), 7), pad(pid, 7), pad(input, 6), pad(output, 7), pad(app, 10), bundle]
                .joined(separator: " "))
        }
        return 0
    }

    /// Liste les micros et sorties tels que le message `devices` les présente (avec leur UID, à
    /// passer dans `start.input_device`, `set_input` ou `play`), puis tous les périphériques du
    /// HAL avec la raison de leur exclusion éventuelle.
    static func listDevices() -> Int32 {
        let raw = AudioDevices.rawDevices()
        let list = AudioDeviceCatalog.build(raw,
                                            defaultInput: AudioDevices.defaultInputDeviceID(),
                                            defaultOutput: AudioDevices.defaultOutputDeviceID())
        for (title, devices) in [("Entrées (micros)", list.inputs), ("Sorties", list.outputs)] {
            print("== \(title) : \(devices.count) ==")
            print("défaut  transport  nom                              UID")
            for d in devices {
                print([pad(d.isDefault ? "oui" : "", 7), pad(d.transport.rawValue, 10), pad(d.name, 32), d.id]
                    .joined(separator: " "))
            }
            print("")
        }
        print("== Tous les périphériques du HAL : \(raw.count) ==")
        print("objet  transport  entrée sortie  remarque          nom / UID")
        for d in raw {
            let note = AudioDeviceCatalog.exclusionReason(d)
                ?? (d.inputChannels == 0 && d.outputChannels == 0 ? "aucun canal" : "")
            let transport = "\(AudioDeviceCatalog.fourCCString(d.transportType)) \(AudioDeviceCatalog.classify(transportType: d.transportType).rawValue)"
            print([pad(String(d.objectID), 6), pad(transport, 10), pad(String(d.inputChannels), 6),
                   pad(String(d.outputChannels), 7), pad(note, 17), "\(d.name ?? "—") / \(d.uid ?? "—")"]
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

// MARK: - Argument `--apps`

extension WatchedApp {
    /// Apps du catalogue désignées par `--apps teams,slack` (sans casse). `unknown` liste les
    /// identifiants inconnus ; une liste vide donne la liste par défaut.
    static func fromCatalog(argument: String) -> (apps: [WatchedApp], unknown: [String]) {
        var apps: [WatchedApp] = []
        var unknown: [String] = []
        for raw in argument.split(separator: ",") {
            let id = raw.trimmingCharacters(in: .whitespaces).lowercased()
            guard !id.isEmpty else { continue }
            if let app = catalog.first(where: { $0.id == id }) {
                if !apps.contains(app) { apps.append(app) }
            } else {
                unknown.append(id)
            }
        }
        if apps.isEmpty, unknown.isEmpty { apps = defaults }
        return (apps, unknown)
    }
}
