import AVFoundation
import XCTest
@testable import StenoAudioKit

final class TeamsTitleTests: XCTestCase {
    func testPicksMeetingWindowOverMainWindow() {
        let windows = [
            WindowCandidate(title: "Chat | Microsoft Teams", isMain: true),
            WindowCandidate(title: "Daily ORION | Microsoft Teams", isMain: false),
        ]
        XCTAssertEqual(TeamsTitleHeuristics.pickMeetingTitle(windows), "Daily ORION")
    }

    func testGenericTitlesGiveNil() {
        let windows = [
            WindowCandidate(title: "Microsoft Teams", isMain: true),
            WindowCandidate(title: "Calendrier | Microsoft Teams", isMain: false),
            WindowCandidate(title: "", isMain: false),
        ]
        XCTAssertNil(TeamsTitleHeuristics.pickMeetingTitle(windows))
    }

    func testMeetingHintsWin() {
        let windows = [
            WindowCandidate(title: "Marie Dupont | Microsoft Teams", isMain: false),
            WindowCandidate(title: "Réunion de planning sprint 42 | Microsoft Teams", isMain: false),
        ]
        XCTAssertEqual(TeamsTitleHeuristics.pickMeetingTitle(windows), "Réunion de planning sprint 42")
    }

    func testDashSuffixIsStripped() {
        XCTAssertEqual(TeamsTitleHeuristics.clean("Point projet – Microsoft Teams"), "Point projet")
        XCTAssertEqual(TeamsTitleHeuristics.clean("Point projet | Microsoft Teams (travail ou scolaire)"), "Point projet")
    }

    func testTeamsBundleMatching() {
        let teams = WatchedApp.defaults[0]
        XCTAssertTrue(teams.matches(bundleID: "com.microsoft.teams2"))
        XCTAssertTrue(teams.matches(bundleID: "com.microsoft.teams2.helper"))
        XCTAssertTrue(teams.matches(bundleID: "com.microsoft.teams"))
        XCTAssertFalse(teams.matches(bundleID: "com.microsoft.edgemac"))
        XCTAssertFalse(teams.matches(bundleID: "com.apple.WebKit.GPU"))
    }
}

final class DiarizationMapperTests: XCTestCase {
    func testLabelsByFirstAppearanceAndMerge() {
        let raw = [
            RawSpeakerTurn(speakerId: "speaker_7", start: 20, end: 25),
            RawSpeakerTurn(speakerId: "speaker_3", start: 0, end: 10),
            RawSpeakerTurn(speakerId: "speaker_3", start: 10.2, end: 15),
            RawSpeakerTurn(speakerId: "speaker_7", start: 30, end: 30), // vide : ignoré
            RawSpeakerTurn(speakerId: "speaker_3", start: 26.123_4, end: 28.987_6),
        ]
        let turns = DiarizationMapper.turns(from: raw)
        XCTAssertEqual(turns, [
            DiarizationTurn(t0: 0, t1: 15, speaker: "S1"),
            DiarizationTurn(t0: 20, t1: 25, speaker: "S2"),
            DiarizationTurn(t0: 26.12, t1: 28.99, speaker: "S1"),
        ])
    }
}

final class AudioUtilitiesTests: XCTestCase {
    private func sine(seconds: Double, rate: Double, amplitude: Float = 0.5, frequency: Float = 440) -> [Float] {
        let count = Int(seconds * rate)
        let step = 2 * Float.pi * frequency / Float(rate)
        return (0..<count).map { amplitude * sin(step * Float($0)) }
    }

    func testLevels() {
        XCTAssertEqual(AudioLevel.dbfs([Float](repeating: 0, count: 100)), AudioLevel.floorDb)
        XCTAssertEqual(AudioLevel.dbfs(sine(seconds: 0.1, rate: 16_000, amplitude: 1)), -3.01, accuracy: 0.05)

        let meter = LevelMeter()
        meter.add(sine(seconds: 0.25, rate: 16_000, amplitude: 1), channel: .them)
        let levels = meter.take()
        XCTAssertEqual(levels.me, AudioLevel.floorDb)
        XCTAssertEqual(levels.them, -3.01, accuracy: 0.05)
        XCTAssertEqual(meter.take().them, AudioLevel.floorDb, "les compteurs sont remis à zéro")
    }

    func testResampler48kTo16k() {
        let input = sine(seconds: 1, rate: 48_000)
        let output = Resampler.resampleAll(input, from: 48_000)
        XCTAssertEqual(Double(output.count), 16_000, accuracy: 64)
        // L'énergie est conservée (440 Hz est loin de la fréquence de coupure).
        XCTAssertEqual(AudioLevel.dbfs(Array(output[1000..<15000])), AudioLevel.dbfs(input), accuracy: 0.5)
    }

    func testResamplerStreamingMatchesLength() {
        let r = Resampler()
        var total = 0
        for _ in 0..<10 {
            total += r.process(sine(seconds: 0.1, rate: 44_100), sampleRate: 44_100).count
        }
        total += r.flush().count
        XCTAssertEqual(Double(total), 16_000, accuracy: 64)
    }

    func testMonoMixdownOfStereoBuffer() throws {
        let format = try XCTUnwrap(AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 2))
        let buffer = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 4))
        buffer.frameLength = 4
        for f in 0..<4 {
            buffer.floatChannelData![0][f] = 1
            buffer.floatChannelData![1][f] = 0
        }
        XCTAssertEqual(AudioBufferUtils.monoSamples(from: buffer, mix: .average), [0.5, 0.5, 0.5, 0.5])
        XCTAssertEqual(AudioBufferUtils.monoSamples(from: buffer, mix: .firstChannel), [1, 1, 1, 1])
    }

    func testWavWriterRoundTrip() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("steno-audio-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("them.wav")

        let writer = try WavWriter(url: url)
        try writer.append(sine(seconds: 0.5, rate: 16_000))
        try writer.appendSilence(8_000)
        try writer.close()
        XCTAssertEqual(writer.durationSeconds, 1.0, accuracy: 0.001)

        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
        XCTAssertEqual((attributes[.size] as? NSNumber)?.intValue, 44 + 16_000 * 2)

        let file = try AVAudioFile(forReading: url)
        XCTAssertEqual(file.fileFormat.sampleRate, 16_000)
        XCTAssertEqual(file.fileFormat.channelCount, 1)
        XCTAssertEqual(file.length, 16_000)
    }
}
