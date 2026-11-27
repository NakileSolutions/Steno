// Session de capture : du `start` au `stop`.
//
// Possède le micro, le tap des apps de visio, les deux chaînes de traitement, les WAV
// éventuels et la minuterie des niveaux. Gère la recréation du tap (processus des apps captées
// modifiés ou apparus, sortie par défaut modifiée, réveil), le changement de micro en cours de
// capture (`set_input` : seul le micro redémarre, le trou est comblé par la chaîne « me ») et le
// mode aparté.

import AppKit
import CoreAudio
import Foundation

/// Ce que le tap doit capter : les processus des apps sélectionnées par `start.apps`.
struct TapSource: Equatable {
    /// Noms des apps captées (messages et journaux).
    var appNames: [String]
    /// Union des objets processus HAL de ces apps (triée).
    var processes: [AudioObjectID]

    static let empty = TapSource(appNames: [], processes: [])

    var label: String {
        appNames.isEmpty ? "aucune app" : appNames.joined(separator: ", ")
    }
}

@available(macOS 14.2, *)
final class CaptureSession {
    struct Config {
        var vocabulary: [String]
        var silenceMs: Int
        var keepAudio: Bool
        var audioDir: URL?
        /// UID du micro (nil = micro par défaut du système, en suivant ses changements).
        var inputDevice: String? = nil
        /// Réunion en présentiel : micro seul, sans tap ni voice processing (rien n'est joué,
        /// et le traitement de la voix atténuerait les personnes éloignées du micro).
        var room: Bool = false
    }

    enum TapMode {
        /// Arbre de processus des apps captées (par défaut).
        case apps
        /// Plan B : tout le son du système sauf Sténo (`STENO_TAP_MODE=global`).
        case global

        static var fromEnvironment: TapMode {
            ProcessInfo.processInfo.environment["STENO_TAP_MODE"] == "global" ? .global : .apps
        }
    }

    let startDate = Date()
    let startUptime = ProcessInfo.processInfo.systemUptime

    private let config: Config
    private let output: Output
    private let permissions: PermissionsTracker
    private let tapMode: TapMode
    private let meter: LevelMeter
    private let levels: LevelEmitter
    private let errorLimiter = RateLimiter(interval: 30)

    private let meResampler = Resampler()
    private let themResampler = Resampler()
    private var mePipeline: ChannelPipeline!
    private var themPipeline: ChannelPipeline!
    private var mic: MicCapture?

    // Tap : tout est manipulé sur `tapQueue`.
    private let tapQueue = DispatchQueue(label: "steno.audio.tap.control")
    private var tap: ProcessTap?
    private var tapTarget: ProcessTap.Target?
    private var tapSource = TapSource.empty
    /// Aucun processus à taper au démarrage : on attend qu'ils apparaissent.
    private var waitingForProcesses = false
    private var tapRetry: DispatchWorkItem?
    private var tapFailures = 0
    private var running = false
    private var outputDeviceListeners: [HALPropertyListener] = []

    private var wakeObserver: NSObjectProtocol?
    private let systemAudioSeen = OnceFlag()

    // Aparté et collecteurs (test du mot d'appel pendant une réunion).
    private let lock = NSLock()
    private var asideIntervals: [(start: Double, end: Double?)] = []
    private var micMuted = false
    private var paused = false
    private var micCollectors: [UUID: ([Float]) -> Void] = [:]

