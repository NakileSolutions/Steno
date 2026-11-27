// Capture directe d'un micro par le HAL Core Audio (IOProc sur le périphérique d'entrée),
// sans AVAudioEngine. Contrairement au moteur, elle ne touche jamais à la sortie audio :
// pas d'échec « IsFormatSampleRateAndChannelCountValid(outputHWFormat) » quand la sortie
// change (casque Bluetooth qui bascule en mode appel, écrans, haut-parleurs), et aucune
// perturbation du son de l'app de visio. Utilisée quand le voice processing est indisponible.

import AudioToolbox
import CoreAudio
import Foundation

@available(macOS 14.2, *)
final class HALInputCapture {
    typealias SampleHandler = (_ samples: [Float], _ sampleRate: Double) -> Void

    let device: AudioDeviceID
    private let onSamples: SampleHandler
    private var bridge: RealtimeSampleBridge?
    private var procID: AudioDeviceIOProcID?
    private var sampleRate: Double = 0

    init(device: AudioDeviceID, onSamples: @escaping SampleHandler) {
        self.device = device
        self.onSamples = onSamples
    }

    deinit {
        stop()
    }

    private(set) var channelCount = 0
    var rate: Double { sampleRate }

    func start() throws {
        let format = try Self.inputFormat(of: device)
        guard format.mFormatID == kAudioFormatLinearPCM,
              format.mFormatFlags & kAudioFormatFlagIsFloat != 0,
              format.mBitsPerChannel == 32
        else {
            throw MicError.restartFailed("format du micro non pris en charge (\(format.mFormatID), \(format.mBitsPerChannel) bits)")
        }
        sampleRate = format.mSampleRate
        channelCount = Int(format.mChannelsPerFrame)

        // First buffer only (interleaved: every channel averaged; non-interleaved: channel 0),
        // mixed on the IO thread into a lock-free ring (no allocation, lock or log there) and
        // processed on the bridge's queue (see `RealtimeSampleBridge`).
        guard let bridge = RealtimeSampleBridge(label: "steno.audio.mic.io", firstBuffer: 0, bufferCount: 1) else {
            throw MicError.restartFailed("allocation of the sample ring failed")
        }
        var proc: AudioDeviceIOProcID?
        var status = AudioDeviceCreateIOProcIDWithBlock(&proc, device, nil, bridge.ioBlock)
        guard status == noErr, let proc else {
            throw CoreAudioError(status: status, operation: "création de l'IOProc du micro")
        }
        let tracker = SampleRateTracker(rate: sampleRate, label: "microphone")
        let onSamples = onSamples
        bridge.start { samples, dropped in
            if dropped > 0 { Log.warn("microphone: \(dropped) frames dropped (processing too slow)") }
            let rate = tracker.add(frames: samples.count)
            if !samples.isEmpty { onSamples(samples, rate) }
        }
        status = AudioDeviceStart(device, proc)
        guard status == noErr else {
            AudioDeviceDestroyIOProcID(device, proc)
            bridge.stop()
            throw CoreAudioError(status: status, operation: "démarrage du micro")
        }
        procID = proc
        self.bridge = bridge
    }

    func stop() {
        if let procID {
            AudioDeviceStop(device, procID)
            AudioDeviceDestroyIOProcID(device, procID)
        }
        procID = nil
        // After the IOProc is gone: nothing writes to the ring any more.
        bridge?.stop()
        bridge = nil
    }

    /// Format virtuel du premier flux d'entrée (Float32 en pratique).
    static func inputFormat(of device: AudioDeviceID) throws -> AudioStreamBasicDescription {
        var addr = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyStreams,
                                              mScope: kAudioObjectPropertyScopeInput,
                                              mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        var status = AudioObjectGetPropertyDataSize(device, &addr, 0, nil, &size)
        guard status == noErr, size >= UInt32(MemoryLayout<AudioStreamID>.size) else { throw MicError.noInputDevice }
        var streams = [AudioStreamID](repeating: 0, count: Int(size) / MemoryLayout<AudioStreamID>.size)
        status = AudioObjectGetPropertyData(device, &addr, 0, nil, &size, &streams)
        guard status == noErr, let first = streams.first else { throw MicError.noInputDevice }

        var fmtAddr = AudioObjectPropertyAddress(mSelector: kAudioStreamPropertyVirtualFormat,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var format = AudioStreamBasicDescription()
        var fmtSize = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        status = AudioObjectGetPropertyData(first, &fmtAddr, 0, nil, &fmtSize, &format)
        guard status == noErr, format.mSampleRate > 0, format.mChannelsPerFrame > 0 else {
            throw MicError.noInputDevice
        }
        return format
    }

}
