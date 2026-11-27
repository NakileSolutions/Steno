// Surveillance permanente des apps de visio (liste fixée par la commande `watch`, Teams seul
// avant le premier `watch`).
//
// Toutes les ~2 s :
// - énumère les objets processus du HAL (kAudioHardwarePropertyProcessObjectList), avec leur
//   bundle ID et leur PID ;
// - attribue chaque processus à une app : bundle commençant par l'un des préfixes de l'app
//   (auxiliaires compris), sinon descendance (PID parent) d'un processus de l'app ;
// - pour chaque app, un appel est en cours si l'un de ses processus utilise le micro
//   (kAudioProcessPropertyIsRunningInput), avec anti-rebond à la fin (`CallActivity`) ;
// - le titre de l'appel est lu par l'accessibilité sur les fenêtres de l'app
//   (`CallWindowTitle`, heuristiques pures dans `CallTitleHeuristics`).
// État séparé par app : un événement `call` à chaque changement pour cette app. Fournit aussi
// la liste des processus à taper (`CallSnapshot`).

import AppKit
import ApplicationServices
import CoreAudio
import Foundation

// MARK: - Relevés

/// Relevé d'une app surveillée.
struct AppProcessSnapshot: Equatable {
    var app: WatchedApp
    /// Objets processus HAL de l'app (triés), à taper.
    var processObjects: [AudioObjectID]
    /// PID des applications de l'app susceptibles d'avoir des fenêtres (accessibilité).
    var appPIDs: [pid_t]
    /// Un processus de l'app utilise le micro.
    var inputActive: Bool
}

/// Relevé de toutes les apps surveillées (dans l'ordre de la liste `watch`).
struct CallSnapshot: Equatable {
    var apps: [AppProcessSnapshot]

    static let empty = CallSnapshot(apps: [])

    func app(_ id: String) -> AppProcessSnapshot? {
        apps.first { $0.app.id == id }
    }

    /// Union triée, sans doublon, des processus des apps `ids`.
    func processObjects(forApps ids: [String]) -> [AudioObjectID] {
        let wanted = Set(ids)
        return Array(Set(apps.filter { wanted.contains($0.app.id) }.flatMap(\.processObjects))).sorted()
    }

    /// App reconnue pour chaque objet processus.
    var appByProcess: [AudioObjectID: WatchedApp] {
        var map: [AudioObjectID: WatchedApp] = [:]
        for entry in apps {
            for object in entry.processObjects where map[object] == nil { map[object] = entry.app }
        }
        return map
    }
}

// MARK: - Anti-rebond (pur, testé)

/// État d'appel d'une app : début dès qu'un processus utilise le micro, fin après
/// `inactivePollsToEnd` relevés négatifs consécutifs (évite les faux « fin d'appel » quand
/// l'app relâche le micro un instant).
struct CallActivity: Equatable {
    enum Transition: Equatable {
        case none, started, ended
    }

    private(set) var active = false
    private var inactivePolls = 0
    let inactivePollsToEnd: Int

    init(inactivePollsToEnd: Int = 2) {
        self.inactivePollsToEnd = max(1, inactivePollsToEnd)
    }

    mutating func update(inputActive: Bool) -> Transition {
        if inputActive {
            inactivePolls = 0
            guard !active else { return .none }
            active = true
            return .started
        }
        guard active else { return .none }
        inactivePolls += 1
        guard inactivePolls >= inactivePollsToEnd else { return .none }
        active = false
        inactivePolls = 0
        return .ended
    }
}

// MARK: - Choix des apps captées (pur, testé)

enum CaptureAppSelection {
    struct Resolution: Equatable {
        /// Apps dont le son est capté.
        var apps: [WatchedApp]
        /// Identifiants demandés mais absents de la liste surveillée.
        var unknown: [String]
    }

    /// `start.apps` résolu contre la liste surveillée : les apps demandées connues, ou toutes
    /// les apps surveillées si la demande est vide ou ne désigne aucune app connue.
    static func resolve(requested: [String], watched: [WatchedApp]) -> Resolution {
        let known = requested.compactMap { id in watched.first { $0.id == id } }
        let unknown = requested.filter { id in !watched.contains { $0.id == id } }
        return Resolution(apps: known.isEmpty ? watched : known, unknown: unknown)
    }
}

// MARK: - Moniteur