    init(config: Config, output: Output, permissions: PermissionsTracker, models: SpeechModels,
         tapMode: TapMode = .fromEnvironment)
    {
        self.config = config
        self.output = output
        self.permissions = permissions
        self.tapMode = tapMode
        let meter = LevelMeter()
        self.meter = meter
        self.levels = LevelEmitter(meter: meter, output: output)

        var meWav: WavWriter?
        var themWav: WavWriter?
        if config.keepAudio {
            if let dir = config.audioDir {
                do {
                    meWav = try WavWriter(url: dir.appendingPathComponent("me.wav"))
                    themWav = try WavWriter(url: dir.appendingPathComponent("them.wav"))
                    Log.info("audio conservé dans \(dir.path)")
                } catch {
                    Log.error("impossible de créer les WAV dans \(dir.path) : \(error)")
                }
            } else {
                Log.warn("keep_audio sans audio_dir : aucun fichier audio ne sera écrit")
            }
        }

        let uptime = startUptime
        let makePipeline = { [weak self] (channel: Channel, wav: WavWriter?) -> ChannelPipeline in
            ChannelPipeline(
                channel: channel,
                sessionStartUptime: uptime,
                silenceMs: config.silenceMs,
                vocabulary: config.vocabulary,
                models: models,
                wav: wav,
                meter: meter,
                onSegment: { utterance, text, speaker in
                    self?.emitSegment(utterance, channel: channel, text: text, speaker: speaker)
                },
                onError: { code, message in self?.reportError(code, message, recoverable: true) }
            )
        }
        mePipeline = makePipeline(.me, meWav)
        themPipeline = makePipeline(.them, themWav)
    }

    // MARK: Cycle de vie

