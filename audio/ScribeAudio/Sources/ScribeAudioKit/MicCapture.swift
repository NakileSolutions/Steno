// Capture du micro (canal « me ») avec AVAudioEngine et voice processing (annulation d'écho).
//
// Robustesse :
// - AVAudioEngineConfigurationChange (casque branché/débranché, AirPods, changement de
//   fréquence) et changement du micro par défaut → moteur recréé, objectif < 5 s ;
// - si le voice processing empêche le démarrage (certains périphériques agrégés), repli
//   sans AEC, signalé dans les journaux.

import AudioToolbox
import AVFoundation
import CoreAudio
import Foundation

enum MicError: Error, CustomStringConvertible {
    case noInputDevice
    case restartFailed(String)

    var description: String {
        switch self {
        case .noInputDevice: return "aucun micro disponible"
        case let .restartFailed(why): return "redémarrage du micro impossible : \(why)"
        }
    }
}

@available(macOS 14.2, *)
final class MicCapture {
    typealias SampleHandler = (_ samples: [Float], _ sampleRate: Double) -> Void

    /// Appelé après plusieurs échecs de redémarrage (la capture réessaie toutes les 5 s).
    var onPersistentFailure: ((Error) -> Void)?
    /// Appelé quand le micro est reparti après une interruption.
    var onRecovered: (() -> Void)?

    private let onSamples: SampleHandler
    private let wantsVoiceProcessing: Bool
    private let queue = DispatchQueue(label: "scribe.audio.mic.control")
    private var engine: AVAudioEngine?
    private var configObserver: NSObjectProtocol?
    private var defaultInputListener: HALPropertyListener?
    private var pendingRestart: DispatchWorkItem?
    private var restartAttempts = 0
    private var wantsRunning = false
    private(set) var voiceProcessingActive = false

    /// Délais successifs entre deux tentatives (total < 5 s avant de signaler l'échec).
    private static let retryDelays: [TimeInterval] = [0.3, 0.7, 1.5, 2.0]

    init(voiceProcessing: Bool = true, onSamples: @escaping SampleHandler) {
        self.wantsVoiceProcessing = voiceProcessing
        self.onSamples = onSamples
    }

    // Pas de `stop()` dans deinit : il fait un `queue.sync` qui bloquerait si la dernière
    // référence était relâchée depuis `queue`. Le moteur s'arrête avec sa libération ;
    // les propriétaires appellent `stop()` explicitement.

    func start() throws {
        try queue.sync {
            wantsRunning = true
            try startEngine()
            if defaultInputListener == nil {
                defaultInputListener = HALPropertyListener(
                    selector: kAudioHardwarePropertyDefaultInputDevice, queue: queue
                ) { [weak self] in
                    self?.scheduleRestart(reason: "micro par défaut modifié", delay: 0.3)
                }
            }
        }
    }

    func stop() {
        queue.sync {
            wantsRunning = false
            pendingRestart?.cancel()
            pendingRestart = nil
            defaultInputListener = nil
            teardownEngine()
        }
    }

    /// Redémarrage forcé (réveil de veille).
    func restartNow(reason: String) {
        scheduleRestart(reason: reason, delay: 0.5)
    }

    // MARK: Moteur

    /// À appeler sur `queue`.
    private func startEngine() throws {
        teardownEngine()
        do {
            try buildAndStart(voiceProcessing: wantsVoiceProcessing)
        } catch {
            guard wantsVoiceProcessing else { throw error }
            Log.warn("micro avec voice processing impossible (\(error)) : repli sans annulation d'écho")
            teardownEngine()
            try buildAndStart(voiceProcessing: false)
        }
    }

    private func buildAndStart(voiceProcessing: Bool) throws {
        let engine = AVAudioEngine()
        let input = engine.inputNode

        if voiceProcessing {
            // AEC : retire de la piste micro le son joué par les haut-parleurs.
            try input.setVoiceProcessingEnabled(true)
            // Ne pas baisser le volume des autres apps (Teams, Slack…) pendant la capture.
            // À VÉRIFIER (spike S1) : effet réel du réglage sur le son de l'app de visio.
            input.voiceProcessingOtherAudioDuckingConfiguration =
                AVAudioVoiceProcessingOtherAudioDuckingConfiguration(enableAdvancedDucking: false,
                                                                     duckingLevel: .min)
        }
        voiceProcessingActive = voiceProcessing

        // Le voice processing pilote entrée ET sortie : le graphe doit contenir la sortie.
        _ = engine.mainMixerNode

        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { throw MicError.noInputDevice }

        input.installTap(onBus: 0, bufferSize: 4096, format: format) { [weak self] buffer, _ in
            guard let self else { return }
            // Avec le voice processing, le canal 0 porte la voix traitée.
            let mono = AudioBufferUtils.monoSamples(from: buffer, mix: .firstChannel)
            if !mono.isEmpty { self.onSamples(mono, buffer.format.sampleRate) }
        }

        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            throw error
        }
        self.engine = engine

        configObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in
            self?.scheduleRestart(reason: "configuration audio modifiée", delay: 0.3)
        }
        Log.info("micro démarré (\(Int(format.sampleRate)) Hz, \(format.channelCount) canaux, AEC \(voiceProcessing ? "actif" : "inactif"))")
    }

    private func teardownEngine() {
        if let configObserver {
            NotificationCenter.default.removeObserver(configObserver)
        }
        configObserver = nil
        if let engine {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }
        engine = nil
    }

    // MARK: Redémarrage

    private func scheduleRestart(reason: String, delay: TimeInterval) {
        queue.async { [weak self] in
            guard let self, self.wantsRunning else { return }
            if self.restartAttempts == 0 { Log.info("micro : \(reason), redémarrage") }
            self.pendingRestart?.cancel()
            let item = DispatchWorkItem { [weak self] in self?.attemptRestart() }
            self.pendingRestart = item
            self.queue.asyncAfter(deadline: .now() + delay, execute: item)
        }
    }

    private func attemptRestart() {
        guard wantsRunning else { return }
        do {
            try startEngine()
            if restartAttempts > 0 { Log.info("micro reparti après \(restartAttempts) nouvel(s) essai(s)") }
            restartAttempts = 0
            onRecovered?()
        } catch {
            restartAttempts += 1
            Log.warn("micro : tentative \(restartAttempts) échouée (\(error))")
            if restartAttempts == Self.retryDelays.count {
                onPersistentFailure?(MicError.restartFailed("\(error)"))
            }
            let delay = restartAttempts <= Self.retryDelays.count
                ? Self.retryDelays[restartAttempts - 1]
                : 5.0
            let item = DispatchWorkItem { [weak self] in self?.attemptRestart() }
            pendingRestart = item
            queue.asyncAfter(deadline: .now() + delay, execute: item)
        }
    }
}

// MARK: - Enregistrement ponctuel (test du mot d'appel hors réunion)

@available(macOS 14.2, *)
enum MicRecorder {
    /// Enregistre `seconds` secondes de micro, rééchantillonnées en 16 kHz mono.
    static func record(seconds: Double) async throws -> [Float] {
        let resampler = Resampler()
        let lock = NSLock()
        var collected: [Float] = []
        let mic = MicCapture(voiceProcessing: true) { samples, rate in
            let out = resampler.process(samples, sampleRate: rate)
            lock.lock()
            collected += out
            lock.unlock()
        }
        try mic.start()
        try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
        mic.stop()
        lock.lock()
        defer { lock.unlock() }
        return collected
    }
}