@available(macOS 14.2, *)
final class CallMonitor {
    /// Appelé à chaque relevé (sur la file du moniteur).
    var onSnapshot: ((CallSnapshot) -> Void)?
    /// Appelé quand l'état d'appel ou le titre d'une app change (sur la file du moniteur).
    var onCallChange: ((_ app: WatchedApp, _ active: Bool, _ title: String?) -> Void)?

    private struct Tracking {
        var activity = CallActivity()
        var title: String?
    }

    private let queue = DispatchQueue(label: "steno.audio.calls", qos: .utility)
    private let interval: TimeInterval
    private var timer: DispatchSourceTimer?
    private var apps: [WatchedApp]
    private var latest = CallSnapshot.empty
    private var tracking: [String: Tracking] = [:]

    init(apps: [WatchedApp] = WatchedApp.defaults, interval: TimeInterval = 2.0) {
        self.apps = apps
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

    /// Remplace la liste des apps surveillées (commande `watch`). Un appel en cours dans une
    /// app retirée est déclaré terminé. Un relevé est fait aussitôt.
    func setApps(_ newApps: [WatchedApp]) {
        queue.async { [self] in
            let ids = Set(newApps.map(\.id))
            for app in apps where !ids.contains(app.id) {
                if tracking[app.id]?.activity.active == true {
                    Log.info("\(app.name) n'est plus surveillé pendant un appel : fin de l'appel annoncée")
                    onCallChange?(app, false, nil)
                }
                tracking[app.id] = nil
            }
            apps = newApps
            if timer != nil { poll() }
        }
    }

    /// Dernier relevé du minuteur.
    func currentSnapshot() -> CallSnapshot {
        queue.sync { latest }
    }

    /// Relevé immédiat avec la liste courante, sans toucher à l'état des appels.
    func scanNow() -> CallSnapshot {
        queue.sync { Self.scan(apps: apps) }
    }

    // MARK: Relevé

    private func poll() {
        let snapshot = Self.scan(apps: apps)
        latest = snapshot
        onSnapshot?(snapshot)

        for entry in snapshot.apps {
            let app = entry.app
            var t = tracking[app.id] ?? Tracking()
            switch t.activity.update(inputActive: entry.inputActive) {
            case .started:
                t.title = CallWindowTitle.read(app: app, pids: entry.appPIDs)
                Log.info("appel \(app.name) détecté (titre : \(t.title ?? "inconnu"))")
                onCallChange?(app, true, t.title)
            case .ended:
                t.title = nil
                Log.info("fin de l'appel \(app.name)")
                onCallChange?(app, false, nil)
            case .none:
                // Le titre est figé dès qu'on en trouve un plausible.
                if t.activity.active, t.title == nil, let title = CallWindowTitle.read(app: app, pids: entry.appPIDs) {
                    t.title = title
                    onCallChange?(app, true, title)
                }
            }
            tracking[app.id] = t
        }
    }

    /// Relevé instantané (utilisable hors moniteur : spikes, diagnostic).
    static func scan(apps: [WatchedApp]) -> CallSnapshot {
        guard !apps.isEmpty else { return .empty }
        let selfPID = getpid()

        // 1. Applications lancées : PID à interroger par l'accessibilité, et racines de la
        //    descendance (un processus audio sans bundle reconnu lancé par l'app lui appartient).
        //    À VÉRIFIER : `NSWorkspace.runningApplications` lu hors du fil principal, dans un
        //    binaire sans NSApplication (la liste est mise à jour par la boucle principale).
        var ownerByPID: [pid_t: String] = [:]
        var windowPIDs: [String: Set<pid_t>] = [:]
        for running in NSWorkspace.shared.runningApplications where !running.isTerminated {
            let pid = running.processIdentifier
            guard pid != selfPID, let bundle = running.bundleIdentifier,
                  let app = WatchedApp.match(bundleID: bundle, in: apps)
            else { continue }
            ownerByPID[pid] = app.id
            // Les auxiliaires sans interface (`.prohibited`) n'ont pas de fenêtre.
            if running.activationPolicy != .prohibited { windowPIDs[app.id, default: []].insert(pid) }
        }

        // 2. Objets processus du HAL.
        struct Entry {
            let object: AudioObjectID
            let pid: pid_t?
            let owner: String?
        }
        var entries: [Entry] = []
        for object in (try? HAL.processObjectList()) ?? [] {
            let pid = HAL.processPID(object)
            if pid == selfPID { continue } // Sténo lui-même n'est jamais une app de visio.
            let owner = HAL.processBundleID(object).flatMap { WatchedApp.match(bundleID: $0, in: apps) }?.id
            if let owner, let pid, ownerByPID[pid] == nil { ownerByPID[pid] = owner }
            entries.append(Entry(object: object, pid: pid, owner: owner))
        }

        // 3. Attribution : bundle, sinon descendance d'un processus d'une app.
        var objects: [String: [AudioObjectID]] = [:]
        var inputActive: [String: Bool] = [:]
        for entry in entries {
            let inherited = entry.pid.flatMap { ancestorOwner(of: $0, owners: ownerByPID) }
            guard let owner = entry.owner ?? inherited else { continue }
            objects[owner, default: []].append(entry.object)
            if HAL.processIsRunningInput(entry.object) { inputActive[owner] = true }
        }

        return CallSnapshot(apps: apps.map { app in
            AppProcessSnapshot(
                app: app,
                processObjects: (objects[app.id] ?? []).sorted(),
                appPIDs: (windowPIDs[app.id] ?? []).sorted(),
                inputActive: inputActive[app.id] ?? false
            )
        })
    }

    /// App du premier ancêtre de `pid` (lui compris) présent dans `owners`.
    private static func ancestorOwner(of pid: pid_t, owners: [pid_t: String]) -> String? {
        guard !owners.isEmpty else { return nil }
        var current = pid
        for _ in 0..<16 {
            if let owner = owners[current] { return owner }
            guard let parent = ProcessInfoBSD.parentPID(of: current), parent > 1, parent != current else {
                return nil
            }
            current = parent
        }
        return nil
    }
}

// MARK: - Titre de l'appel (accessibilité)

enum CallWindowTitle {
    /// Lit les fenêtres des applications `pids` de l'app et choisit le titre de l'appel.
    /// `nil` si l'accessibilité est refusée ou si aucun titre ne convient.
    static func read(app: WatchedApp, pids: [pid_t]) -> String? {
        let windows = windows(pids: pids)
        guard !windows.isEmpty else { return nil }
        return CallTitleHeuristics.pickTitle(for: app, windows: windows)
    }

