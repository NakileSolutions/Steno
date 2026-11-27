// État des permissions macOS (micro, audio système, accessibilité).
//
// - Micro : AVCaptureDevice.authorizationStatus(for: .audio).
// - Accessibilité : AXIsProcessTrusted() (sans invite).
// - Audio système : aucune API publique pour l'interroger. TCC (private API, `SystemAudioTCC`)
//   when available, otherwise `unknown`, except:
//     * la création du tap échoue avec une erreur de permission → `denied` ;
//     * le tap a déjà livré de l'audio non silencieux → `granted` (preuve par l'usage).
//   When access is denied, macOS creates the tap without error and delivers silence:
//   `CaptureSession.start` checks TCC and reports `system_audio_denied` itself.
//
// Rappel TCC : quand Steno.app lance le sidecar, c'est l'app (processus « responsable »)
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
            accessibility: Self.accessibilityStatus(),
            calendar: CalendarReader.status()
        )
    }

    func currentSystemAudio() -> PermissionValue {
        // État réel lu auprès de TCC quand c'est possible (API privée, voir `SystemAudioTCC`).
        if let v = SystemAudioTCC.preflight(), v != .unknown { return v }
        lock.lock()
        defer { lock.unlock() }
        return systemAudio
    }

    /// Affiche l'invite système du droit demandé. Sans effet si l'utilisateur a déjà répondu :
    /// macOS ne redemande pas, il faut passer par les Réglages Système.
    func request(_ kind: PermissionKind) async {
        switch kind {
        case .microphone:
            _ = await Self.requestMicrophoneIfNeeded()
        case .accessibility:
            // Ouvre l'invite « Sténo souhaite contrôler cet ordinateur… » avec un lien vers
            // la bonne page des Réglages ; l'app apparaît alors dans la liste.
            let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
            _ = AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
        case .calendar:
            _ = await CalendarReader.request()
        case .systemAudio:
            if let granted = await SystemAudioTCC.request() {
                setSystemAudio(granted ? .granted : .denied)
            }
        }
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
    /// système au nom du processus responsable, donc Steno.app).
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

/// Autorisation « audio du système » (process taps). Aucune API publique pour la lire ou la
/// demander : on passe par TCC.framework (privé, chargé dynamiquement), comme les apps de
/// capture audio. Si les symboles manquent, on retombe sur la preuve par l'usage.
enum SystemAudioTCC {
    private static let service = "kTCCServiceAudioCapture" as CFString
    /// `TCCAccessPreflight` returns a C `int` (32 bits): typed `CInt`, not the 64-bit `Int`,
    /// whose upper half would be garbage.
    private typealias PreflightFn = @convention(c) (CFString, CFDictionary?) -> CInt
    private typealias RequestFn = @convention(c) (CFString, CFDictionary?, @escaping @convention(block) (Bool) -> Void) -> Void

    private static let handle: UnsafeMutableRawPointer? =
        dlopen("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC", RTLD_NOW)

    private static func symbol<T>(_ name: String, as _: T.Type) -> T? {
        guard let handle, let sym = dlsym(handle, name) else { return nil }
        return unsafeBitCast(sym, to: T.self)
    }

    /// `nil` si l'API est indisponible.
    static func preflight() -> PermissionValue? {
        guard let fn = symbol("TCCAccessPreflight", as: PreflightFn.self) else { return nil }
        switch fn(service, nil) {
        case 0: return .granted
        case 1: return .denied
        default: return .unknown
        }
    }

    /// Affiche l'invite ; `nil` si l'API est indisponible.
    static func request() async -> Bool? {
        guard let fn = symbol("TCCAccessRequest", as: RequestFn.self) else { return nil }
        return await withCheckedContinuation { (c: CheckedContinuation<Bool, Never>) in
            fn(service, nil) { ok in c.resume(returning: ok) }
        }
    }
}
