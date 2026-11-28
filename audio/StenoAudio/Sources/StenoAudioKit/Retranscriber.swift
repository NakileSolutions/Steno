// Retranscription de fin de réunion (commande `retranscribe`).
//
// Le direct utilise Parakeet v3, rapide mais qui choisit seul la langue : en réunion
// française, il bascule souvent en anglais au milieu d'une phrase. À la fin, chaque phrase
// est retranscrite à partir des fichiers conservés (`me.wav`, `them.wav`) avec un modèle à
// langue imposée (Canary 1B v2). Les phrases gardent leur identifiant, leurs bornes et leur
// intervenant : seul le texte change, et seulement si le nouveau texte est exploitable.
//
// Each sentence is read straight from the WAV at its bounds: an hour-long meeting would
// otherwise hold `me.wav` and `them.wav` in memory as `[Float]` (~460 MB) next to Canary.

import AVFoundation
import Foundation

struct RetranscribeItem: Codable, Equatable {
    var id: Int64
    var file: String
    var t0: Double
    var t1: Double
}

struct RetranscribedItem: Codable, Equatable {
    var id: Int64
    var text: String
}

enum Retranscriber {
    /// Marge autour des bornes de la phrase (la VAD coupe parfois un peu tôt).
    static let margin = 0.15
    /// En dessous, la phrase est trop courte pour un modèle à fenêtre de 15 s : on garde le direct.
    static let minimumDuration = 0.4

    /// Bounds (in samples) of a sentence's excerpt, margin included, in a recording of
    /// `length` samples, or `nil` if outside the file or too short.
    static func bounds(t0: Double, t1: Double, length: Int, rate: Double = Double(AudioFormat.sampleRateInt)) -> Range<Int>? {
        guard t1 - t0 >= minimumDuration, t0.isFinite, t1.isFinite else { return nil }
        let a = max(0, Int(((t0 - margin) * rate).rounded(.down)))
        let b = min(length, Int(((t1 + margin) * rate).rounded(.up)))
        guard b - a >= Int(minimumDuration * rate) else { return nil }
        return a..<b
    }

    /// Extrait d'un enregistrement (16 kHz), ou `nil` si hors du fichier ou trop court.
    static func slice(_ samples: [Float], t0: Double, t1: Double, rate: Double = Double(AudioFormat.sampleRateInt)) -> [Float]? {
        guard let range = bounds(t0: t0, t1: t1, length: samples.count, rate: rate) else { return nil }
        return Array(samples[range])
    }

    /// Le nouveau texte remplace-t-il le direct ? Non s'il est vide, sans lettre, part en
    /// boucle (« 1.1.1.1… », un mot répété) ou contient bien plus de texte que l'audio n'en
    /// permet (un débit de parole dépasse rarement 25 caractères par seconde).
    static func usable(_ text: String, duration: Double = 0) -> Bool {
        guard text.contains(where: { $0.isLetter }) else { return false }
        if text.range(of: #"(.{1,12}?)\1{6,}"#, options: .regularExpression) != nil { return false }
        let words = text.lowercased().split { !$0.isLetter && !$0.isNumber }
        if words.count >= 12 && Double(Set(words).count) / Double(words.count) < 0.3 { return false }
        if duration > 0 && Double(text.count) / duration > 40 { return false }
        return true
    }

    static func run(items: [RetranscribeItem],
                    audioDir: URL,
                    recognizer: FinalRecognizer,
                    corrector: VocabularyCorrector,
                    open: (URL) throws -> AudioClipSource = { try WavClipReader($0) }) async -> [RetranscribedItem] {
        // One open file per recording; only the current sentence's samples are in memory.
        var sources: [String: AudioClipSource?] = [:]
        var out: [RetranscribedItem] = []
        let started = Date()
        var kept = 0
        for item in items {
            if sources[item.file] == nil {
                let url = audioDir.appendingPathComponent(item.file)
                do {
                    sources[item.file] = .some(try open(url))
                } catch {
                    Log.warn("retranscription : \(item.file) illisible (\(error))")
                    sources[item.file] = .some(nil)
                }
            }
            guard let source = sources[item.file] ?? nil,
                  let range = bounds(t0: item.t0, t1: item.t1, length: source.length) else {
                kept += 1
                continue
            }
            do {
                let chunk = try source.read(range)
                let raw = try await recognizer.transcribe(chunk).trimmingCharacters(in: .whitespacesAndNewlines)
                guard usable(raw, duration: item.t1 - item.t0) else {
                    Log.info("retranscription : phrase \(item.id) gardée (texte douteux « \(raw.prefix(60)) »)")
                    kept += 1
                    continue
                }
                out.append(RetranscribedItem(id: item.id, text: corrector.correct(raw)))
            } catch {
                Log.warn("retranscription de la phrase \(item.id) : \(error)")
                kept += 1
            }
        }
        Log.info("retranscription (\(recognizer.name)) : \(out.count) phrases réécrites, \(kept) gardées, \(String(format: "%.1f", Date().timeIntervalSince(started))) s")
        return out
    }
}

/// A recording read by excerpt (16 kHz mono samples).
protocol AudioClipSource {
    /// Length in 16 kHz samples.
    var length: Int { get }
    /// The samples in `range` (counted in 16 kHz samples), mono Float32.
    func read(_ range: Range<Int>) throws -> [Float]
}

/// Samples already in memory (tests).
struct InMemoryClipSource: AudioClipSource {
    let samples: [Float]
    var length: Int { samples.count }
    func read(_ range: Range<Int>) throws -> [Float] { Array(samples[range]) }
}

/// Reads excerpts of an audio file (the sidecar's 16 kHz mono WAV, or any other format
/// AVFoundation reads) without loading the whole file.
final class WavClipReader: AudioClipSource {
    private let file: AVAudioFile
    private let fileRate: Double
    private static let rate = Double(AudioFormat.sampleRateInt)

    init(_ url: URL) throws {
        file = try AVAudioFile(forReading: url)
        fileRate = file.processingFormat.sampleRate
    }

    var length: Int { Int((Double(file.length) * Self.rate / fileRate).rounded(.down)) }

    func read(_ range: Range<Int>) throws -> [Float] {
        let start = AVAudioFramePosition((Double(range.lowerBound) * fileRate / Self.rate).rounded(.down))
        let end = min(file.length, AVAudioFramePosition((Double(range.upperBound) * fileRate / Self.rate).rounded(.up)))
        guard end > start,
              let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(end - start)) else {
            return []
        }
        file.framePosition = start
        try file.read(into: buffer, frameCount: AVAudioFrameCount(end - start))
        let mono = AudioBufferUtils.monoSamples(from: buffer, mix: .average)
        return Resampler.resampleAll(mono, from: fileRate)
    }
}
