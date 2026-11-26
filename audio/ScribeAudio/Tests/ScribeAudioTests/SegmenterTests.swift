import XCTest
@testable import ScribeAudioKit

/// Segmenteur + VAD énergie sur des signaux synthétiques (aucun matériel, aucun modèle).
final class SegmenterTests: XCTestCase {
    private let rate = 16_000

    /// Bruit faible déterministe (≈ -65 dBFS) ou sinusoïde 440 Hz d'amplitude 0,3 (≈ -13,5 dBFS).
    private func signal(_ parts: [(seconds: Double, tone: Bool)]) -> [Float] {
        var out: [Float] = []
        var seed: UInt32 = 12_345
        var n = 0
        for part in parts {
            let count = Int(part.seconds * Double(rate))
            for _ in 0..<count {
                seed = seed &* 1_103_515_245 &+ 12_345
                let noise = (Float(seed >> 16 & 0x7FFF) / 32_767 * 2 - 1) * 0.001
                if part.tone {
                    let phase: Float = 2 * Float.pi * 440 * Float(n) / Float(rate)
                    out.append(0.3 * sin(phase) + noise)
                } else {
                    out.append(noise)
                }
                n += 1
            }
        }
        return out
    }

    private func run(_ samples: [Float], config: SegmenterConfig = SegmenterConfig())
        -> (pushed: [Utterance], flushed: Utterance?)
    {
        let vad = EnergyVAD()
        var segmenter = UtteranceSegmenter(config: config)
        var pushed: [Utterance] = []
        var i = 0
        while i < samples.count {
            var frame = Array(samples[i..<min(samples.count, i + vad.frameSize)])
            if frame.count < vad.frameSize { frame += [Float](repeating: 0, count: vad.frameSize - frame.count) }
            pushed += segmenter.push(frame, probability: vad.probability(for: frame))
            i += vad.frameSize
        }
        return (pushed, segmenter.flush())
    }

    func testEnergyVADSeparatesToneFromNoise() {
        let vad = EnergyVAD()
        let noise = signal([(1, false)])
        let tone = signal([(0.1, true)])
        for k in 0..<10 {
            XCTAssertEqual(vad.probability(for: Array(noise[(k * 512)..<((k + 1) * 512)])), 0)
        }
        XCTAssertEqual(vad.probability(for: Array(tone[0..<512])), 1)
    }

    func testSingleUtteranceClosedAfterSilence() {
        let samples = signal([(1.0, false), (2.0, true), (1.5, false)])
        let result = run(samples)
        XCTAssertEqual(result.pushed.count, 1, "l'énoncé est clos par 1 s de silence, avant la fin du signal")
        XCTAssertNil(result.flushed)
        let u = result.pushed[0]
        XCTAssertEqual(u.t0, 0.8, accuracy: 0.1, "début ≈ 1,0 s moins 0,2 s de pré-roll")
        XCTAssertEqual(u.t1, 3.15, accuracy: 0.1, "fin ≈ 3,0 s plus 0,15 s de post-roll")
        XCTAssertEqual(u.samples.count, u.endSample - u.startSample)
    }

    func testUtteranceStillOpenIsReturnedByFlush() {
        let samples = signal([(0.5, false), (1.0, true), (0.3, false)])
        let result = run(samples)
        XCTAssertTrue(result.pushed.isEmpty)
        let u = try? XCTUnwrap(result.flushed)
        XCTAssertNotNil(u)
        XCTAssertEqual(u?.t0 ?? 0, 0.3, accuracy: 0.1)
    }

    func testSilenceMsControlsMerging() {
        // Deux phrases séparées par 0,5 s de silence.
        let samples = signal([(0.5, false), (1.0, true), (0.5, false), (1.0, true), (1.5, false)])

        var long = SegmenterConfig()
        long.silenceMs = 1000
        XCTAssertEqual(run(samples, config: long).pushed.count, 1, "silence_ms = 1000 : une seule phrase")

        var short = SegmenterConfig()
        short.silenceMs = 300
        XCTAssertEqual(run(samples, config: short).pushed.count, 2, "silence_ms = 300 : deux phrases")
    }

    func testLongSpeechIsCutAt25Seconds() {
        let samples = signal([(0.5, false), (30, true), (1.5, false)])
        let result = run(samples)
        XCTAssertEqual(result.pushed.count, 2)
        let first = result.pushed[0]
        XCTAssertGreaterThan(first.duration, 22.9)
        XCTAssertLessThanOrEqual(first.duration, 25.05)
        // Les morceaux sont contigus : rien n'est perdu à la coupe.
        XCTAssertEqual(result.pushed[1].startSample, first.endSample)
        XCTAssertEqual(result.pushed[1].t1, 30.65, accuracy: 0.1)
    }

    func testShortClickIsDiscarded() {
        let samples = signal([(1.0, false), (0.05, true), (1.5, false)])
        let result = run(samples)
        XCTAssertTrue(result.pushed.isEmpty)
        XCTAssertNil(result.flushed)
    }

    func testSkipAdvancesClockAndClosesUtterance() {
        let vad = EnergyVAD()
        var segmenter = UtteranceSegmenter()
        let tone = signal([(1.0, true)])
        var i = 0
        while i + 512 <= tone.count {
            let frame = Array(tone[i..<(i + 512)])
            XCTAssertTrue(segmenter.push(frame, probability: vad.probability(for: frame)).isEmpty)
            i += 512
        }
        let closed = segmenter.skip(samples: 2 * rate)
        XCTAssertNotNil(closed)
        XCTAssertEqual(segmenter.position, i + 2 * rate)
        XCTAssertFalse(segmenter.isInUtterance)

        // Un nouvel énoncé après le trou est daté après le trou.
        for _ in 0..<40 {
            let frame = Array(tone[0..<512])
            _ = segmenter.push(frame, probability: 1)
        }
        let next = segmenter.flush()
        XCTAssertEqual(next?.t0 ?? 0, Double(i + 2 * rate) / Double(rate), accuracy: 0.001)
    }
}
