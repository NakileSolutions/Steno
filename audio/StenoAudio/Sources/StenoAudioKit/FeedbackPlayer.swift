// Signal sonore discret (commande `play`) : deux notes douces d'environ 120 ms.
//
// - Moteur AVAudioEngine **séparé** du micro, créé à la demande, avec un AVAudioPlayerNode et un
//   buffer PCM synthétisé ; arrêté après quelques secondes d'inactivité (la sortie Bluetooth
//   n'est pas gardée ouverte).
// - Le son sort de notre processus vers la sortie demandée : il ne passe jamais par l'app d'appel
//   (le tap ne capte que les processus des apps de visio, ou tout sauf Sténo en mode global).
// - `play` rend la main tout de suite : la lecture se fait sur une file dédiée.

import AudioToolbox
import AVFoundation
import CoreAudio
import Foundation

/// Sons disponibles (`play.sound`).
enum FeedbackSound: String, Equatable, CaseIterable {
    /// Détection d'une commande vocale.
    case detect
}

// MARK: - Synthèse (pure)

enum FeedbackTone {
    struct Note: Equatable {
        var frequency: Double
        var duration: Double
    }

    static let sampleRate: Double = 48_000

    /// Mi5 puis La5 (quarte montante), 55 + 65 ms ≈ 120 ms.
    static let detectNotes = [Note(frequency: 659.25, duration: 0.055), Note(frequency: 880.0, duration: 0.065)]
    /// Amplitude crête ≈ −15 dBFS : audible sans couvrir la voix des participants.
    static let detectGain: Float = 0.18
    /// Fondu d'entrée et de sortie de chaque note (évite les clics).
    static let fadeSeconds = 0.008

    static func samples(for sound: FeedbackSound, sampleRate: Double = sampleRate) -> [Float] {
        switch sound {
        case .detect:
            return synthesize(notes: detectNotes, gain: detectGain, sampleRate: sampleRate)
        }
    }

    /// Notes sinusoïdales enchaînées, chacune avec un fondu en cosinus surélevé et une légère
    /// décroissance exponentielle (attaque douce, pas de note « carrée »). Premier et dernier
    /// échantillon de chaque note à zéro.
    static func synthesize(notes: [Note], gain: Float, sampleRate: Double, fade: Double = fadeSeconds) -> [Float] {
        var out: [Float] = []
        for note in notes {
            let n = max(0, Int((note.duration * sampleRate).rounded()))
            guard n > 1 else { continue }
            let fadeN = max(1, min(Int(fade * sampleRate), n / 2))
            let w = 2 * Double.pi * note.frequency / sampleRate
            for i in 0..<n {
                var env = 1.0
                if i < fadeN {
                    env = 0.5 * (1 - cos(Double.pi * Double(i) / Double(fadeN)))
                } else if i >= n - fadeN {
                    env = 0.5 * (1 - cos(Double.pi * Double(n - 1 - i) / Double(fadeN)))
                }
                let decay = exp(-6 * Double(i) / sampleRate)
                out.append(Float(sin(w * Double(i)) * env * decay) * gain)
            }
        }
        return out
    }
}

// MARK: - Lecture

@available(macOS 14.2, *)
final class FeedbackPlayer {
    /// Délai d'inactivité avant d'arrêter le moteur.
    static let idleDelay: TimeInterval = 3

    private let queue = DispatchQueue(label: "steno.audio.feedback", qos: .userInitiated)
    private var engine: AVAudioEngine?
    private var player: AVAudioPlayerNode?
    private var format: AVAudioFormat?
    /// Périphérique réglé sur le moteur courant (nil = sortie par défaut).
    private var engineDevice: AudioDeviceID?
    private var configObserver: NSObjectProtocol?
    private var idleTeardown: DispatchWorkItem?
    private var buffers: [FeedbackSound: AVAudioPCMBuffer] = [:]

    init() {}

    /// Joue `sound` sur la sortie `deviceUID` (nil = sortie par défaut). Ne bloque pas.
    func play(_ sound: FeedbackSound, deviceUID: String?) {
        queue.async { [weak self] in self?.playOnQueue(sound, deviceUID: deviceUID) }
    }

    func stop() {
        queue.async { [weak self] in self?.teardown() }
    }

