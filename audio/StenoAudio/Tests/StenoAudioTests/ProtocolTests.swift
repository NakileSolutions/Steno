import Foundation
import XCTest
@testable import StenoAudioKit

final class ProtocolTests: XCTestCase {
    // MARK: Encodage (sidecar → cœur)

    private func encode(_ message: OutgoingMessage, file: StaticString = #filePath, line: UInt = #line) throws -> [String: Any] {
        let data = try Output.encodeLine(message)
        XCTAssertEqual(data.last, 0x0A, "chaque message se termine par \\n", file: file, line: line)
        XCTAssertFalse(data.dropLast().contains(0x0A), "un message tient sur une seule ligne", file: file, line: line)
        let object = try JSONSerialization.jsonObject(with: data)
        return try XCTUnwrap(object as? [String: Any], file: file, line: line)
    }

    func testReady() throws {
        let json = try encode(.ready(version: "0.1.0", asr: "parakeet-tdt-v3"))
        XCTAssertEqual(json["type"] as? String, "ready")
        XCTAssertEqual(json["version"] as? String, "0.1.0")
        XCTAssertEqual(json["asr"] as? String, "parakeet-tdt-v3")
        XCTAssertEqual(json.count, 3)
    }

    func testPermissions() throws {
        let json = try encode(.permissions(PermissionsSnapshot(microphone: .granted, systemAudio: .unknown, accessibility: .denied)))
        XCTAssertEqual(json["type"] as? String, "permissions")
        XCTAssertEqual(json["microphone"] as? String, "granted")
        XCTAssertEqual(json["system_audio"] as? String, "unknown")
        XCTAssertEqual(json["accessibility"] as? String, "denied")
    }

    func testCallWithTitle() throws {
        let json = try encode(.call(active: true, app: "slack", title: "Huddle dans #equipe-flotte"))
        XCTAssertEqual(json["type"] as? String, "call")
        XCTAssertEqual(json["active"] as? Bool, true)
        XCTAssertEqual(json["app"] as? String, "slack")
        XCTAssertEqual(json["title"] as? String, "Huddle dans #equipe-flotte")
        XCTAssertEqual(json.count, 4)
    }

    func testCallWithoutTitleEncodesExplicitNull() throws {
        let json = try encode(.call(active: false, app: "teams", title: nil))
        XCTAssertEqual(json["type"] as? String, "call")
        XCTAssertEqual(json["active"] as? Bool, false)
        XCTAssertEqual(json["app"] as? String, "teams")
        XCTAssertTrue(json.keys.contains("title"), "title doit être présent")
        XCTAssertTrue(json["title"] is NSNull, "title doit valoir null")
    }

    func testOldTeamsCallFormatIsNoLongerEmitted() throws {
        let data = try Output.encodeLine(.call(active: true, app: "teams", title: "Daily"))
        let raw = try XCTUnwrap(String(data: data, encoding: .utf8))
        XCTAssertFalse(raw.contains("teams_call"))
    }

    func testSegment() throws {
        let payload = SegmentPayload(ts: "2026-11-26T10:41:12+01:00", t0: 668.2, t1: 672.912_345,
                                     channel: .me, text: "Sténo, cherche les erreurs sur ms-booking…", isPrivate: false)
        let data = try Output.encodeLine(.segment(payload))
        let raw = try XCTUnwrap(String(data: data, encoding: .utf8))
        XCTAssertTrue(raw.contains("…"), "UTF-8 brut, pas d'échappement \\u")
        XCTAssertFalse(raw.contains("\\/"), "pas d'échappement des barres obliques")

        let json = try encode(.segment(payload))
        XCTAssertEqual(json["type"] as? String, "segment")
        XCTAssertEqual(json["ts"] as? String, "2026-11-26T10:41:12+01:00")
        XCTAssertEqual(json["t0"] as? Double, 668.2)
        XCTAssertEqual(json["t1"] as? Double, 672.91, "t1 arrondi au centième")
        XCTAssertEqual(json["channel"] as? String, "me")
        XCTAssertEqual(json["text"] as? String, "Sténo, cherche les erreurs sur ms-booking…")
        XCTAssertEqual(json["private"] as? Bool, false)
        XCTAssertEqual(json.count, 7)
    }

