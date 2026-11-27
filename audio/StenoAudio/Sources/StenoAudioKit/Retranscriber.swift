// Retranscription de fin de réunion (commande `retranscribe`).
//
// Le direct utilise Parakeet v3, rapide mais qui choisit seul la langue : en réunion
// française, il bascule souvent en anglais au milieu d'une phrase. À la fin, chaque phrase
// est retranscrite à partir des fichiers conservés (`me.wav`, `them.wav`) avec un modèle à
// langue imposée (Canary 1B v2). Les phrases gardent leur identifiant, leurs bornes et leur
// intervenant : seul le texte change, et seulement si le nouveau texte est exploitable.

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

    /// Extrait d'un enregistrement (16 kHz), ou `nil` si hors du fichier ou trop court.
    static func slice(_ samples: [Float], t0: Double, t1: Double, rate: Double = Double(AudioFormat.sampleRateInt)) -> [Float]? {
        guard t1 - t0 >= minimumDuration, t0.isFinite, t1.isFinite else { return nil }
        let a = max(0, Int(((t0 - margin) * rate).rounded(.down)))
        let b = min(samples.count, Int(((t1 + margin) * rate).rounded(.up)))
        guard b - a >= Int(minimumDuration * rate) else { return nil }
        return Array(samples[a..<b])
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
                    load: (URL) throws -> [Float]) async -> [RetranscribedItem] {
        var cache: [String: [Float]] = [:]
        var out: [RetranscribedItem] = []
        let started = Date()
        var kept = 0
        for item in items {
            if cache[item.file] == nil {
                let url = audioDir.appendingPathComponent(item.file)
                do {
                    cache[item.file] = try load(url)
                } catch {
                    Log.warn("retranscription : \(item.file) illisible (\(error))")
                    cache[item.file] = []
                }
            }
            guard let samples = cache[item.file], let chunk = slice(samples, t0: item.t0, t1: item.t1) else {
                kept += 1
                continue
            }
            do {
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
