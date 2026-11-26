// Détection d'activité vocale (VAD) et découpage en énoncés.
//
// - `VoiceActivityDetector` : renvoie une probabilité de parole par trame. Deux
//   implémentations : Silero via FluidAudio (FluidAudioBackend.swift) et un repli par
//   énergie (`EnergyVAD`, ci-dessous, sans modèle).
// - `UtteranceSegmenter` : machine à états pure (testable) qui ferme un énoncé après
//   `silence_ms` de silence ou au bout de 25 s. Un segmenteur indépendant par canal.

import Foundation

// MARK: - VAD

protocol VoiceActivityDetector: AnyObject {
    /// Nom pour les journaux ("silero", "energy").
    var name: String { get }
    /// Taille de trame attendue, en échantillons à 16 kHz.
    var frameSize: Int { get }
    /// Probabilité de parole dans [0, 1] pour une trame de `frameSize` échantillons.
    /// Implémentation à état : une instance par canal.
    func speechProbability(_ frame: [Float]) async throws -> Float
}

/// VAD de repli par énergie, avec plancher de bruit adaptatif. Renvoie 0 ou 1.
final class EnergyVAD: VoiceActivityDetector {
    let name = "energy"
    let frameSize: Int

    /// Marge au-dessus du plancher de bruit pour déclarer de la parole.
    var marginDb: Double = 12
    /// Seuil absolu minimal (en dessous, jamais de parole).
    var absoluteMinDb: Double = -50

    private(set) var noiseFloorDb: Double = -60

    /// 512 échantillons = 32 ms à 16 kHz.
    init(frameSize: Int = 512) {
        self.frameSize = frameSize
    }

    func probability(for frame: [Float]) -> Float {
        let db = AudioLevel.dbfs(frame)
        let threshold = max(absoluteMinDb, noiseFloorDb + marginDb)
        let speech = db > threshold

        // Plancher : descend vite, remonte lentement (presque pas pendant la parole).
        if db < noiseFloorDb {
            noiseFloorDb += 0.2 * (db - noiseFloorDb)
        } else {
            noiseFloorDb += (speech ? 0.0005 : 0.02) * (db - noiseFloorDb)
        }
        noiseFloorDb = min(max(noiseFloorDb, -90), -20)
        return speech ? 1 : 0
    }

    func speechProbability(_ frame: [Float]) async throws -> Float {
        probability(for: frame)
    }
}

// MARK: - Énoncés

struct Utterance: Equatable {
    var samples: [Float]
    /// Position du premier échantillon (échantillons depuis `start`).
    var startSample: Int
    /// Position juste après le dernier échantillon.
    var endSample: Int
    var sampleRate: Int

    var t0: Double { Double(startSample) / Double(sampleRate) }
    var t1: Double { Double(endSample) / Double(sampleRate) }
    var duration: Double { t1 - t0 }
}

struct SegmenterConfig: Equatable {
    var sampleRate: Int = AudioFormat.sampleRateInt
    /// Silence qui clôt un énoncé.
    var silenceMs: Int = 1000
    /// Durée maximale d'un énoncé.
    var maxUtteranceSeconds: Double = 25
    /// En dessous, l'énoncé est jeté (clics, toux).
    var minSpeechMs: Int = 200
    /// Audio conservé avant le début détecté (attaque des consonnes).
    var preRollMs: Int = 200
    /// Silence conservé après la fin.
    var postRollMs: Int = 150
    /// Hystérésis sur la probabilité de parole.
    var startThreshold: Float = 0.5
    var continueThreshold: Float = 0.35
    /// Fenêtre (fin de tampon) où chercher le point de coupe le plus calme à 25 s.
    var cutSearchSeconds: Double = 2
}

/// Machine à états de découpage. Valeur pure : aucune dépendance au temps réel.
struct UtteranceSegmenter {
    let config: SegmenterConfig

    /// Nombre total d'échantillons consommés (horloge de la session, en échantillons).
    private(set) var position = 0

    private var active = false
    private var buffer: [Float] = []
    private var bufferStart = 0
    private var speechSamples = 0
    private var trailingSilence = 0
    private var preRoll: [Float] = []

    init(config: SegmenterConfig = SegmenterConfig()) {
        self.config = config
    }