    func testLevelStateWakeTest() throws {
        let level = try encode(.level(me: -32.54, them: -24.1))
        XCTAssertEqual(level["type"] as? String, "level")
        XCTAssertEqual(level["me"] as? Double, -32.5)
        XCTAssertEqual(level["them"] as? Double, -24.1)

        let state = try encode(.state(.listening))
        XCTAssertEqual(state["type"] as? String, "state")
        XCTAssertEqual(state["state"] as? String, "listening")

        let wake = try encode(.wakeTest(text: "steno cherche quelque chose"))
        XCTAssertEqual(wake["type"] as? String, "wake_test")
        XCTAssertEqual(wake["text"] as? String, "steno cherche quelque chose")
    }

    func testDiarization() throws {
        let json = try encode(.diarization(meetingId: "m1", turns: [DiarizationTurn(t0: 12.0, t1: 18.5, speaker: "S1")]))
        XCTAssertEqual(json["type"] as? String, "diarization")
        XCTAssertEqual(json["meeting_id"] as? String, "m1")
        let turns = try XCTUnwrap(json["turns"] as? [[String: Any]])
        XCTAssertEqual(turns.count, 1)
        XCTAssertEqual(turns[0]["t0"] as? Double, 12.0)
        XCTAssertEqual(turns[0]["t1"] as? Double, 18.5)
        XCTAssertEqual(turns[0]["speaker"] as? String, "S1")
    }

    func testErrorCodes() throws {
        let json = try encode(.error(code: .tapFailed, message: "échec", recoverable: true))
        XCTAssertEqual(json["type"] as? String, "error")
        XCTAssertEqual(json["code"] as? String, "tap_failed")
        XCTAssertEqual(json["message"] as? String, "échec")
        XCTAssertEqual(json["recoverable"] as? Bool, true)

        let expected: [ErrorCode: String] = [
            .micDenied: "mic_denied", .systemAudioDenied: "system_audio_denied", .tapFailed: "tap_failed",
            .deviceChanged: "device_changed", .asrFailed: "asr_failed", .modelMissing: "model_missing",
            .badCommand: "bad_command", .deviceMissing: "device_missing",
        ]
        for (code, raw) in expected { XCTAssertEqual(code.rawValue, raw) }
    }

    func testDevicesEncoding() throws {
        let list = AudioDeviceList(
            inputs: [
                AudioDeviceInfo(id: "BuiltInMicrophoneDevice", name: "Micro du MacBook Pro", isDefault: true, transport: .builtin),
                AudioDeviceInfo(id: "AirPodsUID:input", name: "AirPods Pro", isDefault: false, transport: .bluetooth),
            ],
            outputs: [AudioDeviceInfo(id: "AirPodsUID", name: "AirPods Pro", isDefault: true, transport: .bluetooth)]
        )
        let json = try encode(.devices(list))
        XCTAssertEqual(json["type"] as? String, "devices")
        XCTAssertEqual(json.count, 3)
        let inputs = try XCTUnwrap(json["inputs"] as? [[String: Any]])
        let outputs = try XCTUnwrap(json["outputs"] as? [[String: Any]])
        XCTAssertEqual(inputs.count, 2)
        XCTAssertEqual(outputs.count, 1)
        XCTAssertEqual(inputs[0]["id"] as? String, "BuiltInMicrophoneDevice")
        XCTAssertEqual(inputs[0]["name"] as? String, "Micro du MacBook Pro")
        XCTAssertEqual(inputs[0]["default"] as? Bool, true)
        XCTAssertEqual(inputs[0]["transport"] as? String, "builtin")
        XCTAssertEqual(inputs[0].count, 4, "id, name, default, transport")
        XCTAssertEqual(inputs[1]["default"] as? Bool, false)
        XCTAssertEqual(inputs[1]["transport"] as? String, "bluetooth")
        XCTAssertEqual(outputs[0]["id"] as? String, "AirPodsUID")
        XCTAssertEqual(outputs[0]["default"] as? Bool, true)

        // Listes vides : tableaux vides, pas de champ absent.
        let empty = try encode(.devices(.empty))
        XCTAssertEqual((empty["inputs"] as? [Any])?.count, 0)
        XCTAssertEqual((empty["outputs"] as? [Any])?.count, 0)

        // Tous les transports s'encodent selon le contrat.
        XCTAssertEqual(AudioTransport.allCases.map(\.rawValue),
                       ["builtin", "usb", "bluetooth", "virtual", "aggregate", "other"])
    }

