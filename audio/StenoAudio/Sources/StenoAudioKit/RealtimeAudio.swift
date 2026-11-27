// Real-time audio plumbing shared by the process tap and the direct microphone capture.
//
// `AudioDeviceCreateIOProcIDWithBlock` dispatches its block *synchronously* from the HAL IO
// thread (AudioHardware.h: "All IOBlocks are dispatched synchronously"), even when a dispatch
// queue is given: whatever the block does (allocations, locks, `AsyncStream` yields, stderr
// logs) delays the IO cycle of the device, which for the tap is an aggregate containing the
// system output. So the IOProc only mixes the buffers down to mono into a pre-allocated
// lock-free ring (`steno_ring_write_mono`, C), and a timer on a regular queue drains the ring,
// checks the sample rate, resamples and feeds the pipeline.

import AudioToolbox
import CoreAudio
import Foundation
import StenoAudioRT

// MARK: - Objective-C exceptions

enum ObjCException {
    /// Runs `body`; an Objective-C exception it raises becomes a Swift error instead of aborting
    /// the process. Keep `body` short: the exception unwinds through its Swift frames.
    @discardableResult
    static func catching<T>(_ body: () throws -> T) throws -> T {
        var result: Result<T, Error>?
        try StenoObjCExceptionCatcher.catching {
            result = Result { try body() }
        }
        guard let result else {
            throw CoreAudioError(status: OSStatus(kAudioHardwareUnspecifiedError), operation: "Objective-C block")
        }
        return try result.get()
    }
}

// MARK: - Sample rate tracking

/// The rate actually delivered can differ from the announced one: a Bluetooth headset switching
/// to its hands-free profile changes rate without notice. Treating 16 kHz audio as 48 kHz speeds
/// voices up ×3 (absurd transcription, silent VAD). Compares the received throughput (over ~3 s)
/// with the assumed rate and snaps to the nearest standard rate when they clearly differ.
/// Not thread-safe: use it from a single queue.
final class SampleRateTracker {
    private(set) var rate: Double
    private let label: String
    private var windowStart: TimeInterval?
    private var windowFrames = 0

    init(rate: Double, label: String) {
        self.rate = rate
        self.label = label
    }

    /// `frames` received at `now` (monotonic seconds). Returns the rate to use.
    @discardableResult
    func add(frames: Int, now: TimeInterval = ProcessInfo.processInfo.systemUptime) -> Double {
        guard let start = windowStart else {
            windowStart = now
            windowFrames = 0
            return rate
        }
        windowFrames += frames
        let elapsed = now - start
        guard elapsed >= 3 else { return rate }
        let measured = Double(windowFrames) / elapsed
        windowStart = now
        windowFrames = 0
        guard rate > 0, abs(measured - rate) / rate > 0.15,
              let snapped = Self.standardRate(near: measured)
        else { return rate }
        Log.warn("\(label): \(Int(measured)) samples/s received for \(Int(rate)) Hz announced, switching to \(Int(snapped)) Hz")
        rate = snapped
        return rate
    }

    /// Nearest standard rate (within 10 %), otherwise nil.
    static func standardRate(near measured: Double) -> Double? {
        let rates: [Double] = [8000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 88200, 96000]
        guard let best = rates.min(by: { abs($0 - measured) < abs($1 - measured) }),
              abs(best - measured) / best < 0.10
        else { return nil }
        return best
    }
}

// MARK: - IO thread → processing queue

/// Carries mono samples from an IOProc to a regular queue without allocating, locking or
/// logging on the IO thread.
///
/// Usage: register `ioBlock` with `AudioDeviceCreateIOProcIDWithBlock(…, nil, ioBlock)` (no
/// queue: the block is real-time safe and runs directly on the IO thread), `start` the drain,
/// and call `stop` after `AudioDeviceStop` / `AudioDeviceDestroyIOProcID`.
final class RealtimeSampleBridge {
    /// Delivered on the bridge's queue: mono Float32 samples and the number of frames dropped
    /// because the ring was full since the previous call.
    typealias Handler = (_ samples: [Float], _ dropped: UInt64) -> Void

    /// Drain period. 20 ms keeps the added latency negligible compared with the VAD frames.
    static let drainInterval: TimeInterval = 0.02

    let ioBlock: AudioDeviceIOBlock

    private let ring: OpaquePointer
    private let queue: DispatchQueue
    private var timer: DispatchSourceTimer?
    private var handler: Handler?

    /// The IOProc averages every channel of `bufferCount` buffers starting at `firstBuffer`
    /// (negative: counted from the end of the buffer list). `capacitySeconds` at `maxRate` must
    /// cover the longest stall of the drain queue.
    init?(label: String, firstBuffer: Int32, bufferCount: UInt32,
          capacitySeconds: Double = 4, maxRate: Double = 96_000)
    {
        guard let ring = steno_ring_create(UInt32(capacitySeconds * maxRate)) else { return nil }
        self.ring = ring
        self.queue = DispatchQueue(label: label, qos: .userInteractive)
        // Captures only the raw ring pointer: no retain/release, no allocation per cycle.
        self.ioBlock = { _, inputData, _, _, _ in
            _ = steno_ring_write_mono(ring, inputData, firstBuffer, bufferCount)
        }
    }

    deinit {
        stop()
        steno_ring_destroy(ring)
    }

    /// Starts draining. `handler` must not reference the bridge's owner strongly enough to
    /// release it from the bridge queue (`stop` syncs on that queue).
    func start(handler: @escaping Handler) {
        queue.sync {
            guard timer == nil else { return }
            let ring = self.ring
            let source = DispatchSource.makeTimerSource(queue: queue)
            source.schedule(deadline: .now() + Self.drainInterval, repeating: Self.drainInterval,
                            leeway: .milliseconds(5))
            // The handler does not capture `self`: the bridge is never released from its queue.
            source.setEventHandler { Self.drain(ring, into: handler) }
            source.resume()
            timer = source
            self.handler = handler
        }
    }

    /// Stops draining after delivering what is left in the ring. Must not be called from the
    /// bridge's handler.
    func stop() {
        queue.sync {
            guard let timer else { return }
            timer.cancel()
            self.timer = nil
            // Last samples written before the IOProc stopped.
            if let handler { Self.drain(ring, into: handler) }
            handler = nil
        }
    }

    private static func drain(_ ring: OpaquePointer, into handler: Handler) {
        let available = Int(steno_ring_available(ring))
        let dropped = steno_ring_take_dropped(ring)
        guard available > 0 || dropped > 0 else { return }
        var samples = [Float](repeating: 0, count: available)
        let read = samples.withUnsafeMutableBufferPointer { buffer -> Int in
            guard let base = buffer.baseAddress else { return 0 }
            return Int(steno_ring_read(ring, base, UInt32(available)))
        }
        if read < available { samples.removeLast(available - read) }
        handler(samples, dropped)
    }
}
