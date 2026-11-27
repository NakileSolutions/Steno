// Heuristiques pures (testées, sans accessibilité) : parmi les titres des fenêtres d'une app
// de visio, lequel désigne l'appel en cours ?
//
// - Teams : fenêtre de réunion séparée, titre nettoyé de « | Microsoft Teams » (inchangé) ;
// - Slack : fenêtre du huddle (« Huddle… », « Appel… ») avec le nom du canal ;
// - Zoom : fenêtre « Zoom Meeting » / « Réunion Zoom » (sujet si le titre en contient un) ;
// - navigateurs : onglet actif « Meet – … » (Google Meet) ;
// - autres apps : titre de la fenêtre au premier plan, sauf titre générique (nom de l'app, vide).
//
// `nil` plutôt qu'un titre faux : le contrat autorise `"title": null`.
// La lecture des fenêtres elles-mêmes est dans `CallWindowTitle` (CallMonitor.swift).

import Foundation

struct WindowCandidate: Equatable {
    var title: String
    /// Fenêtre principale de l'app (AXMain).
    var isMain: Bool
    /// Fenêtre au premier plan de l'app (AXFocusedWindow).
    var isFocused: Bool = false
}

/// Famille d'app, qui choisit l'heuristique de titre.
enum CallAppKind: Equatable {
    case teams, slack, zoom, browser, other
}

enum CallTitleHeuristics {
    /// Préfixes de bundle des navigateurs courants (minuscules).
    static let browserBundlePrefixes = [
        "com.google.chrome", "com.microsoft.edgemac", "company.thebrowser.browser", "com.brave.browser",
        "org.mozilla.firefox", "com.apple.safari", "com.operasoftware.opera", "com.vivaldi.vivaldi",
        "org.chromium.chromium",
    ]

    /// Identifiants d'app traités comme « Meet dans un navigateur ».
    static let browserAppIDs: Set<String> = ["meet", "google-meet", "googlemeet", "browser", "navigateur", "navigateurs"]

    static func kind(of app: WatchedApp) -> CallAppKind {
        let id = TitleText.fold(app.id)
        let prefixes = app.bundlePrefixes.map { $0.lowercased() }
        func hasPrefix(_ p: String) -> Bool { prefixes.contains { $0.hasPrefix(p) } }
        if id == "teams" || hasPrefix("com.microsoft.teams") { return .teams }
        if id == "slack" || hasPrefix("com.tinyspeck.slackmacgap") { return .slack }
        if id == "zoom" || hasPrefix("us.zoom.") { return .zoom }
        if browserAppIDs.contains(id)
            || prefixes.contains(where: { p in browserBundlePrefixes.contains { p.hasPrefix($0) } })
        {
            return .browser
        }
        return .other
    }

    /// Titre de l'appel pour `app` d'après les fenêtres de ses processus, ou `nil`.
    static func pickTitle(for app: WatchedApp, windows: [WindowCandidate]) -> String? {
        switch kind(of: app) {
        case .teams: return TeamsTitleHeuristics.pickMeetingTitle(windows)
        case .slack: return slackHuddleTitle(windows)
        case .zoom: return zoomMeetingTitle(windows)
        case .browser: return meetTabTitle(windows)
        case .other: return foregroundTitle(windows, appNames: [app.name, app.id])
        }
    }

    // MARK: Slack

    /// Mots (entiers) qui signalent la fenêtre d'un huddle ou d'un appel Slack.
    static let slackCallHints = ["huddle", "huddles", "appel", "call", "meeting", "reunion"]

    /// « Huddle : #equipe-flotte - Acme - Slack » → « Huddle : #equipe-flotte ».
    /// Sans fenêtre de huddle (huddle réduit dans la barre latérale) : `nil`, car le canal
    /// affiché dans la fenêtre principale n'est pas forcément celui de l'appel.
    static func slackHuddleTitle(_ windows: [WindowCandidate]) -> String? {
        var best: (title: String, score: Int)?
        for window in windows {
            let parts = TitleText.parts(TitleText.stripDecorations(window.title))
                .filter { TitleText.fold($0) != "slack" }
            guard let index = parts.firstIndex(where: { TitleText.containsWord($0, anyOf: slackCallHints) }) else {
                continue
            }
            var title = parts[index]
            // « Huddle - #canal - Acme - Slack » : le mot seul ne dit rien, on garde la partie suivante.
            if TitleText.words(title).count <= 1, index + 1 < parts.count {
                title += " - " + parts[index + 1]
            }
            var score = 2
            if title.contains("#") || title.contains("@") { score += 1 }
            // Le huddle agrandi s'ouvre dans une fenêtre à part, non principale.
            if !window.isMain { score += 1 }
            if score > (best?.score ?? 0) { best = (title, score) }
        }
        return best?.title
    }

    // MARK: Zoom

