// Conversion vers le format de travail : 16 kHz, mono, Float32.

import AVFoundation
import Foundation

enum AudioFormat {
    static let sampleRate: Double = 16_000
    static let sampleRateInt = 16_000
}

// MARK: - Mixage mono

enum MonoMix {
    /// Premier canal seulement (micro avec voice processing : le canal 0 est la voix traitée).
    case firstChannel
    /// Moyenne de tous les canaux (tap stéréo).
    case average
}

enum AudioBufferUtils {
    /// Extrait un signal mono Float32 d'un buffer PCM (entrelacé ou non, Float32 ou Int16).
    static func monoSamples(from buffer: AVAudioPCMBuffer, mix: MonoMix) -> [Float] {
        let frames = Int(buffer.frameLength)
        let channels = Int(buffer.format.channelCount)
        guard frames > 0, channels > 0 else { return [] }
        let interleaved = buffer.format.isInterleaved
        let stride = buffer.stride // 1 si non entrelacé, `channels` sinon
        let useAll = mix == .average && channels > 1

        if let data = buffer.floatChannelData {
            var out = [Float](repeating: 0, count: frames)
            if interleaved {
                let p = data[0]
                if useAll {
                    let scale = 1 / Float(channels)
                    for f in 0..<frames {
                        var s: Float = 0
                        for c in 0..<channels { s += p[f * stride + c] }
                        out[f] = s * scale
                    }
                } else {
                    for f in 0..<frames { out[f] = p[f * stride] }
                }
            } else {
                if useAll {
                    let scale = 1 / Float(channels)
                    for c in 0..<channels {
                        let p = data[c]
                        for f in 0..<frames { out[f] += p[f] }
                    }
                    for f in 0..<frames { out[f] *= scale }
                } else {
                    let p = data[0]
                    for f in 0..<frames { out[f] = p[f] }
                }
            }
            return out
        }

        if let data = buffer.int16ChannelData {
            var out = [Float](repeating: 0, count: frames)
            let norm: Float = 1 / 32768
            let used = useAll ? channels : 1
            for c in 0..<used {
                let p = interleaved ? data[0] : data[c]
                for f in 0..<frames {
                    let idx = interleaved ? f * stride + c : f
                    out[f] += Float(p[idx]) * norm
                }
            }
            if used > 1 {
                let scale = 1 / Float(used)
                for f in 0..<frames { out[f] *= scale }
            }
            return out
        }

        Log.debug("format PCM non géré : \(buffer.format)")
        return []
    }
}

// MARK: - Rééchantillonnage

/// Rééchantillonneur en flux continu (le filtre garde son état entre deux appels).
/// Sûr entre threads : un verrou protège le convertisseur, car une source audio peut être
/// recréée (tap, moteur micro) pendant qu'un ancien callback se termine.
final class Resampler {
    let targetRate: Double

    private let lock = NSLock()
    private let outputFormat: AVAudioFormat
    private var converter: AVAudioConverter?
    private var inputFormat: AVAudioFormat?

    init(targetRate: Double = AudioFormat.sampleRate) {
        self.targetRate = targetRate
        // Format standard (Float32 non entrelacé) : l'initialiseur ne peut pas échouer ici.
        outputFormat = AVAudioFormat(standardFormatWithSampleRate: targetRate, channels: 1)!
    }

    /// Convertit un bloc mono Float32 échantillonné à `sampleRate` vers `targetRate`.
    func process(_ samples: [Float], sampleRate: Double) -> [Float] {
        guard !samples.isEmpty, sampleRate > 0 else { return [] }
        if abs(sampleRate - targetRate) < 0.5 { return samples }

        lock.lock()
        defer { lock.unlock() }
        guard let converter = converterFor(sampleRate: sampleRate),
              let inputFormat,
              let input = Self.makeBuffer(samples, format: inputFormat)
        else { return [] }

        let capacity = AVAudioFrameCount(Double(samples.count) * targetRate / sampleRate) + 64
        guard let out = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: capacity) else { return [] }

        var provided = false
        var error: NSError?
        let status = converter.convert(to: out, error: &error) { _, inputStatus in
            if provided {
                inputStatus.pointee = .noDataNow
                return nil
            }
            provided = true
            inputStatus.pointee = .haveData
            return input
        }
        if status == .error {
            Log.warn("rééchantillonnage impossible : \(error?.localizedDescription ?? "erreur inconnue")")
            self.converter = nil
            return []
        }
        return Self.samples(of: out)
    }

    /// Vide le filtre (fin de flux) et réinitialise le convertisseur.
    func flush() -> [Float] {
        lock.lock()
        defer { lock.unlock() }
        guard let converter else { return [] }
        var result: [Float] = []
        while true {
            guard let out = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: 4096) else { break }
            var error: NSError?
            let status = converter.convert(to: out, error: &error) { _, inputStatus in
                inputStatus.pointee = .endOfStream
                return nil
            }
            result += Self.samples(of: out)
            if status != .haveData || out.frameLength == 0 { break }
        }
        converter.reset()
        return result
    }

    /// Conversion d'un signal complet (fichiers, spikes).
    static func resampleAll(_ samples: [Float], from sampleRate: Double,
                            to targetRate: Double = AudioFormat.sampleRate) -> [Float] {
        if abs(sampleRate - targetRate) < 0.5 { return samples }
        let r = Resampler(targetRate: targetRate)
        var out: [Float] = []
        out.reserveCapacity(Int(Double(samples.count) * targetRate / sampleRate) + 1024)
        // Par blocs d'une seconde pour limiter la taille des buffers intermédiaires.
        let block = max(1, Int(sampleRate))
        var i = 0
        while i < samples.count {
            let end = min(samples.count, i + block)
            out += r.process(Array(samples[i..<end]), sampleRate: sampleRate)
            i = end
        }
        out += r.flush()
        return out
    }

    // MARK: Interne

    private func converterFor(sampleRate: Double) -> AVAudioConverter? {
        if let converter, let inputFormat, abs(inputFormat.sampleRate - sampleRate) < 0.5 {
            return converter
        }
        guard let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 1),
              let conv = AVAudioConverter(from: format, to: outputFormat)
        else {
            Log.warn("convertisseur \(sampleRate) Hz → \(targetRate) Hz indisponible")
            return nil
        }
        conv.sampleRateConverterQuality = AVAudioQuality.high.rawValue
        inputFormat = format
        converter = conv
        return conv
    }

    private static func makeBuffer(_ samples: [Float], format: AVAudioFormat) -> AVAudioPCMBuffer? {
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(samples.count)),
              let dst = buffer.floatChannelData?[0]
        else { return nil }
        buffer.frameLength = AVAudioFrameCount(samples.count)
        samples.withUnsafeBufferPointer { src in
            if let base = src.baseAddress { dst.update(from: base, count: samples.count) }
        }
        return buffer
    }

    private static func samples(of buffer: AVAudioPCMBuffer) -> [Float] {
        guard let p = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return [] }
        return Array(UnsafeBufferPointer(start: p, count: Int(buffer.frameLength)))
    }
}
