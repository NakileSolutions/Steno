// Capture du micro (canal « me ») avec AVAudioEngine et voice processing (annulation d'écho).
//
// Robustesse :
// - AVAudioEngineConfigurationChange (casque branché/débranché, AirPods, changement de
//   fréquence) et changement du micro par défaut → moteur recréé, objectif < 5 s ;
// - micro choisi (`start.input_device`, `set_input`) : réglé sur l'unité du nœud d'entrée ;
//   s'il est absent ou débranché, repli sur le micro par défaut (`onDeviceMissing`), puis
//   reprise dès qu'il réapparaît dans la liste des périphériques ;
// - si le voice processing empêche le démarrage (casque Bluetooth en appel, certains
//   agrégats), repli sur une capture directe par le HAL (`HALInputCapture`), qui ne touche
//   pas à la sortie audio. Le voice processing n'est alors plus retenté pendant la capture :
//   chaque essai reconfigure la sortie et perturbe le son de l'app de visio.

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
    /// Appelé (une fois par absence) quand le micro demandé est introuvable, au démarrage, après
    /// `setInputDevice` ou quand il est débranché : la capture se replie sur le micro par défaut.
    /// Argument : l'UID demandé. À fixer avant `start()`.
    var onDeviceMissing: ((String) -> Void)?

    private let onSamples: SampleHandler
    private let wantsVoiceProcessing: Bool
    private let queue = DispatchQueue(label: "steno.audio.mic.control")
    private var engine: AVAudioEngine?
    /// Capture directe (sans voice processing).
    private var hal: HALInputCapture?
    /// Le voice processing a échoué : on ne le retente plus pendant cette capture.
    private var voiceProcessingFailed = false
    private var configObserver: NSObjectProtocol?
    private var defaultInputListener: HALPropertyListener?
    private var devicesListener: HALPropertyListener?
    private var pendingRestart: DispatchWorkItem?
    private var restartAttempts = 0
    private var wantsRunning = false
    private(set) var voiceProcessingActive = false

    // Choix du micro (tout sur `queue`).
    /// UID demandé (`start.input_device` / `set_input`) ; nil = micro par défaut du système.
    private var requestedUID: String?
    /// Vrai quand le micro demandé est absent et que le micro par défaut le remplace.
    private var usingFallback = false
    /// `onDeviceMissing` déjà appelé pour l'absence en cours.
    private var missingReported = false

    /// Délais successifs entre deux tentatives (total < 5 s avant de signaler l'échec).
    private static let retryDelays: [TimeInterval] = [0.3, 0.7, 1.5, 2.0]

    /// `inputDevice` : UID du micro à utiliser (nil = micro par défaut, en suivant ses changements).
    init(voiceProcessing: Bool = true, inputDevice: String? = nil, onSamples: @escaping SampleHandler) {
        self.wantsVoiceProcessing = voiceProcessing
        self.requestedUID = Self.normalized(inputDevice)
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
                    self?.defaultInputChanged()
                }
            }
            if devicesListener == nil {
                devicesListener = HALPropertyListener(
                    selector: kAudioHardwarePropertyDevices, queue: queue
                ) { [weak self] in
                    self?.deviceListChanged()
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
            devicesListener = nil
            teardownEngine()
        }
    }

    /// Redémarrage forcé (réveil de veille).
    func restartNow(reason: String) {
        scheduleRestart(reason: reason, delay: 0.5)
    }

    /// Change de micro pendant la capture (`set_input`) : seul le moteur micro redémarre.
    /// nil = micro par défaut du système.
    func setInputDevice(_ uid: String?) {
        queue.async { [weak self] in
            guard let self else { return }
            let wanted = Self.normalized(uid)
            let label = wanted.map { "« \($0) »" } ?? "micro par défaut"
            if wanted == self.requestedUID, !self.usingFallback, self.engine != nil || self.hal != nil {
                Log.info("micro : \(label) déjà utilisé")
                return
            }
            self.requestedUID = wanted
            self.usingFallback = false
            self.missingReported = false
            self.restartAttempts = 0
            self.scheduleRestart(reason: "changement de micro (\(label))", delay: 0.05)
        }
    }

    // MARK: Choix du périphérique (sur `queue`)

    /// Périphérique à ouvrir : le micro demandé s'il est présent, sinon nil (micro par défaut).
    /// Signale l'absence une fois par épisode et note le retour du micro.
    private func resolveDevice() -> AudioDeviceID? {
        guard let uid = requestedUID else {
            usingFallback = false
            return nil
        }
        if let id = AudioDevices.inputDeviceID(forUID: uid) {
            if usingFallback || missingReported { Log.info("micro « \(uid) » de nouveau disponible : repris") }
            usingFallback = false
            missingReported = false
            return id
        }
        usingFallback = true
        if !missingReported {
            missingReported = true
            Log.warn("micro « \(uid) » introuvable : repli sur le micro par défaut")
            onDeviceMissing?(uid)
        }
        return nil
    }

    /// Le micro par défaut a changé : ne concerne que le mode « micro par défaut » (ou le repli).
    private func defaultInputChanged() {
        guard wantsRunning, requestedUID == nil || usingFallback else { return }
        scheduleRestart(reason: "micro par défaut modifié", delay: 0.3)
    }

    /// Liste des périphériques modifiée : micro demandé débranché ou rebranché ?
    /// (La création de l'agrégat privé du tap modifie aussi la liste : sans effet ici.)
    /// À VÉRIFIER (S1-ter) : des AirPods qui réapparaissent exposent-ils leurs canaux d'entrée dès
    /// cette notification ? Sinon la reprise attend le changement suivant (liste ou configuration).
    private func deviceListChanged() {
        guard wantsRunning, let uid = requestedUID else { return }
        let present = AudioDevices.inputDeviceID(forUID: uid) != nil
        if present, usingFallback {
            scheduleRestart(reason: "micro « \(uid) » rebranché", delay: 0.5)
        } else if !present, !usingFallback {
            scheduleRestart(reason: "micro « \(uid) » débranché", delay: 0.3)
        }
    }

    // MARK: Moteur

    /// À appeler sur `queue`.
    private func startEngine() throws {
        teardownEngine()
        let device = resolveDevice()
        if wantsVoiceProcessing && !voiceProcessingFailed {
            do {
                try buildAndStart(voiceProcessing: true, device: device)
                return
            } catch {
                voiceProcessingFailed = true
                Log.warn("micro avec voice processing impossible (\(error)) : capture directe sans annulation d'écho pour la suite")
                teardownEngine()
            }
        }
        try startDirect(device: device)
    }

    /// Capture directe du micro (HAL), sans voice processing ni sortie audio.
    private func startDirect(device: AudioDeviceID?) throws {
        guard let id = device ?? AudioDevices.defaultInputDeviceID() else { throw MicError.noInputDevice }
        let capture = HALInputCapture(device: id, onSamples: onSamples)
        try capture.start()
        hal = capture
        voiceProcessingActive = false
        Log.info("micro démarré (capture directe) : \(AudioDevices.describe(id)), \(Int(capture.rate)) Hz, \(capture.channelCount) canaux, AEC inactif")
    }

    /// `device` nil = micro par défaut du système (rien n'est réglé sur le nœud d'entrée).
    private func buildAndStart(voiceProcessing: Bool, device: AudioDeviceID?) throws {
        let engine = AVAudioEngine()
        let input = engine.inputNode

        // Ordre retenu : périphérique réglé AVANT le voice processing, puis relu après son
        // activation (qui remplace l'unité d'E/S par VoiceProcessingIO) et réglé de nouveau s'il
        // a été perdu. À VÉRIFIER (spike S1) : lequel des deux réglages est réellement pris en
        // compte (le journal indique si le second a été nécessaire).
        if let device {
            try AudioDevices.setCurrentDevice(device, on: input)
        }

        if voiceProcessing {
            // AEC : retire de la piste micro le son joué par les haut-parleurs.
            try input.setVoiceProcessingEnabled(true)
            // Ne pas baisser le volume des autres apps (Teams, Slack…) pendant la capture.
            // À VÉRIFIER (spike S1) : effet réel du réglage sur le son de l'app de visio.
            input.voiceProcessingOtherAudioDuckingConfiguration =
                AVAudioVoiceProcessingOtherAudioDuckingConfiguration(enableAdvancedDucking: false,
                                                                     duckingLevel: .min)
            if let device, AudioDevices.currentDevice(of: input) != device {
                Log.info("micro : périphérique perdu à l'activation du voice processing, nouveau réglage")
                try AudioDevices.setCurrentDevice(device, on: input)
            }
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

        let actual = AudioDevices.currentDevice(of: input)
        if let device, let actual, actual != device {
            Log.warn("micro : \(AudioDevices.describe(device)) demandé mais \(AudioDevices.describe(actual)) ouvert")
        }
        let which = actual.map { AudioDevices.describe($0) } ?? (device == nil ? "micro par défaut" : "?")
        Log.info("micro démarré : \(which), \(Int(format.sampleRate)) Hz, \(format.channelCount) canaux, AEC \(voiceProcessing ? "actif" : "inactif")")
    }

    private func teardownEngine() {
        hal?.stop()
        hal = nil
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

    /// UID nettoyé ; vide = nil (micro par défaut).
    private static func normalized(_ uid: String?) -> String? {
        guard let uid = uid?.trimmingCharacters(in: .whitespacesAndNewlines), !uid.isEmpty else { return nil }
        return uid
    }
}

// MARK: - Enregistrement ponctuel (test du mot d'appel hors réunion)

@available(macOS 14.2, *)
enum MicRecorder {
    /// Enregistre `seconds` secondes de micro, rééchantillonnées en 16 kHz mono.
    /// `inputDevice` : UID du micro (nil = micro par défaut ; repli silencieux s'il est absent).
    static func record(seconds: Double, inputDevice: String? = nil) async throws -> [Float] {
        let resampler = Resampler()
        let lock = NSLock()
        var collected: [Float] = []
        let mic = MicCapture(voiceProcessing: true, inputDevice: inputDevice) { samples, rate in
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
