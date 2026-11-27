// Diarisation hors ligne du canal « them » (après la réunion).
//
// Le moteur (FluidAudio, pipeline hors ligne) est derrière le protocole `SpeakerDiarizer`
// (FluidAudioBackend.swift). Ici : la mise en forme des tours de parole pour le message
// `{"type":"diarization","meeting_id":…,"turns":[{"t0":…,"t1":…,"speaker":"S1"}]}`.
//
// Les temps sont relatifs au début de them.wav, qui commence au `start` de la capture
// (les trous du tap sont comblés par du silence, voir ChannelPipeline) : ils sont donc
// directement comparables aux t0/t1 des segments.

import Foundation

struct RawSpeakerTurn: Equatable {
    var speakerId: String
    var start: Double
    var end: Double
}

enum DiarizationMapper {
    /// Renomme les locuteurs S1, S2… par ordre de première prise de parole, fusionne les
    /// tours consécutifs d'un même locuteur séparés de moins de `mergeGap` secondes.
    static func turns(from raw: [RawSpeakerTurn], mergeGap: Double = 0.3) -> [DiarizationTurn] {
        let sorted = raw
            .filter { $0.end > $0.start && $0.start.isFinite && $0.end.isFinite }
            .sorted { $0.start < $1.start }

        var labels: [String: String] = [:]
        var result: [DiarizationTurn] = []
        for turn in sorted {
            let label: String
            if let existing = labels[turn.speakerId] {
                label = existing
            } else {
                label = "S\(labels.count + 1)"
                labels[turn.speakerId] = label
            }
            if var last = result.last, last.speaker == label, turn.start - last.t1 <= mergeGap {
                last.t1 = max(last.t1, turn.end)
                result[result.count - 1] = last
            } else {
                result.append(DiarizationTurn(t0: turn.start, t1: turn.end, speaker: label))
            }
        }
        return result.map {
            DiarizationTurn(t0: JSONNumber.round($0.t0, decimals: 2),
                            t1: JSONNumber.round($0.t1, decimals: 2),
                            speaker: $0.speaker)
        }
    }
}

enum DiarizationError: Error, CustomStringConvertible {
    case missingFile(String)

    var description: String {
        switch self {
        case let .missingFile(path): return "Fichier audio introuvable : \(path)"
        }
    }
}

/// Chemin de `<audio_dir>/them.wav`, ou erreur si le fichier n'existe pas.
func diarizationInput(audioDir: String, file: String = "them.wav") throws -> URL {
    let url = URL(fileURLWithPath: audioDir, isDirectory: true).appendingPathComponent(file)
    guard FileManager.default.fileExists(atPath: url.path) else {
        throw DiarizationError.missingFile(url.path)
    }
    return url
}

/// Exécute la diarisation d'un fichier et met en forme les tours de parole.
func runDiarization(fileURL: URL, diarizer: SpeakerDiarizer) async throws -> [DiarizationTurn] {
    let started = Date()
    let raw = try await diarizer.diarize(fileURL: fileURL)
    let turns = DiarizationMapper.turns(from: raw)
    let speakers = Set(turns.map(\.speaker)).count
    Log.info("diarisation : \(turns.count) tours, \(speakers) locuteurs, \(String(format: "%.1f", Date().timeIntervalSince(started))) s")
    return turns
}
