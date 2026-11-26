// Correction par dictionnaire, après la transcription.
//
// Le moteur ASR ne permet pas (simplement) d'injecter du vocabulaire : on remplace donc dans
// le texte les formes approchantes par la forme canonique du vocabulaire (mot d'appel, noms
// de services…). Comparaison insensible à la casse, aux accents et aux séparateurs.
//
// Garde-fous contre les faux positifs (« script » ne doit jamais devenir « Scribe ») :
// - distance d'édition maximale selon la longueur (0 jusqu'à 4 lettres, 1 jusqu'à 8, 2 au-delà) ;
// - pour une correction approchée, la première lettre doit être la même ;
// - les entrées de moins de 4 caractères normalisés sont ignorées (« ai » ≠ « AI ») ;
// - une fenêtre de plusieurs mots ne franchit pas la ponctuation (seulement espaces et tirets).

import Foundation

struct VocabularyCorrector {
    struct Entry: Equatable {
        let canonical: String
        /// Forme normalisée sans séparateurs (ex. "msbooking").
        let key: String
        let tokenCount: Int
    }

    let entries: [Entry]
    private let maxWindow: Int

    init(vocabulary: [String]) {
        var seen = Set<String>()
        var list: [Entry] = []
        for raw in vocabulary {
            let canonical = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            let key = Self.normalize(canonical)
            guard key.count >= 4, !seen.contains(key) else { continue }
            seen.insert(key)
            let tokens = Self.tokenize(canonical).filter(\.isWord).count
            list.append(Entry(canonical: canonical, key: key, tokenCount: max(1, tokens)))
        }
        entries = list
        maxWindow = min(4, (list.map(\.tokenCount).max() ?? 1) + 1)
    }

    var isEmpty: Bool { entries.isEmpty }

    func correct(_ text: String) -> String {
        guard !entries.isEmpty, !text.isEmpty else { return text }
        let tokens = Self.tokenize(text)
        let words = tokens.indices.filter { tokens[$0].isWord }
        guard !words.isEmpty else { return text }

        var result = ""
        var cursor = text.startIndex
        var i = 0
        while i < words.count {
            var matched = false
            let longest = min(maxWindow, words.count - i)
            for n in stride(from: longest, through: 1, by: -1) {
                let window = Array(words[i..<(i + n)])
                guard Self.joinable(window, tokens: tokens, in: text) else { continue }
                let joined = window.map { Self.normalize(String(text[tokens[$0].range])) }.joined()
                guard let entry = bestMatch(for: joined, windowSize: n) else { continue }

                let start = tokens[window.first!].range.lowerBound
                let end = tokens[window.last!].range.upperBound
                result += text[cursor..<start]
                result += entry.canonical
                cursor = end
                i += n
                matched = true
                break
            }
            if !matched { i += 1 }
        }
        result += text[cursor...]
        return result
    }

    // MARK: Correspondance

    private func bestMatch(for joined: String, windowSize: Int) -> Entry? {
        guard !joined.isEmpty else { return nil }
        var best: (entry: Entry, distance: Int)?
        let a = Array(joined)
        for entry in entries {
            if joined == entry.key { return entry }
            // Approché : fenêtre de taille comparable, même initiale, longueur proche.
            guard windowSize <= entry.tokenCount + 1, joined.count >= 4 else { continue }
            let limit = Self.maxDistance(forLength: entry.key.count)
            guard limit > 0,
                  abs(joined.count - entry.key.count) <= limit,
                  joined.first == entry.key.first
            else { continue }
            let d = Self.levenshtein(a, Array(entry.key), limit: limit)
            if d <= limit, d < (best?.distance ?? Int.max) {
                best = (entry, d)
            }
        }
        return best?.entry
    }

    static func maxDistance(forLength n: Int) -> Int {
        switch n {
        case ..<5: return 0
        case 5...8: return 1
        default: return 2
        }
    }

    /// Distance de Levenshtein bornée (renvoie `limit + 1` dès que la borne est dépassée).
    static func levenshtein(_ a: [Character], _ b: [Character], limit: Int) -> Int {
        if a == b { return 0 }
        if a.isEmpty { return b.count }
        if b.isEmpty { return a.count }
        var previous = Array(0...b.count)
        var current = [Int](repeating: 0, count: b.count + 1)
        for i in 1...a.count {
            current[0] = i
            var rowMin = current[0]
            for j in 1...b.count {
                let cost = a[i - 1] == b[j - 1] ? 0 : 1
                current[j] = min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + cost)
                rowMin = min(rowMin, current[j])
            }
            if rowMin > limit { return limit + 1 }
            swap(&previous, &current)
        }
        return previous[b.count]
    }

    // MARK: Normalisation et découpage

    /// Minuscules, sans accents, lettres et chiffres seulement.
    static func normalize(_ s: String) -> String {
        let folded = s.folding(options: [.caseInsensitive, .diacriticInsensitive, .widthInsensitive],
                               locale: Locale(identifier: "fr_FR"))
        return String(folded.filter { $0.isLetter || $0.isNumber })
    }

    struct Token {
        let range: Range<String.Index>
        let isWord: Bool
    }

    static func tokenize(_ text: String) -> [Token] {
        var tokens: [Token] = []
        var start = text.startIndex
        var currentIsWord: Bool?
        var idx = text.startIndex
        while idx < text.endIndex {
            let isWord = text[idx].isLetter || text[idx].isNumber
            if let c = currentIsWord, c != isWord {
                tokens.append(Token(range: start..<idx, isWord: c))
                start = idx
            }
            currentIsWord = isWord
            idx = text.index(after: idx)
        }
        if let c = currentIsWord {
            tokens.append(Token(range: start..<text.endIndex, isWord: c))
        }
        return tokens
    }

    /// Les mots de la fenêtre ne sont séparés que par des espaces ou des tirets.
    private static func joinable(_ window: [Int], tokens: [Token], in text: String) -> Bool {
        guard window.count > 1 else { return true }
        for k in 0..<(window.count - 1) {
            let a = window[k], b = window[k + 1]
            guard b == a + 2 else { return false }
            let sep = text[tokens[a + 1].range]
            guard sep.allSatisfy({ $0.isWhitespace || $0 == "-" }) else { return false }
        }
        return true
    }
}
