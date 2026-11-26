// État des permissions macOS (micro, audio système, accessibilité).
//
// - Micro : AVCaptureDevice.authorizationStatus(for: .audio).
// - Accessibilité : AXIsProcessTrusted() (sans invite).
// - Audio système : aucune API publique pour l'interroger. On reste à `unknown`, sauf :
//     * la création du tap échoue avec une erreur de permission → `denied` ;
//     * le tap a déjà livré de l'audio non silencieux → `granted` (preuve par l'usage).
//   À VÉRIFIER (spike S1/S4) : quand la permission est refusée, macOS peut créer le tap
//   sans erreur et livrer du silence ; dans ce cas l'état reste `unknown`.
//
// Rappel TCC : quand Scribe.app lance le sidecar, c'est l'app (processus « responsable »)
// qui porte les permissions. Voir README.

import ApplicationServices
import AVFoundation
import Foundation

final class PermissionsTracker {
    private let lock = NSLock()
    private var systemAudio: PermissionValue = .unknown

    func snapshot() -> PermissionsSnapshot {
        PermissionsSnapshot(
            microphone: Self.microphoneStatus(),
            systemAudio: currentSystemAudio(),
            accessibility: Self.accessibilityStatus()
        )
    }

    func currentSystemAudio() -> PermissionValue {
        lock.lock()
        defer { lock.unlock() }
        return systemAudio
    }

    /// Met à jour l'état de l'audio système ; renvoie `true` s'il a changé.
    @discardableResult
    func setSystemAudio(_ value: PermissionValue) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard systemAudio != value else { return false }
        systemAudio = value
        return true
    }

    static func microphoneStatus() -> PermissionValue {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            return .granted
        case .denied, .restricted:
            return .denied
        case .notDetermined:
            return .unknown
        @unknown default:
            return .unknown
        }
    }

    static func accessibilityStatus() -> PermissionValue {
        AXIsProcessTrusted() ? .granted : .denied
    }

    /// Demande l'accès au micro si l'utilisateur n'a pas encore répondu (affiche l'invite
    /// système au nom du processus responsable, donc Scribe.app).
    static func requestMicrophoneIfNeeded() async -> PermissionValue {
        let current = microphoneStatus()
        guard current == .unknown else { return current }
        let granted = await withCheckedContinuation { (continuation: CheckedContinuation<Bool, Never>) in
            AVCaptureDevice.requestAccess(for: .audio) { ok in
                continuation.resume(returning: ok)
            }
        }
        return granted ? .granted : .denied
    }
}