    /// Libellés de la fenêtre de réunion de Zoom (comparés sans casse ni accents).
    static let zoomMeetingMarkers = ["Zoom Meeting", "Réunion Zoom", "Zoom Réunion", "Zoom Webinar", "Webinaire Zoom"]

    /// « Zoom Meeting » → « Zoom Meeting » ; « Daily – Zoom Meeting » → « Daily ».
    /// La fenêtre d'accueil (« Zoom », « Zoom Workplace ») ne donne rien.
    static func zoomMeetingTitle(_ windows: [WindowCandidate]) -> String? {
        var best: (title: String, score: Int)?
        for window in windows {
            let raw = TitleText.stripDecorations(window.title)
            guard let range = zoomMeetingMarkers.lazy
                .compactMap({ raw.range(of: $0, options: [.caseInsensitive, .diacriticInsensitive]) })
                .first
            else { continue }
            let marker = String(raw[range])
            let remainder = raw.replacingCharacters(in: range, with: " | ")
            let topic = TitleText.parts(remainder).first { !isZoomGeneric($0) }
            let score = topic == nil ? 2 : 3
            if score > (best?.score ?? 0) { best = (topic ?? marker, score) }
        }
        return best?.title
    }

    private static func isZoomGeneric(_ part: String) -> Bool {
        let folded = TitleText.fold(part)
        if ["", "zoom", "zoom workplace", "zoom.us"].contains(folded) { return true }
        // « 40-Minutes » : limite de durée des comptes gratuits.
        if folded.range(of: #"^\d+\s*-?\s*min(ute)?s?$"#, options: .regularExpression) != nil { return true }
        return !folded.contains(where: \.isLetter)
    }

    // MARK: Navigateurs (Google Meet)

    /// Noms de navigateurs ajoutés en fin de titre de fenêtre (ce qui suit est le profil).
    static let browserNames: Set<String> = [
        "google chrome", "chrome", "chromium", "safari", "microsoft edge", "edge", "arc", "firefox",
        "mozilla firefox", "brave", "brave browser", "opera", "vivaldi",
    ]

    /// « Meet – Point hebdo - Google Chrome - Travail » → « Point hebdo » ;
    /// « Meet - abc-defg-hij - Google Chrome » → « Google Meet ».
    /// Seul l'onglet actif de chaque fenêtre est visible par l'accessibilité.
    static func meetTabTitle(_ windows: [WindowCandidate]) -> String? {
        var best: (title: String, score: Int)?
        for window in TitleText.foregroundFirst(windows) {
            var parts = TitleText.parts(TitleText.stripDecorations(window.title))
            if let cut = parts.firstIndex(where: { browserNames.contains(TitleText.fold($0)) }) {
                parts = Array(parts[..<cut])
            }
            var isMeet = false
            var rest: [String] = []
            for part in parts {
                let folded = TitleText.fold(part)
                if folded == "meet" || folded == "google meet" {
                    isMeet = true
                } else if folded.hasPrefix("meet:") || folded.hasPrefix("google meet:") {
                    // « Meet: Point hebdo »
                    isMeet = true
                    if let colon = part.firstIndex(of: ":") {
                        let topic = part[part.index(after: colon)...].trimmingCharacters(in: .whitespaces)
                        if !topic.isEmpty { rest.append(topic) }
                    }
                } else if !isMeetCode(folded) {
                    rest.append(part)
                }
            }
            guard isMeet else { continue }
            var score = rest.isEmpty ? 2 : 4
            if window.isFocused { score += 1 }
            if score > (best?.score ?? 0) { best = (rest.first ?? "Google Meet", score) }
        }
        return best?.title
    }

    /// Code de réunion Meet : `abc-defg-hij`.
    static func isMeetCode(_ folded: String) -> Bool {
        folded.range(of: #"^[a-z]{3}-[a-z]{4}-[a-z]{3}$"#, options: .regularExpression) != nil
    }

    // MARK: Par défaut

    /// Titre de la fenêtre au premier plan (à défaut la fenêtre principale), débarrassé du
    /// nom de l'app ; `nil` si ce qui reste est générique (nom de l'app seul, vide).
    static func foregroundTitle(_ windows: [WindowCandidate], appNames: [String]) -> String? {
        let generic = Set(appNames.map(TitleText.fold)).union(["", "untitled", "sans titre"])
        var ordered = windows.filter(\.isFocused) + windows.filter { $0.isMain && !$0.isFocused }
        // L'accessibilité n'a signalé ni fenêtre au premier plan ni fenêtre principale.
        if ordered.isEmpty { ordered = windows }
        for window in ordered {
            let parts = TitleText.parts(TitleText.stripDecorations(window.title))
                .filter { !generic.contains(TitleText.fold($0)) }
            if let first = parts.first { return first }
        }
        return nil
    }
}

// MARK: - Teams (heuristique d'origine)

/// Heuristique pure (testée) : quel titre de fenêtre Teams ressemble à une réunion ?
enum TeamsTitleHeuristics {
    /// Titres d'écrans de Teams qui ne sont pas des réunions (comparés sans casse ni accents).
    static let genericTitles: Set<String> = [
        "microsoft teams", "teams", "microsoft teams (work or school)", "microsoft teams (travail ou scolaire)",
        "chat", "conversation", "conversations", "activite", "activity", "calendrier", "calendar",
        "equipes", "teams and channels", "appels", "calls", "fichiers", "files", "onedrive",
        "applications", "apps", "aide", "help", "parametres", "settings", "notifications",
        "communautes", "communities", "reseau", "network", "nouveau message", "new message",
        "notification", "", "microsoft teams notification", "partage d'ecran", "screen sharing",
    ]

    /// Mots qui signalent une réunion.
    static let meetingHints = [
        "reunion", "meeting", "daily", "standup", "stand-up", "sync", "point", "rétro", "retro",
        "sprint", "planning", "demo", "weekly", "hebdo", "revue", "review", "comite", "copil",
        "atelier", "workshop", "kick-off", "kickoff", "1:1", "entretien", "appel avec", "call with",
        "refinement", "grooming", "brief",
    ]

    static func pickMeetingTitle(_ windows: [WindowCandidate]) -> String? {
        var best: (title: String, score: Int)?
        for window in windows {
            guard let cleaned = clean(window.title) else { continue }
            let folded = fold(cleaned)
            guard !genericTitles.contains(folded) else { continue }
            var score = 1
            if meetingHints.contains(where: { folded.contains(fold($0)) }) { score += 3 }
            // Le nouveau Teams ouvre la réunion dans une fenêtre séparée, non principale.
            if !window.isMain { score += 1 }
            if score > (best?.score ?? 0) { best = (cleaned, score) }
        }
        return best?.title
    }

    /// Retire le suffixe « | Microsoft Teams » et les parties génériques.
    static func clean(_ raw: String) -> String? {
        let separators = CharacterSet(charactersIn: "|")
        let parts = raw.components(separatedBy: separators)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .map(stripDashSuffix)
            .filter { !$0.isEmpty }
            .filter { !fold($0).contains("microsoft teams") && fold($0) != "teams" }
            .filter { !genericTitles.contains(fold($0)) }
        guard let first = parts.first else { return nil }
        return first
    }

    /// « Daily – Microsoft Teams » → « Daily ».
    private static func stripDashSuffix(_ s: String) -> String {
        for sep in [" - ", " – ", " — "] {
            if let range = s.range(of: sep, options: .backwards) {
                let tail = s[range.upperBound...]
                if fold(String(tail)).contains("teams") {
                    return String(s[..<range.lowerBound]).trimmingCharacters(in: .whitespaces)
                }
            }
        }
        return s
    }

    static func fold(_ s: String) -> String {
        TitleText.fold(s)
    }
}

// MARK: - Outils de texte

enum TitleText {
    /// Minuscules, sans accents, sans espaces aux extrémités.
    static func fold(_ s: String) -> String {
        s.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "fr_FR"))
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Mots (lettres et chiffres) du texte replié.
    static func words(_ s: String) -> [String] {
        fold(s).split(whereSeparator: { !$0.isLetter && !$0.isNumber }).map(String.init)
    }

    /// Vrai si l'un des `hints` (repliés) est un mot entier de `s`.
    static func containsWord(_ s: String, anyOf hints: [String]) -> Bool {
        let w = Set(words(s))
        return hints.contains { w.contains(fold($0)) }
    }

    /// Découpe un titre de fenêtre sur ses séparateurs usuels (« | », « - », « – », « — », « · »
    /// entourés d'espaces : « stand-up » ou « abc-defg-hij » restent entiers).
    static func parts(_ s: String) -> [String] {
        let sentinel = "\u{1F}"
        var t = s
        for sep in [" | ", "|", " - ", " – ", " — ", " · "] {
            t = t.replacingOccurrences(of: sep, with: sentinel)
        }
        return t.components(separatedBy: sentinel)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

    /// Retire les décorations de début de titre : compteur de non-lus « (3) », puces « • ● * ».
    static func stripDecorations(_ s: String) -> String {
        s.replacingOccurrences(of: #"^\s*(\(\d+\)|[•●*])\s*"#, with: "", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Fenêtre au premier plan d'abord, puis principale, puis les autres (ordre stable).
    static func foregroundFirst(_ windows: [WindowCandidate]) -> [WindowCandidate] {
        windows.filter(\.isFocused)
            + windows.filter { $0.isMain && !$0.isFocused }
            + windows.filter { !$0.isMain && !$0.isFocused }
    }
}
