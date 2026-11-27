import XCTest
@testable import StenoAudioKit

final class CallTitleHeuristicsTests: XCTestCase {
    private let teams = WatchedApp.defaults[0]
    private let slack = WatchedApp(id: "slack", name: "Slack", bundlePrefixes: ["com.tinyspeck.slackmacgap"])
    private let zoom = WatchedApp(id: "zoom", name: "Zoom", bundlePrefixes: ["us.zoom.xos"])
    private let chrome = WatchedApp(id: "meet", name: "Google Meet", bundlePrefixes: ["com.google.Chrome", "com.apple.Safari"])
    private let webex = WatchedApp(id: "webex", name: "Webex", bundlePrefixes: ["Cisco-Systems.Spark"])

    private func w(_ title: String, main: Bool = false, focused: Bool = false) -> WindowCandidate {
        WindowCandidate(title: title, isMain: main, isFocused: focused)
    }

    // MARK: Famille d'app

    func testKind() {
        XCTAssertEqual(CallTitleHeuristics.kind(of: teams), .teams)
        XCTAssertEqual(CallTitleHeuristics.kind(of: slack), .slack)
        XCTAssertEqual(CallTitleHeuristics.kind(of: zoom), .zoom)
        XCTAssertEqual(CallTitleHeuristics.kind(of: chrome), .browser)
        XCTAssertEqual(CallTitleHeuristics.kind(of: WatchedApp(id: "edge", name: "Edge", bundlePrefixes: ["com.microsoft.edgemac"])), .browser)
        XCTAssertEqual(CallTitleHeuristics.kind(of: WatchedApp(id: "visio", name: "Visio", bundlePrefixes: ["com.tinyspeck.slackmacgap"])), .slack,
                       "reconnue par le bundle quel que soit l'identifiant")
        XCTAssertEqual(CallTitleHeuristics.kind(of: webex), .other)
    }

    // MARK: Teams (inchangé)

    func testTeams() {
        let windows = [w("Chat | Microsoft Teams", main: true, focused: true), w("Daily ORION | Microsoft Teams")]
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: teams, windows: windows), "Daily ORION")
        XCTAssertNil(CallTitleHeuristics.pickTitle(for: teams, windows: [w("Microsoft Teams", main: true)]))
    }

    // MARK: Slack

    func testSlackHuddleWindow() {
        let windows = [
            w("#general - Acme - Slack", main: true, focused: true),
            w("Huddle : #equipe-flotte - Acme - Slack"),
        ]
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: slack, windows: windows), "Huddle : #equipe-flotte")
    }

    func testSlackVariants() {
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: slack, windows: [w("Slack | Huddle dans #equipe-flotte")]),
                       "Huddle dans #equipe-flotte")
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: slack, windows: [w("Appel avec Marie Dupont - Slack")]),
                       "Appel avec Marie Dupont")
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: slack, windows: [w("(3) Huddle in #general - Acme - Slack")]),
                       "Huddle in #general", "compteur de non-lus retiré")
        // Le mot seul ne suffit pas : on garde la partie suivante (canal ou personne).
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: slack, windows: [w("Huddle - Marie Dupont - Acme - Slack")]),
                       "Huddle - Marie Dupont")
    }

    func testSlackWithoutHuddleWindowGivesNil() {
        let windows = [w("#appels-offres - Acme - Slack", main: true, focused: true), w("Slack")]
        XCTAssertNil(CallTitleHeuristics.pickTitle(for: slack, windows: windows),
                     "le canal affiché n'est pas forcément celui du huddle")
    }

    // MARK: Zoom

    func testZoomMeetingWindow() {
        let windows = [w("Zoom Workplace", main: true), w("Zoom Meeting")]
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: zoom, windows: windows), "Zoom Meeting")
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: zoom, windows: [w("Réunion Zoom")]), "Réunion Zoom")
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: zoom, windows: [w("Zoom Meeting 40-Minutes")]), "Zoom Meeting")
    }

    func testZoomTopicWins() {
        let windows = [w("Zoom Meeting"), w("Daily standup - Zoom Meeting")]
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: zoom, windows: windows), "Daily standup")
    }

    func testZoomHomeWindowGivesNil() {
        XCTAssertNil(CallTitleHeuristics.pickTitle(for: zoom, windows: [w("Zoom", main: true, focused: true), w("Zoom Workplace")]))
    }

    // MARK: Navigateurs (Google Meet)

    func testMeetTabWithEventName() {
        let windows = [w("Meet – Point hebdo - Google Chrome - Travail", main: true, focused: true)]
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: chrome, windows: windows), "Point hebdo")
    }

    func testMeetTabWithCodeOnly() {
        let windows = [
            w("Boîte de réception - Gmail - Google Chrome", main: true, focused: true),
            w("Meet - abc-defg-hij - Google Chrome"),
        ]
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: chrome, windows: windows), "Google Meet")
    }

    func testMeetColonPrefix() {
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: chrome, windows: [w("Meet: Revue de sprint")]), "Revue de sprint")
    }

    func testNonMeetTabsGiveNil() {
        let windows = [
            w("Meeting notes - Google Docs - Google Chrome", focused: true),
            w("Let's meet tomorrow - Gmail - Google Chrome"),
        ]
        XCTAssertNil(CallTitleHeuristics.pickTitle(for: chrome, windows: windows))
    }

    // MARK: Par défaut

    func testDefaultUsesForegroundWindow() {
        let windows = [w("Webex", main: true), w("Réunion personnelle de Marie Dupont - Webex", focused: true)]
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: webex, windows: windows), "Réunion personnelle de Marie Dupont")
    }

    func testDefaultRejectsGenericTitles() {
        XCTAssertNil(CallTitleHeuristics.pickTitle(for: webex, windows: [w("Webex", main: true, focused: true)]))
        XCTAssertNil(CallTitleHeuristics.pickTitle(for: webex, windows: [w("  ", focused: true)]))
        XCTAssertNil(CallTitleHeuristics.pickTitle(for: webex, windows: []))
        // Une fenêtre quelconque (ni principale ni au premier plan) n'est pas retenue.
        XCTAssertNil(CallTitleHeuristics.pickTitle(for: webex, windows: [w("webex", focused: true), w("Préférences")]))
    }

    func testDefaultFallsBackToMainThenAnyWindow() {
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: webex, windows: [w("Webex", focused: true), w("Point projet", main: true)]),
                       "Point projet")
        XCTAssertEqual(CallTitleHeuristics.pickTitle(for: webex, windows: [w("Point projet | Webex")]), "Point projet",
                       "sans information de premier plan, toutes les fenêtres comptent")
    }

    // MARK: Outils

    func testTitleTextParts() {
        XCTAssertEqual(TitleText.parts("Stand-up - abc-defg-hij | Slack"), ["Stand-up", "abc-defg-hij", "Slack"])
        XCTAssertTrue(TitleText.containsWord("Huddle : #équipe", anyOf: ["huddle"]))
        XCTAssertFalse(TitleText.containsWord("#appels-offres", anyOf: ["appel"]))
    }
}