    // MARK: Interne (sur `queue`)

    private func playOnQueue(_ sound: FeedbackSound, deviceUID: String?) {
        idleTeardown?.cancel()
        idleTeardown = nil

        var device: AudioDeviceID?
        if let uid = deviceUID {
            device = AudioDevices.outputDeviceID(forUID: uid)
            if device == nil { Log.warn("sortie « \(uid) » introuvable : signal joué sur la sortie par défaut") }
        }

        if engine == nil || engineDevice != device || engine?.isRunning != true {
            teardown()
            do {
                try build(device: device)
            } catch {
                teardown()
                guard device != nil else {
                    Log.warn("signal sonore impossible : \(error)")
                    return
                }
                Log.warn("signal sonore sur la sortie demandée impossible (\(error)) : sortie par défaut")
                do {
                    try build(device: nil)
                } catch {
                    teardown()
                    Log.warn("signal sonore impossible : \(error)")
                    return
                }
            }
        }

        guard let player, let buffer = buffer(for: sound) else { return }
        do {
            try ObjCException.catching {
                player.scheduleBuffer(buffer, at: nil, options: [.interrupts], completionHandler: nil)
                if !player.isPlaying { player.play() }
            }
        } catch {
            Log.warn("sound cue failed (\(error))")
            teardown()
            return
        }
        Log.debug("signal « \(sound.rawValue) » joué")

        let item = DispatchWorkItem { [weak self] in self?.teardown() }
        idleTeardown = item
        queue.asyncAfter(deadline: .now() + Self.idleDelay, execute: item)
    }

    private func build(device: AudioDeviceID?) throws {
        let engine = AVAudioEngine()
        // Le périphérique se règle avant toute connexion : le format de sortie du mixeur suit
        // alors celui du périphérique choisi. L'entrée n'est jamais touchée (pas de micro ouvert).
        // À VÉRIFIER (spike S1) : réglage pris en compte sur `outputNode` d'un moteur sans entrée.
        if let device {
            try AudioDevices.setCurrentDevice(device, on: engine.outputNode)
        }
        guard let format = AVAudioFormat(standardFormatWithSampleRate: FeedbackTone.sampleRate, channels: 1) else {
            throw CoreAudioError(status: OSStatus(kAudioHardwareUnspecifiedError), operation: "format du signal")
        }
        let player = AVAudioPlayerNode()
        // `connect` and `prepare` raise Objective-C exceptions when the output changes format
        // meanwhile (AirPods switching to hands-free mode): turned into a Swift error.
        try ObjCException.catching {
            engine.attach(player)
            // Mono → mixeur principal (centré sur les deux canaux de sortie).
            engine.connect(player, to: engine.mainMixerNode, format: format)
            engine.prepare()
            try engine.start()
        }

        self.engine = engine
        self.player = player
        self.format = format
        self.engineDevice = device
        configObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in
            // Sortie débranchée ou modifiée : le moteur sera recréé au prochain `play`.
            self?.queue.async { self?.teardown() }
        }
        let target = device.map { AudioDevices.describe($0) } ?? "sortie par défaut"
        Log.debug("moteur du signal sonore démarré (\(target))")
    }

    private func buffer(for sound: FeedbackSound) -> AVAudioPCMBuffer? {
        if let cached = buffers[sound] { return cached }
        guard let format else { return nil }
        let samples = FeedbackTone.samples(for: sound, sampleRate: format.sampleRate)
        guard !samples.isEmpty,
              let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(samples.count)),
              let dst = buffer.floatChannelData?[0]
        else { return nil }
        buffer.frameLength = AVAudioFrameCount(samples.count)
        samples.withUnsafeBufferPointer { src in
            if let base = src.baseAddress { dst.update(from: base, count: samples.count) }
        }
        buffers[sound] = buffer
        return buffer
    }

    private func teardown() {
        idleTeardown?.cancel()
        idleTeardown = nil
        if let configObserver { NotificationCenter.default.removeObserver(configObserver) }
        configObserver = nil
        let (player, engine) = (self.player, self.engine)
        try? ObjCException.catching {
            player?.stop()
            engine?.stop()
        }
        self.player = nil
        self.engine = nil
        engineDevice = nil
    }
}