    func testDeviceMissingError() throws {
        let json = try encode(.error(code: .deviceMissing, message: DeviceMessages.inputMissing("AirPodsUID"), recoverable: true))
        XCTAssertEqual(json["code"] as? String, "device_missing")
        XCTAssertEqual(json["message"] as? String, "Micro « AirPodsUID » introuvable : micro par défaut utilisé.")
        XCTAssertEqual(json["recoverable"] as? Bool, true)
    }

    func testNonFiniteNumbersDoNotBreakEncoding() throws {
        let json = try encode(.level(me: -.infinity, them: .nan))
        XCTAssertNotNil(json["me"] as? Double)
        XCTAssertNotNil(json["them"] as? Double)
    }

    func testOutputPreservesOrderOneLinePerMessage() {
        let lock = NSLock()
        var lines: [Data] = []
        let output = Output(writer: { data in
            lock.lock(); lines.append(data); lock.unlock()
            return true
        })
        output.send(.ready(version: "0.1.0", asr: "x"))
        output.send(.state(.listening))
        output.send(.state(.stopped))
        output.drain()
        XCTAssertEqual(lines.count, 3)
        let types = lines.compactMap { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] }
            .compactMap { $0["type"] as? String }
        XCTAssertEqual(types, ["ready", "state", "state"])
    }

    // MARK: Décodage (cœur → sidecar)

    func testParseStartFull() {
        let line = #"{"cmd":"start","vocabulary":["Sténo","Atlas"],"silence_ms":1000,"keep_audio":true,"audio_dir":"/tmp/Steno/m1"}"#
        guard case let .success(.start(opts)) = CommandParser.parse(line) else { return XCTFail() }
        XCTAssertEqual(opts.vocabulary, ["Sténo", "Atlas"])
        XCTAssertEqual(opts.silenceMs, 1000)
        XCTAssertTrue(opts.keepAudio)
        XCTAssertEqual(opts.audioDir, "/tmp/Steno/m1")
        XCTAssertEqual(opts.apps, [], "apps absent = toutes les apps surveillées")
    }

    func testParseStartApps() {
        let line = #"{"cmd":"start","vocabulary":[],"silence_ms":1000,"keep_audio":false,"audio_dir":"/x","apps":["slack"]}"#
        guard case let .success(.start(opts)) = CommandParser.parse(line) else { return XCTFail() }
        XCTAssertEqual(opts.apps, ["slack"])

        // Nettoyage : espaces, vides et doublons retirés, ordre conservé.
        guard case let .success(.start(messy)) = CommandParser.parse(#"{"cmd":"start","apps":[" zoom ","","slack","zoom"]}"#)
        else { return XCTFail() }
        XCTAssertEqual(messy.apps, ["zoom", "slack"])

        guard case let .success(.start(empty)) = CommandParser.parse(#"{"cmd":"start","apps":[]}"#) else { return XCTFail() }
        XCTAssertEqual(empty.apps, [])

        XCTAssertEqual(CommandParser.parse(#"{"cmd":"start","apps":"slack"}"#),
                       .failure(.invalidField(command: "start", field: "apps")))
    }

    func testParseWatch() {
        let line = #"{"cmd":"watch","apps":[{"id":"teams","name":"Microsoft Teams","bundles":["com.microsoft.teams2","com.microsoft.teams"]},{"id":"slack","name":"Slack","bundles":["com.tinyspeck.slackmacgap"]}]}"#
        XCTAssertEqual(CommandParser.parse(line), .success(.watch([
            WatchedApp(id: "teams", name: "Microsoft Teams", bundlePrefixes: ["com.microsoft.teams2", "com.microsoft.teams"]),
            WatchedApp(id: "slack", name: "Slack", bundlePrefixes: ["com.tinyspeck.slackmacgap"]),
        ])))
    }

    func testParseWatchIgnoresUnknownFieldsAndBadEntries() {
        let entries = [
            #"{"id":"zoom","bundles":["us.zoom.xos"],"icone":"zoom.png"}"#,
            #"{"name":"Sans identifiant","bundles":["com.example"]}"#,
            #"{"id":"vide","name":"Sans préfixe","bundles":[]}"#,
            #"{"id":"zoom","name":"Doublon","bundles":["us.zoom.autre"]}"#,
            #"{"id":" meet ","name":"  ","bundles":[" com.google.Chrome ",""]}"#,
        ]
        let line = #"{"cmd":"watch","futur":true,"apps":["# + entries.joined(separator: ",") + "]}"
        XCTAssertEqual(CommandParser.parse(line), .success(.watch([
            // Nom absent ou vide : l'identifiant sert de nom.
            WatchedApp(id: "zoom", name: "zoom", bundlePrefixes: ["us.zoom.xos"]),
            WatchedApp(id: "meet", name: "meet", bundlePrefixes: ["com.google.Chrome"]),
        ])))
    }

    func testParseWatchEmptyAndInvalid() {
        // Liste vide : plus aucune app surveillée.
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"watch","apps":[]}"#), .success(.watch([])))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"watch"}"#), .failure(.invalidField(command: "watch", field: "apps")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"watch","apps":{"id":"teams"}}"#),
                       .failure(.invalidField(command: "watch", field: "apps")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"watch","apps":[{"id":3,"bundles":["x"]}]}"#),
                       .failure(.invalidField(command: "watch", field: "apps")))
    }

    func testWatchedAppMatching() {
        let teams = WatchedApp(id: "teams", name: "Teams", bundlePrefixes: ["com.microsoft.teams2", "com.microsoft.teams"])
        let slack = WatchedApp(id: "slack", name: "Slack", bundlePrefixes: ["com.tinyspeck.slackmacgap"])
        let microsoft = WatchedApp(id: "ms", name: "Microsoft", bundlePrefixes: ["com.microsoft"])
        let apps = [microsoft, teams, slack]

        XCTAssertTrue(teams.matches(bundleID: "com.microsoft.teams2.helper.renderer"))
        XCTAssertTrue(slack.matches(bundleID: "COM.TinySpeck.SlackMacGap.helper"), "comparaison sans casse")
        XCTAssertFalse(slack.matches(bundleID: "com.tinyspeck"))
        XCTAssertEqual(WatchedApp.match(bundleID: "com.microsoft.teams2.helper", in: apps)?.id, "teams", "préfixe le plus long")
        XCTAssertEqual(WatchedApp.match(bundleID: "com.microsoft.edgemac", in: apps)?.id, "ms")
        XCTAssertEqual(WatchedApp.match(bundleID: "com.tinyspeck.slackmacgap", in: apps)?.id, "slack")
        XCTAssertNil(WatchedApp.match(bundleID: "us.zoom.xos", in: apps))
        XCTAssertNil(WatchedApp.match(bundleID: "com.apple.Safari", in: []))
    }

    func testDefaultWatchListIsTeamsOnly() {
        XCTAssertEqual(WatchedApp.defaults.map(\.id), ["teams"])
        XCTAssertEqual(WatchedApp.defaults[0].bundlePrefixes, ["com.microsoft.teams2", "com.microsoft.teams"])
    }

    func testParseStartDefaultsAndClamping() {
        guard case let .success(.start(opts)) = CommandParser.parse(#"{"cmd":"start"}"#) else { return XCTFail() }
        XCTAssertEqual(opts, StartOptions())
        XCTAssertEqual(opts.silenceMs, 1000)
        XCTAssertFalse(opts.keepAudio)
        XCTAssertNil(opts.audioDir)

        guard case let .success(.start(low)) = CommandParser.parse(#"{"cmd":"start","silence_ms":10}"#) else { return XCTFail() }
        XCTAssertEqual(low.silenceMs, 300)
        guard case let .success(.start(dbl)) = CommandParser.parse(#"{"cmd":"start","silence_ms":800.0}"#) else { return XCTFail() }
        XCTAssertEqual(dbl.silenceMs, 800)
    }

    // MARK: Périphériques

    func testParseStartInputDevice() {
        // Absent : nil (le choix mémorisé par set_input s'applique).
        guard case let .success(.start(absent)) = CommandParser.parse(#"{"cmd":"start"}"#) else { return XCTFail() }
        XCTAssertNil(absent.inputDevice)

        // null : micro par défaut du système.
        guard case let .success(.start(null)) = CommandParser.parse(#"{"cmd":"start","input_device":null}"#)
        else { return XCTFail() }
        XCTAssertEqual(null.inputDevice, .systemDefault)

        // UID (espaces retirés).
        let line = #"{"cmd":"start","vocabulary":["Sténo"],"silence_ms":1000,"keep_audio":false,"audio_dir":"/x","apps":["slack"],"input_device":" BuiltInMicrophoneDevice "}"#
        guard case let .success(.start(opts)) = CommandParser.parse(line) else { return XCTFail() }
        XCTAssertEqual(opts.inputDevice, .uid("BuiltInMicrophoneDevice"))
        XCTAssertEqual(opts.inputDevice?.uid, "BuiltInMicrophoneDevice")
        XCTAssertEqual(opts.apps, ["slack"], "les autres champs restent lus")
        XCTAssertEqual(opts.vocabulary, ["Sténo"])

        // Chaîne vide : micro par défaut.
        guard case let .success(.start(empty)) = CommandParser.parse(#"{"cmd":"start","input_device":""}"#)
        else { return XCTFail() }
        XCTAssertEqual(empty.inputDevice, .systemDefault)
        XCTAssertNil(DeviceSelection.systemDefault.uid)

        // Mauvais type.
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"start","input_device":42}"#),
                       .failure(.invalidField(command: "start", field: "input_device")))
    }

    func testParseSetInput() {
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"set_input","device":"BuiltInMicrophoneDevice"}"#),
                       .success(.setInput(.uid("BuiltInMicrophoneDevice"))))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"set_input","device":null}"#),
                       .success(.setInput(.systemDefault)))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"set_input","device":"AirPodsUID","futur":1}"#),
                       .success(.setInput(.uid("AirPodsUID"))), "champs inconnus ignorés")
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"set_input"}"#),
                       .failure(.invalidField(command: "set_input", field: "device")), "device obligatoire")
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"set_input","device":["x"]}"#),
                       .failure(.invalidField(command: "set_input", field: "device")))
    }

    func testParseDevices() {
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"devices"}"#), .success(.devices))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"devices","inputs":true}"#), .success(.devices))
    }

    func testParsePlay() {
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"play","sound":"detect","device":null}"#),
                       .success(.play(sound: .detect, device: .systemDefault)))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"play","sound":"detect","device":"AirPodsUID"}"#),
                       .success(.play(sound: .detect, device: .uid("AirPodsUID"))))
        // Champs absents : son `detect`, sortie par défaut.
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"play"}"#),
                       .success(.play(sound: .detect, device: .systemDefault)))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"play","sound":"fanfare"}"#),
                       .failure(.invalidField(command: "play", field: "sound")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"play","sound":3}"#),
                       .failure(.invalidField(command: "play", field: "sound")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"play","device":false}"#),
                       .failure(.invalidField(command: "play", field: "device")))
    }

    func testUnknownFieldsAreIgnored() {
        let line = #"{"cmd":"aside","on":true,"futur":{"x":1},"autre":[1,2]}"#
        XCTAssertEqual(CommandParser.parse(line), .success(.aside(on: true)))
    }

    func testParseSimpleCommands() {
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"stop"}"#), .success(.stop))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"aside","on":false}"#), .success(.aside(on: false)))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"wake_test","seconds":8}"#), .success(.wakeTest(seconds: 8)))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"wake_test"}"#), .success(.wakeTest(seconds: 8)))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"wake_test","seconds":600}"#), .success(.wakeTest(seconds: 30)))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"permissions"}"#), .success(.permissions))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"shutdown"}"#), .success(.shutdown))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"diarize","meeting_id":"m1","audio_dir":"/a/b"}"#),
                       .success(.diarize(meetingId: "m1", audioDir: "/a/b")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"diarize","meeting_id":"m1","audio_dir":"/a/b","file":"me.wav"}"#),
                       .success(.diarize(meetingId: "m1", audioDir: "/a/b", file: "me.wav")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"diarize","meeting_id":"m1","audio_dir":"/a/b","file":"../x"}"#),
                       .failure(.invalidField(command: "diarize", field: "file")))
        XCTAssertEqual(
            CommandParser.parse(#"{"cmd":"retranscribe","meeting_id":"m1","audio_dir":"/a","language":"fr","items":[{"id":3,"file":"me.wav","t0":1.5,"t1":4}]}"#),
            .success(.retranscribe(meetingId: "m1", audioDir: "/a", language: "fr", vocabulary: [],
                                   items: [RetranscribeItem(id: 3, file: "me.wav", t0: 1.5, t1: 4)])))
        XCTAssertEqual(
            CommandParser.parse(#"{"cmd":"retranscribe","meeting_id":"m1","audio_dir":"/a","language":"fr","items":[{"id":3,"file":"../x.wav","t0":1,"t1":2}]}"#),
            .failure(.invalidField(command: "retranscribe", field: "items")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"pause"}"#), .success(.pause))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"resume"}"#), .success(.resume))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"mute","on":true}"#), .success(.mute(on: true)))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"request_permission","kind":"system_audio"}"#),
                       .success(.requestPermission(.systemAudio)))
    }

    func testParseStartRoom() {
        guard case let .success(.start(o)) = CommandParser.parse(#"{"cmd":"start","apps":[],"room":true}"#) else {
            return XCTFail("start attendu")
        }
        XCTAssertTrue(o.room)
        guard case let .success(.start(d)) = CommandParser.parse(#"{"cmd":"start"}"#) else { return XCTFail("start attendu") }
        XCTAssertFalse(d.room)
    }

    func testBadCommands() {
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"danse"}"#), .failure(.unknownCommand("danse")))
        XCTAssertEqual(CommandParser.parse("pas du json"), .failure(.invalidJSON))
        XCTAssertEqual(CommandParser.parse("[1,2,3]"), .failure(.invalidJSON))
        XCTAssertEqual(CommandParser.parse(#"{"commande":"start"}"#), .failure(.missingCmd))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"aside"}"#), .failure(.invalidField(command: "aside", field: "on")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"aside","on":"oui"}"#), .failure(.invalidField(command: "aside", field: "on")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"diarize","audio_dir":"/a"}"#),
                       .failure(.invalidField(command: "diarize", field: "meeting_id")))
        // Message utilisateur en français.
        XCTAssertTrue(CommandParseError.unknownCommand("danse").message.contains("Commande inconnue"))
    }

    // MARK: Horodatage

    func testTimestampFormat() throws {
        let date = try XCTUnwrap(ISO8601DateFormatter().date(from: "2026-11-26T09:41:12Z"))
        let paris = try XCTUnwrap(TimeZone(identifier: "Europe/Paris"))
        XCTAssertEqual(Timestamp.iso8601(date, timeZone: paris), "2026-11-26T10:41:12+01:00")
        let utc = try XCTUnwrap(TimeZone(identifier: "UTC"))
        XCTAssertEqual(Timestamp.iso8601(date, timeZone: utc), "2026-11-26T09:41:12+00:00")
    }
}