final class CallActivityTests: XCTestCase {
    func testDebounce() {
        var a = CallActivity()
        XCTAssertEqual(a.update(inputActive: false), .none)
        XCTAssertEqual(a.update(inputActive: true), .started)
        XCTAssertTrue(a.active)
        XCTAssertEqual(a.update(inputActive: true), .none)
        // Micro relâché un instant : pas de fin d'appel.
        XCTAssertEqual(a.update(inputActive: false), .none)
        XCTAssertEqual(a.update(inputActive: true), .none)
        XCTAssertEqual(a.update(inputActive: false), .none)
        XCTAssertEqual(a.update(inputActive: false), .ended)
        XCTAssertFalse(a.active)
        XCTAssertEqual(a.update(inputActive: false), .none)
        XCTAssertEqual(a.update(inputActive: true), .started)
    }

    func testStatesAreIndependentPerApp() {
        var teams = CallActivity()
        var slack = CallActivity()
        XCTAssertEqual(teams.update(inputActive: true), .started)
        XCTAssertEqual(slack.update(inputActive: false), .none)
        XCTAssertEqual(slack.update(inputActive: true), .started)
        XCTAssertEqual(teams.update(inputActive: false), .none)
        XCTAssertEqual(teams.update(inputActive: false), .ended)
        XCTAssertTrue(slack.active)
    }
}

final class CaptureSelectionTests: XCTestCase {
    private let teams = WatchedApp.defaults[0]
    private let slack = WatchedApp(id: "slack", name: "Slack", bundlePrefixes: ["com.tinyspeck.slackmacgap"])

    func testResolve() {
        let watched = [teams, slack]
        XCTAssertEqual(CaptureAppSelection.resolve(requested: [], watched: watched),
                       .init(apps: watched, unknown: []), "vide = toutes les apps surveillées")
        XCTAssertEqual(CaptureAppSelection.resolve(requested: ["slack"], watched: watched),
                       .init(apps: [slack], unknown: []))
        XCTAssertEqual(CaptureAppSelection.resolve(requested: ["slack", "zoom"], watched: watched),
                       .init(apps: [slack], unknown: ["zoom"]))
        XCTAssertEqual(CaptureAppSelection.resolve(requested: ["zoom"], watched: watched),
                       .init(apps: watched, unknown: ["zoom"]), "aucune app connue : repli sur toutes")
        XCTAssertEqual(CaptureAppSelection.resolve(requested: [], watched: []), .init(apps: [], unknown: []))
    }

    func testSnapshotUnion() {
        let snapshot = CallSnapshot(apps: [
            AppProcessSnapshot(app: teams, processObjects: [3, 5], appPIDs: [100], inputActive: true),
            AppProcessSnapshot(app: slack, processObjects: [9, 12], appPIDs: [], inputActive: false),
        ])
        XCTAssertEqual(snapshot.processObjects(forApps: ["teams", "slack"]), [3, 5, 9, 12])
        XCTAssertEqual(snapshot.processObjects(forApps: ["slack"]), [9, 12])
        XCTAssertEqual(snapshot.processObjects(forApps: ["zoom"]), [])
        XCTAssertEqual(snapshot.appByProcess[9]?.id, "slack")
        XCTAssertNil(snapshot.appByProcess[42])
        XCTAssertEqual(snapshot.app("teams")?.inputActive, true)
    }

    func testCatalogArgument() {
        XCTAssertEqual(WatchedApp.fromCatalog(argument: "teams,Slack").apps.map(\.id), ["teams", "slack"])
        let mixed = WatchedApp.fromCatalog(argument: "teams, zoom ,inconnu,teams")
        XCTAssertEqual(mixed.apps.map(\.id), ["teams", "zoom"])
        XCTAssertEqual(mixed.unknown, ["inconnu"])
        XCTAssertEqual(WatchedApp.fromCatalog(argument: "").apps, WatchedApp.defaults)
        XCTAssertTrue(WatchedApp.catalog.contains { $0.id == "meet" })
    }
}
