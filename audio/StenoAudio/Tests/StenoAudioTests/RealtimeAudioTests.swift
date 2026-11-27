import AVFoundation
import CoreAudio
import StenoAudioRT
import XCTest
@testable import StenoAudioKit

final class SampleRingTests: XCTestCase {
    /// AudioBufferList over `buffers` (each with its channel count), kept alive by the caller.
    private func withBufferList<R>(_ buffers: [(channels: UInt32, samples: [Float])],
                                   _ body: (UnsafePointer<AudioBufferList>) -> R) -> R
    {
        let list = AudioBufferList.allocate(maximumBuffers: buffers.count)
        defer { free(list.unsafeMutablePointer) }
        var storage = buffers.map { $0.samples }
        let pointers = storage.indices.map { i -> UnsafeMutablePointer<Float> in
            let p = UnsafeMutablePointer<Float>.allocate(capacity: storage[i].count)
            p.initialize(from: &storage[i], count: storage[i].count)
            return p
        }
        defer { pointers.forEach { $0.deallocate() } }
        for (i, b) in buffers.enumerated() {
            list[i] = AudioBuffer(mNumberChannels: b.channels,
                                  mDataByteSize: UInt32(b.samples.count * MemoryLayout<Float>.size),
                                  mData: UnsafeMutableRawPointer(pointers[i]))
        }
        return body(UnsafePointer(list.unsafePointer))
    }

    private func readAll(_ ring: OpaquePointer) -> [Float] {
        var out = [Float](repeating: 0, count: Int(steno_ring_available(ring)))
        let n = out.withUnsafeMutableBufferPointer { steno_ring_read(ring, $0.baseAddress!, UInt32($0.count)) }
        return Array(out.prefix(Int(n)))
    }

    func testWriteReadAndWrapAround() throws {
        let ring = try XCTUnwrap(steno_ring_create(1024))
        defer { steno_ring_destroy(ring) }
        var expected: [Float] = []
        // Several laps around the 1024-sample buffer.
        for lap in 0..<5 {
            let block = (0..<700).map { Float(lap * 1000 + $0) }
            XCTAssertEqual(block.withUnsafeBufferPointer { steno_ring_write(ring, $0.baseAddress!, 700) }, 700)
            expected = block
            XCTAssertEqual(readAll(ring), expected)
        }
        XCTAssertEqual(steno_ring_take_dropped(ring), 0)
    }

    func testOverflowDropsAndCounts() throws {
        let ring = try XCTUnwrap(steno_ring_create(1024))
        defer { steno_ring_destroy(ring) }
        let block = [Float](repeating: 1, count: 1500)
        XCTAssertEqual(block.withUnsafeBufferPointer { steno_ring_write(ring, $0.baseAddress!, 1500) }, 1024)
        XCTAssertEqual(steno_ring_take_dropped(ring), 476)
        XCTAssertEqual(steno_ring_take_dropped(ring), 0)
        XCTAssertEqual(readAll(ring).count, 1024)
    }

    func testMonoMixInterleaved() throws {
        let ring = try XCTUnwrap(steno_ring_create(1024))
        defer { steno_ring_destroy(ring) }
        // Interleaved stereo: L = 1, R = 0 → 0.5.
        let n = withBufferList([(2, [1, 0, 1, 0, 1, 0])]) { steno_ring_write_mono(ring, $0, 0, 1) }
        XCTAssertEqual(n, 3)
        XCTAssertEqual(readAll(ring), [0.5, 0.5, 0.5])
    }

    func testMonoMixNonInterleavedUsesTrailingBuffers() throws {
        let ring = try XCTUnwrap(steno_ring_create(1024))
        defer { steno_ring_destroy(ring) }
        // Aggregate input: a sub-device microphone buffer (9s) first, then the tap's two
        // non-interleaved channels (1 and 0): only the last two buffers are mixed.
        let n = withBufferList([(1, [9, 9]), (1, [1, 1]), (1, [0, 0])]) { steno_ring_write_mono(ring, $0, -2, 2) }
        XCTAssertEqual(n, 2)
        XCTAssertEqual(readAll(ring), [0.5, 0.5])
    }

