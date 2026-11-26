import Foundation
import XCTest
@testable import ScribeAudioKit

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

    func testTeamsCallWithTitle() throws {
        let json = try encode(.teamsCall(active: true, title: "Daily ORION"))
        XCTAssertEqual(json["type"] as? String, "teams_call")
        XCTAssertEqual(json["active"] as? Bool, true)
        XCTAssertEqual(json["title"] as? String, "Daily ORION")
    }

    func testTeamsCallWithoutTitleEncodesExplicitNull() throws {
        let json = try encode(.teamsCall(active: false, title: nil))
        XCTAssertEqual(json["active"] as? Bool, false)
        XCTAssertTrue(json.keys.contains("title"), "title doit être présent")
        XCTAssertTrue(json["title"] is NSNull, "title doit valoir null")
    }

    func testSegment() throws {
        let payload = SegmentPayload(ts: "2026-11-26T10:41:12+01:00", t0: 668.2, t1: 672.912_345,
                                     channel: .me, text: "Scribe, cherche les erreurs sur ms-booking…", isPrivate: false)
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
        XCTAssertEqual(json["text"] as? String, "Scribe, cherche les erreurs sur ms-booking…")
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

        let wake = try encode(.wakeTest(text: "scribe cherche quelque chose"))
        XCTAssertEqual(wake["type"] as? String, "wake_test")
        XCTAssertEqual(wake["text"] as? String, "scribe cherche quelque chose")
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
            .badCommand: "bad_command",
        ]
        for (code, raw) in expected { XCTAssertEqual(code.rawValue, raw) }
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
        let line = #"{"cmd":"start","vocabulary":["Scribe","Atlas"],"silence_ms":1000,"keep_audio":true,"audio_dir":"/tmp/Scribe/m1"}"#
        guard case let .success(.start(opts)) = CommandParser.parse(line) else { return XCTFail() }
        XCTAssertEqual(opts.vocabulary, ["Scribe", "Atlas"])
        XCTAssertEqual(opts.silenceMs, 1000)
        XCTAssertTrue(opts.keepAudio)
        XCTAssertEqual(opts.audioDir, "/tmp/Scribe/m1")
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
