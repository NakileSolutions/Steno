// Filtre des fausses phrases : sur un bruit (souffle, clic, voix lointaine, silence de
// casque Bluetooth), le modèle de transcription « invente » parfois une courte phrase,
// souvent en anglais (« Thank you. », « Shall we? », « Oh. »). Une phrase est écartée si :
//   1. l'énoncé est presque silencieux (aucune voix réelle) ;
//   2. ou c'est une interjection anglaise isolée typique des hallucinations, prononcée
//      brièvement ou transcrite avec une confiance faible ;
//      real one-word answers ("Yes.", "No.", "OK.") are short by nature: they are only
//      dropped when transcribed with a low confidence, never for being short;
//   3. ou c'est une phrase très courte transcrite avec une confiance très faible.
// Les mots français courts (« Oui. », « D'accord. ») ne sont jamais visés par la règle 2.

import Foundation

enum HallucinationFilter {
    /// Niveau moyen (dBFS) en dessous duquel un énoncé ne contient pas de vraie voix.
    static let silenceDb: Double = -55
    /// Confiance sous laquelle une interjection isolée est écartée même si elle est longue.
    static let fillerConfidence: Float = 0.8
    /// Confiance sous laquelle une phrase de 1 à 3 mots est écartée.
    static let shortConfidence: Float = 0.35

    /// Interjections et formules que le modèle produit sur du bruit (forme normalisée).
    static let fillers: Set<String> = [
        "thank you", "thanks", "thank god", "thank you very much", "thanks for watching",
        "okay", "ok", "oh", "ah", "uh", "um", "hmm", "mm", "mhm", "mm hmm", "uh huh",
        "yeah", "yes", "no", "so", "right", "bye", "bye bye", "you", "shall we", "well",
        "all right", "alright", "sorry", "hello", "hi", "wow", "yep", "nope", "huh",
    ]

    /// Fillers that are also real answers in a meeting: kept when short, unless the model is
    /// unsure of them (a confident "Yes." is someone answering, not noise).
    static let answers: Set<String> = ["yes", "no", "ok", "okay", "yeah", "right"]

    /// Raison de l'écart, ou nil si la phrase est conservée.
    static func rejection(text: String, confidence: Float?, duration: Double, rmsDb: Double) -> String? {
        let norm = normalized(text)
        guard !norm.isEmpty else { return "texte vide" }
        if rmsDb < silenceDb {
            return String(format: "énoncé presque silencieux (%.0f dBFS)", rmsDb)
        }
        let words = norm.split(separator: " ").count
        let unsure = (confidence ?? 1) < fillerConfidence
        if fillers.contains(norm), unsure || (duration < 1.5 && !answers.contains(norm)) {
            return "interjection isolée typique d'une hallucination"
        }
        if words <= 3, let c = confidence, c < shortConfidence {
            return String(format: "phrase courte peu fiable (confiance %.2f)", c)
        }
        return nil
    }

    /// Minuscules, sans ponctuation ni espaces superflus (apostrophes et tirets → espace).
    static func normalized(_ text: String) -> String {
        let lowered = text.lowercased()
        var out = ""
        for ch in lowered {
            if ch.isLetter || ch.isNumber {
                out.append(ch)
            } else {
                out.append(" ")
            }
        }
        return out.split(separator: " ").joined(separator: " ")
    }

    /// Niveau moyen (RMS) d'un énoncé, en dBFS.
    static func rmsDb(_ samples: [Float]) -> Double {
        guard !samples.isEmpty else { return -120 }
        var sum: Double = 0
        for s in samples { sum += Double(s) * Double(s) }
        let rms = (sum / Double(samples.count)).squareRoot()
        return rms > 0 ? 20 * log10(rms) : -120
    }
}
