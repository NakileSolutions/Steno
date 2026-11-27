// Capture du son des apps de visio par un process tap Core Audio (macOS 14.2+).
//
// Étapes (cf. sample AudioCap) :
//   1. CATapDescription(stereoMixdownOfProcesses:) sur TOUT l'arbre de processus des apps
//      captées (Teams, Slack… peuvent sortir l'audio par un processus auxiliaire) ;
//   2. AudioHardwareCreateProcessTap ;
//   3. agrégat privé contenant la sortie audio par défaut (celle de l'appel) + le tap ;
//   4. IOProc temps réel : the block runs on the HAL IO thread and only copies the mono mix
//      into a lock-free ring; a regular queue drains it (see `RealtimeSampleBridge`).
// La recréation (changement de la liste des processus captés, de périphérique de sortie,
// réveil) est pilotée par CaptureSession.

import AudioToolbox
import AVFoundation
import CoreAudio
import Foundation

@available(macOS 14.2, *)
final class ProcessTap {
    enum Target: Equatable {
        /// Mixage stéréo des processus listés (arbre des apps captées).
        case processes([AudioObjectID])
        /// Plan B : tout le son du système sauf les processus listés (Sténo lui-même).
        case systemExcluding([AudioObjectID])
    }

    enum TapError: Error, CustomStringConvertible {
        case permissionDenied(OSStatus)
        case failed(String, OSStatus)

        var description: String {
            switch self {
            case let .permissionDenied(status):
                return "accès à l'audio système refusé (OSStatus \(status) « \(HAL.fourCC(status)) »)"
            case let .failed(step, status):
                return "\(step) a échoué (OSStatus \(status) « \(HAL.fourCC(status)) »)"
            }
        }
    }

    /// Échantillons mono Float32 et leur fréquence d'origine.
    typealias SampleHandler = (_ samples: [Float], _ sampleRate: Double) -> Void

    let target: Target

    private let onSamples: SampleHandler
    private var bridge: RealtimeSampleBridge?
    private var tapID = HAL.unknownObject
    private var aggregateID = HAL.unknownObject
    private var procID: AudioDeviceIOProcID?
    private var format: AVAudioFormat?
    private(set) var isRunning = false

    init(target: Target, onSamples: @escaping SampleHandler) {
        self.target = target
        self.onSamples = onSamples
    }

    deinit {
        stop()
    }

    var sampleRate: Double? { format?.sampleRate }

    func start() throws {
        guard !isRunning else { return }
        do {
            try createTap()
            try createAggregateAndRun()
            isRunning = true
            Log.info("tap audio démarré (\(describe(target)), \(format.map { "\(Int($0.sampleRate)) Hz, \($0.channelCount) canaux" } ?? "?"))")
        } catch {
            stop()
            throw error
        }
    }

    func stop() {
        if aggregateID != HAL.unknownObject {
            if let procID {
                _ = AudioDeviceStop(aggregateID, procID)
                _ = AudioDeviceDestroyIOProcID(aggregateID, procID)
            }
            _ = AudioHardwareDestroyAggregateDevice(aggregateID)
        }
        procID = nil
        // After the IOProc is gone: nothing writes to the ring any more.
        bridge?.stop()
        bridge = nil
        aggregateID = HAL.unknownObject
        if tapID != HAL.unknownObject {
            _ = AudioHardwareDestroyProcessTap(tapID)
        }
        tapID = HAL.unknownObject
        if isRunning { Log.info("tap audio arrêté") }
        isRunning = false
    }

    // MARK: Interne

    private var tapUUID = UUID()

    private func createTap() throws {
        let description: CATapDescription
        switch target {
        case let .processes(ids):
            description = CATapDescription(stereoMixdownOfProcesses: ids)
        case let .systemExcluding(ids):
            description = CATapDescription(stereoGlobalTapButExcludeProcesses: ids)
        }
        // À VÉRIFIER (spike S1) : noms Swift des propriétés de CATapDescription
        // (`uuid`, `name`, `isPrivate`, `muteBehavior`) et init `stereoGlobalTapButExcludeProcesses:`.
        tapUUID = UUID()
        description.uuid = tapUUID
        description.name = "Steno"
        description.isPrivate = true
        description.muteBehavior = .unmuted

        var tap = HAL.unknownObject
        let status = AudioHardwareCreateProcessTap(description, &tap)
        guard status == noErr, tap != HAL.unknownObject else {
            if Self.isPermissionError(status) { throw TapError.permissionDenied(status) }
            throw TapError.failed("création du tap", status)
        }
        tapID = tap

        var asbd: AudioStreamBasicDescription
        do {
            asbd = try HAL.tapFormat(tapID)
        } catch let error as CoreAudioError {
            throw TapError.failed("lecture du format du tap", error.status)
        }
        // The IOProc mixes Float32 samples down without converting them (real-time path).
        guard let fmt = AVAudioFormat(streamDescription: &asbd), fmt.commonFormat == .pcmFormatFloat32 else {
            throw TapError.failed("format du tap non pris en charge", 0)
        }
        format = fmt
    }

