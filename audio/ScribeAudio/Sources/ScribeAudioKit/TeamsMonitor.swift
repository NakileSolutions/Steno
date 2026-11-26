// Surveillance permanente de Microsoft Teams.
//
// Toutes les ~2 s :
// - énumère les objets processus du HAL (kAudioHardwarePropertyProcessObjectList) ;
// - retient l'arbre Teams : bundle `com.microsoft.teams2` / `com.microsoft.teams` et leurs
//   auxiliaires (préfixe), plus tout processus audio descendant (PID parent) d'un
//   processus Teams ;
// - un appel est en cours si l'un d'eux utilise le micro (kAudioProcessPropertyIsRunningInput) ;
// - le titre de la réunion est lu par l'API d'accessibilité sur les fenêtres de l'app Teams.
// Émet `teams_call` à chaque changement ; fournit la liste des processus à taper.

import AppKit
import ApplicationServices
import CoreAudio
import Foundation

@available(macOS 14.2, *)
struct TeamsSnapshot: Equatable {
    /// Objets processus HAL de l'arbre Teams (triés), à taper.
    var processObjects: [AudioObjectID]
    /// PID des applications Teams (pour l'accessibilité).
    var appPIDs: [pid_t]
    /// Un processus Teams utilise le micro.
    var inputActive: Bool

    static let empty = TeamsSnapshot(processObjects: [], appPIDs: [], inputActive: false)
}

@available(macOS 14.2, *)
final class TeamsMonitor {
    static let mainBundleIDs = ["com.microsoft.teams2", "com.microsoft.teams"]
    static let bundlePrefix = "com.microsoft.teams"

    /// Appelé à chaque relevé (sur la file du moniteur).
    var onSnapshot: ((TeamsSnapshot) -> Void)?
    /// Appelé quand l'état d'appel ou le titre change (sur la file du moniteur).
    var onCallChange: ((_ active: Bool, _ title: String?) -> Void)?

    private let queue = DispatchQueue(label: "scribe.audio.teams", qos: .utility)
    private let interval: TimeInterval
    private var timer: DispatchSourceTimer?
    private var latest = TeamsSnapshot.empty
    private var callActive = false
    private var inactivePolls = 0
    private var title: String?

    /// Nombre de relevés négatifs consécutifs avant de déclarer la fin de l'appel
    /// (évite les faux « fin d'appel » quand Teams relâche le micro un instant).
    private let inactivePollsToEnd = 2

    init(interval: TimeInterval = 2.0) {
        self.interval = interval
    }

    func start() {
        queue.async { [self] in
            guard timer == nil else { return }
            let t = DispatchSource.makeTimerSource(queue: queue)
            t.schedule(deadline: .now(), repeating: interval, leeway: .milliseconds(200))
            t.setEventHandler { [weak self] in self?.poll() }
            timer = t
            t.resume()
        }
    }

    func stop() {
        queue.sync {
            timer?.cancel()
            timer = nil
        }
    }

    /// Dernier relevé connu.
    func currentSnapshot() -> TeamsSnapshot {
        queue.sync { latest }
    }

    // MARK: Relevé

    private func poll() {
        let snapshot = Self.scan()
        latest = snapshot
        onSnapshot?(snapshot)

        if snapshot.inputActive {
            inactivePolls = 0
            if !callActive {
                callActive = true
                title = TeamsWindowTitle.read(pids: snapshot.appPIDs)
                Log.info("appel Teams détecté (titre : \(title ?? "inconnu"))")
                onCallChange?(true, title)
            } else if title == nil, let t = TeamsWindowTitle.read(pids: snapshot.appPIDs) {
                // Le titre est figé dès qu'on en trouve un plausible.
                title = t
                onCallChange?(true, t)
            }
        } else if callActive {
            inactivePolls += 1
            if inactivePolls >= inactivePollsToEnd {
                callActive = false
                title = nil
                Log.info("fin de l'appel Teams")
                onCallChange?(false, nil)
            }
        }
    }

    static func isTeamsBundle(_ bundleID: String) -> Bool {
        bundleID.lowercased().hasPrefix(bundlePrefix)
    }

    /// Relevé instantané (utilisable hors moniteur, ex. spike S1).
    static func scan() -> TeamsSnapshot {
        var appPIDs = Set<pid_t>()
        for id in mainBundleIDs {
            for app in NSRunningApplication.runningApplications(withBundleIdentifier: id) where !app.isTerminated {
                appPIDs.insert(app.processIdentifier)
            }
        }

        let objects = (try? HAL.processObjectList()) ?? []
        var entries: [(object: AudioObjectID, pid: pid_t?, isTeams: Bool)] = []
        var teamsPIDs = appPIDs
        for object in objects {
            let pid = HAL.processPID(object)
            let isTeams = HAL.processBundleID(object).map(isTeamsBundle) ?? false
            if isTeams, let pid { teamsPIDs.insert(pid) }
            entries.append((object, pid, isTeams))
        }

        var tree: [AudioObjectID] = []
        var inputActive = false
        for entry in entries {
            let belongs = entry.isTeams || (entry.pid.map { descends(from: teamsPIDs, pid: $0) } ?? false)
            guard belongs else { continue }
            tree.append(entry.object)
            if HAL.processIsRunningInput(entry.object) { inputActive = true }
        }
        return TeamsSnapshot(processObjects: tree.sorted(), appPIDs: appPIDs.sorted(), inputActive: inputActive)
    }

    /// Vrai si `pid` est (un descendant d')un processus de `ancestors`.
    private static func descends(from ancestors: Set<pid_t>, pid: pid_t) -> Bool {
        guard !ancestors.isEmpty else { return false }
        var current = pid
        for _ in 0..<16 {
            if ancestors.contains(current) { return true }
            guard let parent = ProcessInfoBSD.parentPID(of: current), parent > 1, parent != current else {
                return false
            }
            current = parent
        }
        return false
    }
}

// MARK: - Titre de la fenêtre de réunion (accessibilité)

struct WindowCandidate: Equatable {
    var title: String
    var isMain: Bool
}

enum TeamsWindowTitle {
    /// Lit les titres des fenêtres des apps Teams et choisit celui d'une réunion.
    /// `nil` si l'accessibilité est refusée ou si aucun titre ne ressemble à une réunion.
    static func read(pids: [pid_t]) -> String? {
        guard AXIsProcessTrusted(), !pids.isEmpty else { return nil }
        var candidates: [WindowCandidate] = []
        for pid in pids {
            let app = AXUIElementCreateApplication(pid)
            // Teams figé ne doit pas bloquer le moniteur.
            AXUIElementSetMessagingTimeout(app, 1.0)
            var value: CFTypeRef?
            guard AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &value) == .success,
                  let windows = value as? [AXUIElement]
            else { continue }
            for window in windows {
                var titleRef: CFTypeRef?
                guard AXUIElementCopyAttributeValue(window, kAXTitleAttribute as CFString, &titleRef) == .success,
                      let title = titleRef as? String
                else { continue }
                var mainRef: CFTypeRef?
                _ = AXUIElementCopyAttributeValue(window, kAXMainAttribute as CFString, &mainRef)
                candidates.append(WindowCandidate(title: title, isMain: (mainRef as? Bool) ?? false))
            }
        }
        return TeamsTitleHeuristics.pickMeetingTitle(candidates)
    }
}

/// Heuristique pure (testée) : quel titre de fenêtre ressemble à une réunion ?
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
        s.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "fr_FR"))
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
