// Capture du son de Teams par un process tap Core Audio (macOS 14.2+).
//
// Étapes (cf. sample AudioCap) :
//   1. CATapDescription(stereoMixdownOfProcesses:) sur TOUT l'arbre de processus Teams
//      (le nouveau Teams peut sortir l'audio par un processus auxiliaire) ;
//   2. AudioHardwareCreateProcessTap ;
//   3. agrégat privé contenant la sortie système par défaut + le tap ;
//   4. IOProc par bloc sur une file dédiée (pas le thread temps réel : on peut allouer).
// La recréation (changement de la liste des processus Teams, de périphérique de sortie,
// réveil) est pilotée par CaptureSession.

import AudioToolbox
import AVFoundation
import CoreAudio
import Foundation

@available(macOS 14.2, *)
final class ProcessTap {
    enum Target: Equatable {
        /// Mixage stéréo des processus listés (arbre Teams).
        case processes([AudioObjectID])
        /// Plan B : tout le son du système sauf les processus listés (Scribe lui-même).
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
    private let ioQueue = DispatchQueue(label: "scribe.audio.tap.io", qos: .userInteractive)
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
        description.name = "Scribe"
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
        guard let fmt = AVAudioFormat(streamDescription: &asbd) else {
            throw TapError.failed("format du tap non pris en charge", 0)
        }
        format = fmt
    }

    private func createAggregateAndRun() throws {
        let outputUID: String
        do {
            outputUID = try HAL.deviceUID(HAL.defaultSystemOutputDevice())
        } catch let error as CoreAudioError {
            throw TapError.failed("lecture de la sortie par défaut", error.status)
        }

        let description: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Scribe-Tap",
            kAudioAggregateDeviceUIDKey: "scribe-tap-\(UUID().uuidString)",
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

        status = AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, ioQueue) {
            [weak self] _, inputData, _, _, _ in
            self?.handle(inputData)
        }
        guard status == noErr else { throw TapError.failed("création de l'IOProc", status) }

        status = AudioDeviceStart(aggregateID, procID)
        guard status == noErr else {
            if Self.isPermissionError(status) { throw TapError.permissionDenied(status) }
            throw TapError.failed("démarrage de l'agrégat", status)
        }
    }

    private func handle(_ inputData: UnsafePointer<AudioBufferList>) {
        guard let format,
              let buffer = AVAudioPCMBuffer(pcmFormat: format, bufferListNoCopy: inputData, deallocator: nil)
        else { return }
        let mono = AudioBufferUtils.monoSamples(from: buffer, mix: .average)
        if !mono.isEmpty { onSamples(mono, format.sampleRate) }
    }

    /// À VÉRIFIER (spike S1/S4) : code(s) renvoyé(s) quand l'utilisateur ou le MDM refuse
    /// l'audio système. Hypothèse : 'nope' (illegal operation) ou '!hog' (permissions).
    static func isPermissionError(_ status: OSStatus) -> Bool {
        status == OSStatus(kAudioHardwareIllegalOperationError)
            || status == OSStatus(kAudioDevicePermissionsError)
    }

    private func describe(_ target: Target) -> String {
        switch target {
        case let .processes(ids): return "processus Teams \(ids)"
        case let .systemExcluding(ids): return "son système hors \(ids)"
        }
    }
}
