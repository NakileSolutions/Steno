// Sortie stdout sérialisée (une ligne JSON par message) et journaux sur stderr.
//
// stdout est réservé au protocole : rien d'autre ne doit y être écrit en mode protocole.

import Foundation

final class Output {
    private let queue = DispatchQueue(label: "steno.audio.stdout", qos: .userInitiated)
    private let writer: (Data) -> Bool
    private var broken = false

    /// Called once (from the output queue) when stdout breaks. Set before the first `send`. The
    /// protocol mode shuts down cleanly from it (WAV headers patched) instead of exiting at once.
    /// Without it, the process exits.
    var onBroken: (() -> Void)?

    /// Écrit sur le descripteur `fd` (stdout par défaut).
    convenience init(fileDescriptor fd: Int32 = STDOUT_FILENO) {
        self.init(writer: { data in Output.writeAll(data, to: fd) })
    }

    /// `writer` renvoie `false` si la sortie est cassée (cœur disparu).
    init(writer: @escaping (Data) -> Bool) {
        self.writer = writer
    }

    /// Envoie un message (asynchrone, ordre préservé, ne bloque jamais l'appelant).
    func send(_ message: OutgoingMessage) {
        queue.async { [self] in
            guard !broken else { return }
            do {
                let line = try Output.encodeLine(message)
                if !writer(line) {
                    broken = true
                    Log.warn("stdout fermé : le cœur ne lit plus, arrêt du sidecar")
                    guard let onBroken else { exit(0) }
                    onBroken()
                }
            } catch {
                Log.error("encodage JSON impossible : \(error)")
            }
        }
    }

    /// Attend que tous les messages en file soient écrits.
    func drain() {
        queue.sync {}
    }

    static func makeEncoder() -> JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        return encoder
    }

    /// Encode un message en une ligne JSON terminée par "\n".
    static func encodeLine(_ message: OutgoingMessage) throws -> Data {
        var data = try makeEncoder().encode(message)
        data.append(0x0A)
        return data
    }

    /// `write(2)` en boucle : pas de tampon, donc chaque ligne est « flushée » immédiatement.
    static func writeAll(_ data: Data, to fd: Int32) -> Bool {
        data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) -> Bool in
            guard let base = raw.baseAddress else { return true }
            var offset = 0
            while offset < raw.count {
                let n = Darwin.write(fd, base + offset, raw.count - offset)
                if n < 0 {
                    if errno == EINTR { continue }
                    return false // EPIPE (SIGPIPE ignoré) ou autre erreur fatale
                }
                offset += n
            }
            return true
        }
    }
}

// MARK: - Journaux (stderr uniquement)

enum Log {
    static let debugEnabled = ProcessInfo.processInfo.environment["STENO_AUDIO_DEBUG"] != nil
    private static let lock = NSLock()

    static func debug(_ message: @autoclosure () -> String) {
        if debugEnabled { write("DEBUG", message()) }
    }

    static func info(_ message: @autoclosure () -> String) { write("INFO", message()) }
    static func warn(_ message: @autoclosure () -> String) { write("WARN", message()) }
    static func error(_ message: @autoclosure () -> String) { write("ERROR", message()) }

    private static func write(_ level: String, _ message: String) {
        let line = "[steno-audio] \(level) \(message)\n"
        lock.lock()
        defer { lock.unlock() }
        fputs(line, stderr)
    }
}

// MARK: - Petits utilitaires de concurrence

/// Drapeau « une seule fois », sûr entre threads.
final class OnceFlag {
    private let lock = NSLock()
    private var done = false

    /// Renvoie `true` au premier appel seulement.
    func claim() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if done { return false }
        done = true
        return true
    }
}

/// Exécute `operation` et rend la main au plus tard après `seconds`.
/// Renvoie `true` si l'opération s'est terminée à temps (elle continue sinon en arrière-plan).
@discardableResult
func runWithTimeout(seconds: Double, _ operation: @escaping () async -> Void) async -> Bool {
    let work = Task { await operation() }
    return await withCheckedContinuation { (continuation: CheckedContinuation<Bool, Never>) in
        let once = OnceFlag()
        Task {
            await work.value
            if once.claim() { continuation.resume(returning: true) }
        }
        Task {
            try? await Task.sleep(nanoseconds: UInt64(max(0, seconds) * 1_000_000_000))
            if once.claim() { continuation.resume(returning: false) }
        }
    }
}

/// Limiteur simple : n'autorise une action qu'une fois par intervalle (erreurs répétées).
final class RateLimiter {
    private let interval: TimeInterval
    private let lock = NSLock()
    private var last: [String: TimeInterval] = [:]

    init(interval: TimeInterval) { self.interval = interval }

    func allow(_ key: String) -> Bool {
        let now = ProcessInfo.processInfo.systemUptime
        lock.lock()
        defer { lock.unlock() }
        if let t = last[key], now - t < interval { return false }
        last[key] = now
        return true
    }
}
