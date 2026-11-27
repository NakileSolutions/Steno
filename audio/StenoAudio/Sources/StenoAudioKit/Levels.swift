// Niveaux RMS en dBFS par canal, pour le vumètre de la vue « en direct ».
// Le sidecar émet `{"type":"level","me":…,"them":…}` au plus 4 fois par seconde.

import Accelerate
import Foundation

enum AudioLevel {
    /// Plancher utilisé quand il n'y a pas de signal (ou pas d'échantillons).
    static let floorDb: Double = -100

    static func rms(_ samples: [Float]) -> Float {
        guard !samples.isEmpty else { return 0 }
        let sum = vDSP.sumOfSquares(samples)
        return (sum / Float(samples.count)).squareRoot()
    }

    static func dbfs(rms: Double) -> Double {
        guard rms.isFinite, rms > 0 else { return floorDb }
        return max(floorDb, min(0, 20 * log10(rms)))
    }

    static func dbfs(_ samples: [Float]) -> Double {
        dbfs(rms: Double(rms(samples)))
    }
}

/// Accumule l'énergie par canal entre deux émissions. Sûr entre threads.
final class LevelMeter {
    private let lock = NSLock()
    private var sumSquares: [Channel: Double] = [:]
    private var counts: [Channel: Int] = [:]

    func add(_ samples: [Float], channel: Channel) {
        guard !samples.isEmpty else { return }
        let sum = Double(vDSP.sumOfSquares(samples))
        lock.lock()
        sumSquares[channel, default: 0] += sum
        counts[channel, default: 0] += samples.count
        lock.unlock()
    }

    /// Renvoie les niveaux depuis le dernier appel et remet les compteurs à zéro.
    func take() -> (me: Double, them: Double) {
        lock.lock()
        defer {
            sumSquares.removeAll(keepingCapacity: true)
            counts.removeAll(keepingCapacity: true)
            lock.unlock()
        }
        func level(_ channel: Channel) -> Double {
            guard let n = counts[channel], n > 0, let s = sumSquares[channel] else {
                return AudioLevel.floorDb
            }
            return AudioLevel.dbfs(rms: (s / Double(n)).squareRoot())
        }
        return (level(.me), level(.them))
    }
}

/// Minuterie qui publie les niveaux toutes les 250 ms (donc au plus 4×/s).
final class LevelEmitter {
    static let interval: TimeInterval = 0.25

    private let timer: DispatchSourceTimer

    init(meter: LevelMeter, output: Output) {
        timer = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "steno.audio.levels", qos: .utility))
        timer.schedule(deadline: .now() + Self.interval, repeating: Self.interval, leeway: .milliseconds(20))
        timer.setEventHandler { [meter, output] in
            let levels = meter.take()
            output.send(.level(me: levels.me, them: levels.them))
        }
    }

    private var started = false

    func start() {
        guard !started else { return }
        started = true
        timer.resume()
    }

    func stop() {
        // Une source GCD suspendue ne doit pas être libérée : on la relance avant d'annuler.
        if !started {
            started = true
            timer.resume()
        }
        timer.cancel()
    }
}
