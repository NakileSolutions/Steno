// Écriture WAV PCM 16 bits mono 16 kHz, seulement si `start.keep_audio` vaut true
// (ou en mode spike). Le fichier est créé en 0600.

import Foundation

final class WavWriter {
    let url: URL
    let sampleRate: Int

    private let handle: FileHandle
    private var dataBytes: UInt32 = 0
    private var closed = false
    /// `dataBytes` when the header was last written.
    private var headerBytes: UInt32 = 0

    /// The header is rewritten with the current sizes every this many seconds of audio, so a
    /// sidecar killed before `close` (SIGKILL, crash) leaves a readable file missing at most
    /// that much audio.
    static let headerRefreshSeconds: Double = 5

    init(url: URL, sampleRate: Int = AudioFormat.sampleRateInt) throws {
        self.url = url
        self.sampleRate = sampleRate
        let fm = FileManager.default
        try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard fm.createFile(atPath: url.path, contents: nil, attributes: [.posixPermissions: 0o600]) else {
            throw CocoaError(.fileWriteUnknown, userInfo: [NSFilePathErrorKey: url.path])
        }
        handle = try FileHandle(forWritingTo: url)
        try handle.write(contentsOf: Self.header(sampleRate: sampleRate, dataBytes: 0))
    }

    deinit {
        try? close()
    }

    /// Ajoute des échantillons Float32 dans [-1, 1].
    func append(_ samples: [Float]) throws {
        guard !closed, !samples.isEmpty else { return }
        var data = Data(count: samples.count * 2)
        data.withUnsafeMutableBytes { (raw: UnsafeMutableRawBufferPointer) in
            let out = raw.bindMemory(to: Int16.self)
            for (i, s) in samples.enumerated() {
                let clamped = max(-1, min(1, s.isFinite ? s : 0))
                out[i] = Int16(clamped * 32767).littleEndian
            }
        }
        try write(data)
    }

    /// Ajoute `count` échantillons de silence (trous du flux). Never capped: the segmenter clock
    /// advances by the whole gap, and `retranscribe` / diarization cut the WAV at `t0`/`t1`, so
    /// the file must advance by as much (about 115 MB per hour of gap; the 4 GiB WAV limit
    /// still applies).
    func appendSilence(_ count: Int) throws {
        guard !closed, count > 0 else { return }
        var remaining = count
        let chunk = Data(count: min(remaining, sampleRate) * 2)
        while remaining > 0 {
            let n = min(remaining, sampleRate)
            try write(n == sampleRate ? chunk : Data(count: n * 2))
            remaining -= n
        }
    }

    var durationSeconds: Double { Double(dataBytes / 2) / Double(sampleRate) }

    /// Réécrit l'en-tête avec les tailles finales et ferme le fichier.
    func close() throws {
        guard !closed else { return }
        closed = true
        defer { try? handle.close() }
        try handle.seek(toOffset: 0)
        try handle.write(contentsOf: Self.header(sampleRate: sampleRate, dataBytes: dataBytes))
        try handle.synchronize()
    }

    private func write(_ data: Data) throws {
        // Limite du format WAV (4 Gio) : au-delà on arrête d'écrire plutôt que corrompre.
        guard UInt64(dataBytes) + UInt64(data.count) < UInt64(UInt32.max - 64) else { return }
        try handle.write(contentsOf: data)
        dataBytes += UInt32(data.count)
        let refreshBytes = UInt32(Self.headerRefreshSeconds * Double(sampleRate)) * 2
        if dataBytes - headerBytes >= refreshBytes { try refreshHeader() }
    }

    /// Rewrites the header with the current sizes, then goes back to the end of the file.
    private func refreshHeader() throws {
        try handle.seek(toOffset: 0)
        try handle.write(contentsOf: Self.header(sampleRate: sampleRate, dataBytes: dataBytes))
        try handle.seekToEnd()
        headerBytes = dataBytes
    }

    static func header(sampleRate: Int, dataBytes: UInt32) -> Data {
        var d = Data()
        func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        func u16(_ v: UInt16) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        let channels: UInt16 = 1
        let bits: UInt16 = 16
        let byteRate = UInt32(sampleRate) * UInt32(channels) * UInt32(bits / 8)
        d.append(contentsOf: Array("RIFF".utf8)); u32(36 + dataBytes)
        d.append(contentsOf: Array("WAVE".utf8))
        d.append(contentsOf: Array("fmt ".utf8)); u32(16)
        u16(1) // PCM
        u16(channels)
        u32(UInt32(sampleRate))
        u32(byteRate)
        u16(channels * bits / 8)
        u16(bits)
        d.append(contentsOf: Array("data".utf8)); u32(dataBytes)
        return d
    }
}
