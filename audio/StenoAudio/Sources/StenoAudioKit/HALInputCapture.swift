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
    private let ioQueue = DispatchQueue(label: "steno.audio.mic.io", qos: .userInteractive)
    private var procID: AudioDeviceIOProcID?
    // Manipulés sur `ioQueue` après le démarrage.
    private var sampleRate: Double = 0
    private var windowStart: TimeInterval?
    private var windowFrames = 0

    init(device: AudioDeviceID, onSamples: @escaping SampleHandler) {
        self.device = device
        self.onSamples = onSamples
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

        var proc: AudioDeviceIOProcID?
        var status = AudioDeviceCreateIOProcIDWithBlock(&proc, device, ioQueue) { [weak self] _, inputData, _, _, _ in
            self?.handle(inputData)
        }
        guard status == noErr, let proc else {
            throw CoreAudioError(status: status, operation: "création de l'IOProc du micro")
        }
        procID = proc
        status = AudioDeviceStart(device, proc)
        guard status == noErr else {
            AudioDeviceDestroyIOProcID(device, proc)
            procID = nil
            throw CoreAudioError(status: status, operation: "démarrage du micro")
        }
    }

    func stop() {
        guard let procID else { return }
        AudioDeviceStop(device, procID)
        AudioDeviceDestroyIOProcID(device, procID)
        self.procID = nil
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

    private func handle(_ inputData: UnsafePointer<AudioBufferList>) {
        let list = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: inputData))
        guard let buffer = list.first, let data = buffer.mData else { return }
        // Entrelacé : un tampon à N canaux ; non entrelacé : un tampon par canal (on prend le 1er).
        let channels = max(1, Int(buffer.mNumberChannels))
        let frames = Int(buffer.mDataByteSize) / MemoryLayout<Float>.size / channels
        guard frames > 0 else { return }
        let p = data.assumingMemoryBound(to: Float.self)
        var mono = [Float](repeating: 0, count: frames)
        if channels == 1 {
            for i in 0..<frames { mono[i] = p[i] }
        } else {
            let scale = 1 / Float(channels)
            for f in 0..<frames {
                var s: Float = 0
                for c in 0..<channels { s += p[f * channels + c] }
                mono[f] = s * scale
            }
        }
        checkRate(frames: frames)
        onSamples(mono, sampleRate)
    }

    /// Garde-fou : un casque Bluetooth qui bascule de profil change de fréquence sans prévenir.
    private func checkRate(frames: Int) {
        let now = ProcessInfo.processInfo.systemUptime
        guard let start = windowStart else {
            windowStart = now
            windowFrames = 0
            return
        }
        windowFrames += frames
        let elapsed = now - start
        guard elapsed >= 3 else { return }
        let measured = Double(windowFrames) / elapsed
        windowStart = now
        windowFrames = 0
        guard sampleRate > 0, abs(measured - sampleRate) / sampleRate > 0.15,
              let snapped = ProcessTap.standardRate(near: measured)
        else { return }
        Log.warn("micro : \(Int(measured)) échantillons/s reçus pour \(Int(sampleRate)) Hz annoncés : passage à \(Int(snapped)) Hz")
        sampleRate = snapped
    }
}