    /// Titres et rôles (principale, premier plan) des fenêtres des applications `pids`.
    static func windows(pids: [pid_t]) -> [WindowCandidate] {
        guard AXIsProcessTrusted(), !pids.isEmpty else { return [] }
        var candidates: [WindowCandidate] = []
        for pid in pids {
            let appElement = AXUIElementCreateApplication(pid)
            // Une app figée ne doit pas bloquer le moniteur.
            AXUIElementSetMessagingTimeout(appElement, 1.0)
            // À VÉRIFIER (spike S1-bis) : AXFocusedWindow renseigné pour une app en arrière-plan,
            // et égalité des éléments par CFEqual.
            let focused = element(appElement, kAXFocusedWindowAttribute)
            var value: CFTypeRef?
            guard AXUIElementCopyAttributeValue(appElement, kAXWindowsAttribute as CFString, &value) == .success,
                  let windows = value as? [AXUIElement]
            else { continue }
            for window in windows {
                var titleRef: CFTypeRef?
                guard AXUIElementCopyAttributeValue(window, kAXTitleAttribute as CFString, &titleRef) == .success,
                      let title = titleRef as? String
                else { continue }
                var mainRef: CFTypeRef?
                _ = AXUIElementCopyAttributeValue(window, kAXMainAttribute as CFString, &mainRef)
                let isFocused = focused.map { CFEqual($0, window) } ?? false
                candidates.append(WindowCandidate(title: title, isMain: (mainRef as? Bool) ?? false, isFocused: isFocused))
            }
        }
        return candidates
    }

    /// Attribut de type élément d'accessibilité (ex. fenêtre au premier plan).
    private static func element(_ parent: AXUIElement, _ attribute: String) -> AXUIElement? {
        var ref: CFTypeRef?
        guard AXUIElementCopyAttributeValue(parent, attribute as CFString, &ref) == .success,
              let ref, CFGetTypeID(ref) == AXUIElementGetTypeID()
        else { return nil }
        return unsafeBitCast(ref, to: AXUIElement.self)
    }
}