    func start(tapSource initial: TapSource) async {
        mePipeline.start()
        themPipeline.start()
        levels.start()

        // Micro (canal « me »).
        let micPermission = await PermissionsTracker.requestMicrophoneIfNeeded()
        if micPermission == .denied {
            reportError(.micDenied, "Accès au micro refusé : autorisez Sténo dans Réglages Système › Confidentialité et sécurité › Micro.", recoverable: false)
        } else {
            let mic = MicCapture(voiceProcessing: !config.room, inputDevice: config.inputDevice) { [weak self] samples, rate in
                self?.handleMicSamples(samples, sampleRate: rate)
            }
            mic.onDeviceMissing = { [weak self] uid in
                // Pas de limiteur : MicCapture ne signale qu'une fois par absence.
                self?.output.send(.error(code: .deviceMissing, message: DeviceMessages.inputMissing(uid), recoverable: true))
            }
            mic.onPersistentFailure = { [weak self] error in
                self?.reportError(.deviceChanged, "Micro indisponible après un changement de périphérique (\(error)). Nouvel essai toutes les 5 s.", recoverable: true)
            }
            do {
                try mic.start()
                self.mic = mic
            } catch {
                reportError(.deviceChanged, "Démarrage du micro impossible : \(error)", recoverable: true)
                // On garde l'objet : il réessaiera au prochain changement de périphérique.
                self.mic = mic
                mic.restartNow(reason: "échec au démarrage")
            }
        }

        // Tap (canal « them ») : aucun en présentiel.
        if config.room {
            Log.info("réunion en présentiel : micro seul")
        } else { tapQueue.sync {
            running = true
            tapSource = initial
            // When system audio access is denied, macOS still creates the tap without error and
            // delivers silence: ask TCC first so the user learns why "them" stays empty. The tap
            // is created anyway: if the answer is wrong (private API), real audio proves it and
            // `handleTapSamples` records the permission as granted.
            if permissions.currentSystemAudio() == .denied {
                Log.warn("system audio access denied (TCC): the tap will only receive silence")
                permissions.setSystemAudio(.denied)
                reportError(.systemAudioDenied, Self.systemAudioDeniedMessage, recoverable: false)
            }
            for selector in [kAudioHardwarePropertyDefaultSystemOutputDevice, kAudioHardwarePropertyDefaultOutputDevice] {
                if let l = HALPropertyListener(selector: selector, queue: tapQueue, handler: { [weak self] in
                    self?.tapQueue.async { self?.reconcileTap(force: true, reason: "sortie audio par défaut modifiée") }
                }) {
                    outputDeviceListeners.append(l)
                }
            }
            reconcileTap(force: false, reason: "démarrage")
            if tapMode == .apps, initial.processes.isEmpty {
                // App pas encore lancée (ou sans client audio) : le micro tourne quand même et
                // le tap sera créé dès que ses processus apparaîtront (`updateTapSource`).
                let message: String
                if initial.appNames.isEmpty {
                    message = "Aucune app de visio n'est surveillée : seul le micro est capté, pas le son des participants."
                } else {
                    message = "Aucun processus audio de \(initial.label) : l'app n'est pas lancée ou n'a pas encore ouvert l'audio. Le micro est capté ; le son des participants le sera dès que \(initial.label) apparaîtra."
                }
                reportError(.tapFailed, message, recoverable: true)
            }
        } }

        // Veille / réveil.
        wakeObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification, object: nil, queue: nil
        ) { [weak self] _ in
            guard let self else { return }
            Log.info("réveil du Mac : recréation du micro et du tap")
            self.mic?.restartNow(reason: "réveil")
            self.tapQueue.async { self.reconcileTap(force: true, reason: "réveil") }
        }
    }

    /// Arrête la capture puis vide les segments en cours (attend leur transcription).
    func stop() async {
        if let wakeObserver { NSWorkspace.shared.notificationCenter.removeObserver(wakeObserver) }
        wakeObserver = nil
        mic?.stop()
        mic = nil
        tapQueue.sync {
            running = false
            tapRetry?.cancel()
            tapRetry = nil
            outputDeviceListeners.removeAll()
            tap?.stop()
            tap = nil
            tapTarget = nil
        }
        levels.stop()
        await mePipeline.finish()
        await themPipeline.finish()
    }

    // MARK: Micro

    /// Change de micro sans interrompre la capture (`set_input`). nil = micro par défaut.
    /// L'horloge `t0/t1` continue : le trou éventuel est comblé par `ChannelPipeline`, et le
    /// rééchantillonneur s'adapte à la fréquence du nouveau micro.
    func setInputDevice(_ uid: String?) {
        guard let mic else {
            Log.warn("set_input : micro inactif (accès refusé ?), changement ignoré")
            return
        }
        mic.setInputDevice(uid)
    }

    // MARK: Apps captées et tap

    /// Nouvelle liste de processus à capter (relevé du moniteur toutes les 2 s, ou `watch`).
    func updateTapSource(_ source: TapSource) {
        tapQueue.async { [weak self] in
            guard let self, self.running, source != self.tapSource else { return }
            let reason = source.appNames == self.tapSource.appNames
                ? "processus de \(source.label) modifiés"
                : "apps captées : \(source.label)"
            self.tapSource = source
            self.reconcileTap(force: false, reason: reason)
        }
    }

    /// À appeler sur `tapQueue`.
    private func reconcileTap(force: Bool, reason: String) {
        guard running else { return }
        let desired: ProcessTap.Target?
        switch tapMode {
        case .global:
            let me = HAL.processObject(forPID: getpid())
            desired = .systemExcluding(me.map { [$0] } ?? [])
        case .apps:
            // À VÉRIFIER (spike S1-bis) : un seul tap sur l'union des processus de plusieurs apps.
            desired = tapSource.processes.isEmpty ? nil : .processes(tapSource.processes)
        }
        if !force, desired == tapTarget, (tap != nil) == (desired != nil) { return }

        Log.info("tap : \(reason)")
        tapRetry?.cancel()
        tapRetry = nil
        tap?.stop()
        tap = nil
        tapTarget = desired
        guard let desired else {
            // Mode apps sans processus (app fermée ou pas encore lancée) : en attente.
            waitingForProcesses = true
            Log.info("tap : aucun processus de \(tapSource.label) à capter, en attente de leur apparition")
            return
        }

        let newTap = ProcessTap(target: desired) { [weak self] samples, rate in
            self?.handleTapSamples(samples, sampleRate: rate)
        }
        do {
            try newTap.start()
            tap = newTap
            tapFailures = 0
            if waitingForProcesses {
                waitingForProcesses = false
                Log.info("processus de \(tapSource.label) apparus : capture du son des participants démarrée")
            }
        } catch ProcessTap.TapError.permissionDenied(let status) {
            Log.error("tap refusé (OSStatus \(status))")
            permissions.setSystemAudio(.denied)
            reportError(.systemAudioDenied, Self.systemAudioDeniedMessage, recoverable: false)
        } catch {
            tapFailures += 1
            Log.error("tap : \(error)")
            let what = tapMode == .global ? "du Mac" : "de \(tapSource.label)"
            reportError(.tapFailed, "Capture du son \(what) impossible (\(error)). Nouvel essai en cours.", recoverable: true)
            let delay = min(10.0, Double(tapFailures) * 2)
            let item = DispatchWorkItem { [weak self] in self?.reconcileTap(force: true, reason: "nouvel essai") }
            tapRetry = item
            tapQueue.asyncAfter(deadline: .now() + delay, execute: item)
        }
    }

    // MARK: Audio

    private func handleMicSamples(_ samples: [Float], sampleRate: Double) {
        let out = meResampler.process(samples, sampleRate: sampleRate)
        guard !out.isEmpty else { return }
        lock.lock()
        let muted = micMuted || paused
        lock.unlock()
        // Micro coupé : du silence à la place (VAD muette, WAV et horloge continus).
        mePipeline.feed(muted ? [Float](repeating: 0, count: out.count) : out)
        lock.lock()
        let collectors = Array(micCollectors.values)
        lock.unlock()
        for c in collectors { c(out) }
    }

    private func handleTapSamples(_ samples: [Float], sampleRate: Double) {
        let out = themResampler.process(samples, sampleRate: sampleRate)
        guard !out.isEmpty else { return }
        // Preuve par l'usage : de l'audio non nul implique que la permission est accordée.
        if out.contains(where: { abs($0) > 1e-6 }), systemAudioSeen.claim() {
            permissions.setSystemAudio(.granted)
            Log.info("audio système reçu : permission accordée")
        }
        lock.lock()
        let isPaused = paused
        lock.unlock()
        themPipeline.feed(isPaused ? [Float](repeating: 0, count: out.count) : out)
    }

    /// Enregistre `seconds` secondes du micro déjà ouvert (test du mot d'appel en réunion).
    func recordMic(seconds: Double) async -> [Float] {
        let id = UUID()
        let buffer = SampleBuffer()
        lock.lock()
        micCollectors[id] = { buffer.append($0) }
        lock.unlock()
        try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
        lock.lock()
        micCollectors[id] = nil
        lock.unlock()
        return buffer.samples
    }

    // MARK: Micro coupé

    /// Pause : silence sur les deux canaux (WAV et horloge continus, énoncés clos proprement).
    func setPaused(_ on: Bool) {
        lock.lock()
        paused = on
        lock.unlock()
    }

    func setMicMuted(_ on: Bool) {
        lock.lock()
        micMuted = on
        lock.unlock()
    }

    // MARK: Aparté

    func setAside(_ on: Bool) {
        let now = ProcessInfo.processInfo.systemUptime - startUptime
        lock.lock()
        defer { lock.unlock() }
        if on {
            if asideIntervals.last.map({ $0.end != nil }) ?? true {
                asideIntervals.append((start: now, end: nil))
            }
        } else if let last = asideIntervals.last, last.end == nil {
            asideIntervals[asideIntervals.count - 1].end = now
        }
    }

    /// Un énoncé est privé si l'aparté a été actif à un moment quelconque pendant [t0, t1].
    private func isAside(t0: Double, t1: Double) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return asideIntervals.contains { interval in
            interval.start <= t1 && (interval.end ?? .infinity) >= t0
        }
    }

    // MARK: Émission

    private func emitSegment(_ utterance: Utterance, channel: Channel, text: String, speaker: String?) {
        let isPrivate = channel == .me && isAside(t0: utterance.t0, t1: utterance.t1)
        let ts = Timestamp.iso8601(startDate.addingTimeInterval(utterance.t1))
        output.send(.segment(SegmentPayload(ts: ts, t0: utterance.t0, t1: utterance.t1,
                                            channel: channel, text: text, isPrivate: isPrivate,
                                            speaker: speaker)))
    }

    private static let systemAudioDeniedMessage = "Accès à l'audio système refusé : autorisez Sténo dans Réglages Système › Confidentialité et sécurité › Enregistrement audio du système."

    private func reportError(_ code: ErrorCode, _ message: String, recoverable: Bool) {
        guard errorLimiter.allow(code.rawValue) else { return }
        output.send(.error(code: code, message: message, recoverable: recoverable))
    }
}

/// Tampon d'échantillons sûr entre threads.
final class SampleBuffer {
    private let lock = NSLock()
    private var storage: [Float] = []

    func append(_ samples: [Float]) {
        lock.lock()
        storage += samples
        lock.unlock()
    }

    var samples: [Float] {
        lock.lock()
        defer { lock.unlock() }
        return storage
    }
}