    func testMonoMixFirstBufferOnly() throws {
        let ring = try XCTUnwrap(steno_ring_create(1024))
        defer { steno_ring_destroy(ring) }
        let n = withBufferList([(1, [0.25, 0.75]), (1, [1, 1])]) { steno_ring_write_mono(ring, $0, 0, 1) }
        XCTAssertEqual(n, 2)
        XCTAssertEqual(readAll(ring), [0.25, 0.75])
    }

    func testBridgeDeliversWhatTheIOBlockWrote() throws {
        let bridge = try XCTUnwrap(RealtimeSampleBridge(label: "test.bridge", firstBuffer: 0, bufferCount: 1))
        let received = SampleBuffer()
        let delivered = expectation(description: "samples delivered")
        delivered.assertForOverFulfill = false
        bridge.start { samples, _ in
            received.append(samples)
            if received.samples.count >= 4 { delivered.fulfill() }
        }
        var timestamp = AudioTimeStamp()
        let output = AudioBufferList.allocate(maximumBuffers: 1)
        defer { free(output.unsafeMutablePointer) }
        withBufferList([(1, [0.1, 0.2, 0.3, 0.4])]) { input in
            bridge.ioBlock(&timestamp, input, &timestamp, output.unsafeMutablePointer, &timestamp)
        }
        wait(for: [delivered], timeout: 2)
        bridge.stop()
        XCTAssertEqual(received.samples, [0.1, 0.2, 0.3, 0.4])
    }
}

final class SampleRateTrackerTests: XCTestCase {
    func testSnapsToMeasuredRate() {
        // Announced 48 kHz, 16 kHz actually delivered (Bluetooth hands-free profile).
        let tracker = SampleRateTracker(rate: 48_000, label: "test")
        tracker.add(frames: 0, now: 100)
        XCTAssertEqual(tracker.add(frames: 16_000 * 3, now: 103), 16_000)
    }

    func testKeepsRateWithinTolerance() {
        let tracker = SampleRateTracker(rate: 48_000, label: "test")
        tracker.add(frames: 0, now: 0)
        XCTAssertEqual(tracker.add(frames: 47_000 * 3, now: 3), 48_000)
    }

    func testStandardRate() {
        XCTAssertEqual(SampleRateTracker.standardRate(near: 23_900), 24_000)
        XCTAssertNil(SampleRateTracker.standardRate(near: 60_000))
    }
}

final class ObjCExceptionTests: XCTestCase {
    func testObjectiveCExceptionBecomesSwiftError() {
        XCTAssertThrowsError(try ObjCException.catching {
            NSException(name: .invalidArgumentException, reason: "format mismatch", userInfo: nil).raise()
        }) { error in
            XCTAssertTrue("\(error.localizedDescription)".contains("format mismatch"))
        }
    }

    func testValuesAndSwiftErrorsPassThrough() throws {
        XCTAssertEqual(try ObjCException.catching { 42 }, 42)
        struct Boom: Error {}
        XCTAssertThrowsError(try ObjCException.catching { throw Boom() }) { XCTAssertTrue($0 is Boom) }
    }
}

final class WavWriterCrashSafetyTests: XCTestCase {
    func testHeaderIsReadableBeforeClose() throws {
        // A sidecar killed before `close` (SIGKILL) must still leave a readable file.
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("steno-audio-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("me.wav")
        let writer = try WavWriter(url: url)
        let refresh = Int(WavWriter.headerRefreshSeconds) * 16_000
        try writer.append([Float](repeating: 0.25, count: refresh))
        try writer.append([Float](repeating: 0.25, count: 800))

        let file = try AVAudioFile(forReading: url)
        XCTAssertEqual(file.length, AVAudioFramePosition(refresh))
        try writer.close()
        XCTAssertEqual(try AVAudioFile(forReading: url).length, AVAudioFramePosition(refresh + 800))
    }

    func testLongGapsAreFilledCompletely() throws {
        // The segmenter clock advances by the whole gap: the WAV must stay aligned with it.
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("steno-audio-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let writer = try WavWriter(url: dir.appendingPathComponent("them.wav"), sampleRate: 100)
        try writer.appendSilence(100 * 3600)
        try writer.close()
        XCTAssertEqual(writer.durationSeconds, 3600, accuracy: 0.001)
    }
}