    var isInUtterance: Bool { active }

    private var silenceSamples: Int { max(1, config.sampleRate * config.silenceMs / 1000) }
    private var minSpeechSamples: Int { config.sampleRate * config.minSpeechMs / 1000 }
    private var preRollSamples: Int { config.sampleRate * config.preRollMs / 1000 }
    private var postRollSamples: Int { config.sampleRate * config.postRollMs / 1000 }
    private var maxSamples: Int { Int(config.maxUtteranceSeconds * Double(config.sampleRate)) }

    /// Pousse une trame et sa probabilité de parole ; renvoie les énoncés terminés.
    mutating func push(_ frame: [Float], probability: Float) -> [Utterance] {
        guard !frame.isEmpty else { return [] }
        let frameStart = position
        position += frame.count

        if !active {
            if probability >= config.startThreshold {
                active = true
                bufferStart = frameStart - preRoll.count
                buffer = preRoll
                buffer.append(contentsOf: frame)
                preRoll.removeAll(keepingCapacity: true)
                speechSamples = frame.count
                trailingSilence = 0
            } else {
                preRoll.append(contentsOf: frame)
                if preRoll.count > preRollSamples {
                    preRoll.removeFirst(preRoll.count - preRollSamples)
                }
            }
            return []
        }

        buffer.append(contentsOf: frame)
        if probability >= config.continueThreshold {
            speechSamples += frame.count
            trailingSilence = 0
        } else {
            trailingSilence += frame.count
        }

        if trailingSilence >= silenceSamples {
            return closeUtterance().map { [$0] } ?? []
        }
        if buffer.count >= maxSamples {
            return [cutLongUtterance()]
        }
        return []
    }

    /// Clôt l'énoncé en cours (fin de capture).
    mutating func flush() -> Utterance? {
        guard active else {
            preRoll.removeAll(keepingCapacity: true)
            return nil
        }
        return closeUtterance()
    }

    /// Trou dans le flux (tap recréé, veille…) : clôt l'énoncé et avance l'horloge.
    mutating func skip(samples: Int) -> Utterance? {
        let u = flush()
        preRoll.removeAll(keepingCapacity: true)
        position += max(0, samples)
        return u
    }

    // MARK: Interne

    private mutating func closeUtterance() -> Utterance? {
        let drop = max(0, trailingSilence - postRollSamples)
        let keep = buffer.count - drop
        var result: Utterance?
        if speechSamples >= minSpeechSamples, keep > 0 {
            result = Utterance(samples: Array(buffer.prefix(keep)),
                               startSample: bufferStart,
                               endSample: bufferStart + keep,
                               sampleRate: config.sampleRate)
        }
        // La fin du tampon (silence) sert de pré-roll au prochain énoncé.
        preRoll = Array(buffer.suffix(preRollSamples))
        active = false
        buffer.removeAll(keepingCapacity: true)
        speechSamples = 0
        trailingSilence = 0
        return result
    }

    /// Coupe un énoncé trop long au point le plus calme des dernières secondes ;
    /// la suite reste dans le tampon (l'énoncé continue).
    private mutating func cutLongUtterance() -> Utterance {
        let cut = quietestCutPoint()
        let head = Array(buffer.prefix(cut))
        let rest = Array(buffer.suffix(from: cut))
        let u = Utterance(samples: head, startSample: bufferStart,
                          endSample: bufferStart + cut, sampleRate: config.sampleRate)
        bufferStart += cut
        buffer = rest
        speechSamples = rest.count
        trailingSilence = min(trailingSilence, rest.count)
        return u
    }

    private func quietestCutPoint() -> Int {
        let window = max(1, config.sampleRate / 50) // 20 ms
        let searchStart = max(window, buffer.count - Int(config.cutSearchSeconds * Double(config.sampleRate)))
        var best = buffer.count
        var bestEnergy = Float.greatestFiniteMagnitude
        var i = searchStart
        while i + window <= buffer.count {
            var e: Float = 0
            for j in i..<(i + window) { e += buffer[j] * buffer[j] }
            if e < bestEnergy {
                bestEnergy = e
                best = i + window / 2
            }
            i += window
        }
        return min(max(best, 1), buffer.count)
    }
}