    private func createAggregateAndRun() throws {
        let outputUID: String
        do {
            outputUID = try HAL.deviceUID(HAL.defaultOutputDevice())
        } catch let error as CoreAudioError {
            throw TapError.failed("lecture de la sortie par défaut", error.status)
        }

        let description: [String: Any] = [
            // Nom et préfixe d'UID connus d'`AudioDeviceCatalog`, qui exclut cet agrégat de `devices`.
            kAudioAggregateDeviceNameKey: AudioDeviceCatalog.stenoTapName,
            kAudioAggregateDeviceUIDKey: "\(AudioDeviceCatalog.stenoTapUIDPrefix)\(UUID().uuidString)",
            kAudioAggregateDeviceMainSubDeviceKey: outputUID,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [
                [kAudioSubDeviceUIDKey: outputUID],
            ],
            kAudioAggregateDeviceTapListKey: [
                [
                    kAudioSubTapDriftCompensationKey: true,
                    kAudioSubTapUIDKey: tapUUID.uuidString,
                ],
            ],
        ]

        var aggregate = HAL.unknownObject
        var status = AudioHardwareCreateAggregateDevice(description as CFDictionary, &aggregate)
        guard status == noErr else {
            if Self.isPermissionError(status) { throw TapError.permissionDenied(status) }
            throw TapError.failed("création de l'agrégat", status)
        }
        aggregateID = aggregate
        alignFormatOnAggregate()

        // The aggregate's input lists the streams of its sub-device first (the default output's
        // microphone, for AirPods) and the tap's last: only the tap's buffers are mixed.
        let tapBuffers = tapBufferCount()
        logAggregateInputLayout(tapBuffers: tapBuffers)
        guard let bridge = RealtimeSampleBridge(label: "steno.audio.tap.io",
                                                firstBuffer: -Int32(tapBuffers), bufferCount: tapBuffers)
        else {
            throw TapError.failed("allocation of the sample ring", OSStatus(kAudioHardwareUnspecifiedError))
        }
        self.bridge = bridge
        // No dispatch queue: the block is real-time safe and runs directly on the IO thread.
        status = AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, nil, bridge.ioBlock)
        guard status == noErr else { throw TapError.failed("création de l'IOProc", status) }

        // Processing on the bridge's queue. The closure captures neither `self` nor the bridge.
        let tracker = SampleRateTracker(rate: format?.sampleRate ?? 48_000, label: "tap")
        let onSamples = onSamples
        bridge.start { samples, dropped in
            if dropped > 0 { Log.warn("tap: \(dropped) frames dropped (processing too slow)") }
            let rate = tracker.add(frames: samples.count)
            if !samples.isEmpty { onSamples(samples, rate) }
        }

        status = AudioDeviceStart(aggregateID, procID)
        guard status == noErr else {
            if Self.isPermissionError(status) { throw TapError.permissionDenied(status) }
            throw TapError.failed("démarrage de l'agrégat", status)
        }
    }

    /// Buffers the tap contributes to the IOProc's input: one if interleaved, one per channel
    /// otherwise.
    private func tapBufferCount() -> UInt32 {
        guard let format else { return 1 }
        return format.isInterleaved ? 1 : max(1, format.channelCount)
    }

    /// Diagnostic: input buffers of the aggregate (sub-device streams + tap).
    private func logAggregateInputLayout(tapBuffers: UInt32) {
        var addr = HAL.address(kAudioDevicePropertyStreamConfiguration, scope: kAudioObjectPropertyScopeInput)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(aggregateID, &addr, 0, nil, &size) == noErr, size > 0 else { return }
        let raw = UnsafeMutableRawPointer.allocate(byteCount: Int(size), alignment: MemoryLayout<AudioBufferList>.alignment)
        defer { raw.deallocate() }
        guard AudioObjectGetPropertyData(aggregateID, &addr, 0, nil, &size, raw) == noErr else { return }
        let list = UnsafeMutableAudioBufferListPointer(raw.assumingMemoryBound(to: AudioBufferList.self))
        let channels = list.map { String($0.mNumberChannels) }.joined(separator: "+")
        Log.info("tap: aggregate input \(list.count) buffer(s) (\(channels) channels), the last \(tapBuffers) from the tap")
    }

    /// L'agrégat est cadencé par la sortie par défaut. Le format du tap annonce souvent
    /// 48 kHz alors qu'un casque Bluetooth en appel (profil mains libres) tourne à 16 ou
    /// 24 kHz : les échantillons arrivent alors à ce rythme. Les traiter comme du 48 kHz
    /// accélère la voix ×3 (transcription absurde, VAD muette). On suit donc la fréquence
    /// nominale de l'agrégat (then `SampleRateTracker` follows the measured throughput).
    private func alignFormatOnAggregate() {
        guard let f = format,
              let nominal = try? HAL.read(aggregateID, kAudioDevicePropertyNominalSampleRate, default: Float64(0)),
              nominal > 0, abs(nominal - f.sampleRate) > 1
        else { return }
        Log.warn("tap : format annoncé \(Int(f.sampleRate)) Hz, agrégat cadencé à \(Int(nominal)) Hz : on suit l'agrégat")
        setSampleRate(nominal)
    }

    private func setSampleRate(_ rate: Double) {
        guard let f = format else { return }
        var asbd = f.streamDescription.pointee
        asbd.mSampleRate = rate
        if let updated = AVAudioFormat(streamDescription: &asbd) { format = updated }
    }

    /// À VÉRIFIER (spike S1/S4) : code(s) renvoyé(s) quand l'utilisateur ou le MDM refuse
    /// l'audio système. Hypothèse : 'nope' (illegal operation) ou '!hog' (permissions).
    static func isPermissionError(_ status: OSStatus) -> Bool {
        status == OSStatus(kAudioHardwareIllegalOperationError)
            || status == OSStatus(kAudioDevicePermissionsError)
    }

    private func describe(_ target: Target) -> String {
        switch target {
        case let .processes(ids): return "processus \(ids)"
        case let .systemExcluding(ids): return "son système hors \(ids)"
        }
    }
}
