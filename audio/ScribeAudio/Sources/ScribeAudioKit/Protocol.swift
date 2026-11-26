// Protocole JSON lines entre le sidecar et le cœur (docs/contracts.md §2).
//
// - Une ligne JSON par message, UTF-8, terminée par "\n".
// - Sidecar → cœur : `OutgoingMessage` (champ "type").
// - Cœur → sidecar : `Command` (champ "cmd"). Les champs inconnus sont ignorés ; une commande
//   inconnue ou illisible produit une erreur `bad_command`.

import Foundation

// MARK: - Constantes

enum SidecarInfo {
    static let version = "0.1.0"
}

// MARK: - Types partagés

enum PermissionValue: String, Codable, Equatable {
    case granted, denied, unknown
}

struct PermissionsSnapshot: Equatable {
    var microphone: PermissionValue
    var systemAudio: PermissionValue
    var accessibility: PermissionValue
}

enum CaptureState: String, Codable, Equatable {
    case listening, paused, stopped
}

enum Channel: String, Codable, Equatable, CaseIterable {
    case me, them
}

enum ErrorCode: String, Codable, Equatable {
    case micDenied = "mic_denied"
    case systemAudioDenied = "system_audio_denied"
    case tapFailed = "tap_failed"
    case deviceChanged = "device_changed"
    case asrFailed = "asr_failed"
    case modelMissing = "model_missing"
    case badCommand = "bad_command"
}

struct SegmentPayload: Equatable {
    /// Horodatage ISO 8601 local avec fuseau de la fin de l'énoncé.
    var ts: String
    /// Secondes depuis `start`.
    var t0: Double
    var t1: Double
    var channel: Channel
    var text: String
    var isPrivate: Bool
}

struct DiarizationTurn: Equatable, Encodable {
    var t0: Double
    var t1: Double
    var speaker: String
}

// MARK: - Sidecar → cœur

enum OutgoingMessage: Equatable {
    case ready(version: String, asr: String)
    case permissions(PermissionsSnapshot)
    case teamsCall(active: Bool, title: String?)
    case segment(SegmentPayload)
    case level(me: Double, them: Double)
    case state(CaptureState)
    case wakeTest(text: String)
    case diarization(meetingId: String, turns: [DiarizationTurn])
    case error(code: ErrorCode, message: String, recoverable: Bool)
}

extension OutgoingMessage: Encodable {
    private enum Keys: String, CodingKey {
        case type
        case version, asr
        case microphone
        case systemAudio = "system_audio"
        case accessibility
        case active, title
        case ts, t0, t1, channel, text
        case isPrivate = "private"
        case me, them
        case state
        case meetingId = "meeting_id"
        case turns
        case code, message, recoverable
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case let .ready(version, asr):
            try c.encode("ready", forKey: .type)
            try c.encode(version, forKey: .version)
            try c.encode(asr, forKey: .asr)

        case let .permissions(p):
            try c.encode("permissions", forKey: .type)
            try c.encode(p.microphone, forKey: .microphone)
            try c.encode(p.systemAudio, forKey: .systemAudio)
            try c.encode(p.accessibility, forKey: .accessibility)

        case let .teamsCall(active, title):
            try c.encode("teams_call", forKey: .type)
            try c.encode(active, forKey: .active)
            // Le contrat veut `"title": null` explicite quand le titre est inconnu.
            if let title {
                try c.encode(title, forKey: .title)
            } else {
                try c.encodeNil(forKey: .title)
            }

        case let .segment(s):
            try c.encode("segment", forKey: .type)
            try c.encode(s.ts, forKey: .ts)
            try c.encode(JSONNumber.round(s.t0, decimals: 2), forKey: .t0)
            try c.encode(JSONNumber.round(s.t1, decimals: 2), forKey: .t1)
            try c.encode(s.channel, forKey: .channel)
            try c.encode(s.text, forKey: .text)
            try c.encode(s.isPrivate, forKey: .isPrivate)

        case let .level(me, them):
            try c.encode("level", forKey: .type)
            try c.encode(JSONNumber.round(me, decimals: 1), forKey: .me)
            try c.encode(JSONNumber.round(them, decimals: 1), forKey: .them)

        case let .state(state):
            try c.encode("state", forKey: .type)
            try c.encode(state, forKey: .state)

        case let .wakeTest(text):
            try c.encode("wake_test", forKey: .type)
            try c.encode(text, forKey: .text)

        case let .diarization(meetingId, turns):
            try c.encode("diarization", forKey: .type)
            try c.encode(meetingId, forKey: .meetingId)
            try c.encode(turns, forKey: .turns)

        case let .error(code, message, recoverable):
            try c.encode("error", forKey: .type)
            try c.encode(code, forKey: .code)
            try c.encode(message, forKey: .message)
            try c.encode(recoverable, forKey: .recoverable)
        }
    }
}

enum JSONNumber {
    /// Arrondit et neutralise les valeurs non finies (JSONEncoder les refuse).
    static func round(_ value: Double, decimals: Int) -> Double {
        guard value.isFinite else { return 0 }
        let factor = pow(10.0, Double(decimals))
        return (value * factor).rounded() / factor
    }
}

// MARK: - Cœur → sidecar

struct StartOptions: Equatable {
    var vocabulary: [String] = []
    var silenceMs: Int = 1000
    var keepAudio: Bool = false
    var audioDir: String?

    /// Bornes raisonnables pour `silence_ms` (le plan vise 0,8 à 1,2 s).
    static let silenceRange = 300...3000
}

enum Command: Equatable {
    case start(StartOptions)
    case stop
    case aside(on: Bool)
    case wakeTest(seconds: Double)
    case diarize(meetingId: String, audioDir: String)
    case permissions
    case shutdown
}

enum CommandParseError: Error, Equatable {
    case invalidJSON
    case missingCmd
    case unknownCommand(String)
    case invalidField(command: String, field: String)

    /// Message en français destiné à l'utilisateur (champ `message` de l'erreur).
    var message: String {
        switch self {
        case .invalidJSON:
            return "Commande illisible : JSON invalide."
        case .missingCmd:
            return "Commande illisible : champ \"cmd\" absent."
        case let .unknownCommand(cmd):
            return "Commande inconnue : \"\(cmd)\"."
        case let .invalidField(cmd, field):
            return "Commande \"\(cmd)\" : champ \"\(field)\" absent ou invalide."
        }
    }
}

enum CommandParser {
    static func parse(_ line: String) -> Result<Command, CommandParseError> {
        let data = Data(line.utf8)
        let decoder = JSONDecoder()

        // 1. Enveloppe : seul "cmd" compte à ce stade.
        let envelope: Envelope
        do {
            envelope = try decoder.decode(Envelope.self, from: data)
        } catch {
            return .failure(.invalidJSON)
        }
        guard let cmd = envelope.cmd else { return .failure(.missingCmd) }

        // 2. Charge utile propre à chaque commande (champs inconnus ignorés par JSONDecoder).
        do {
            switch cmd {
            case "start":
                let p = try decoder.decode(StartPayload.self, from: data)
                var opts = StartOptions()
                opts.vocabulary = (p.vocabulary ?? [])
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .filter { !$0.isEmpty }
                if let ms = p.silenceMs, ms.isFinite {
                    let r = StartOptions.silenceRange
                    opts.silenceMs = min(max(Int(ms.rounded()), r.lowerBound), r.upperBound)
                }
                opts.keepAudio = p.keepAudio ?? false
                if let dir = p.audioDir, !dir.isEmpty { opts.audioDir = dir }
                return .success(.start(opts))

            case "stop":
                return .success(.stop)

            case "aside":
                let p = try decoder.decode(AsidePayload.self, from: data)
                guard let on = p.on else { return .failure(.invalidField(command: cmd, field: "on")) }
                return .success(.aside(on: on))

            case "wake_test":
                let p = try decoder.decode(WakeTestPayload.self, from: data)
                var seconds = p.seconds ?? 8
                if !seconds.isFinite { seconds = 8 }
                return .success(.wakeTest(seconds: min(max(seconds, 1), 30)))

            case "diarize":
                let p = try decoder.decode(DiarizePayload.self, from: data)
                guard let id = p.meetingId, !id.isEmpty else {
                    return .failure(.invalidField(command: cmd, field: "meeting_id"))
                }
                guard let dir = p.audioDir, !dir.isEmpty else {
                    return .failure(.invalidField(command: cmd, field: "audio_dir"))
                }
                return .success(.diarize(meetingId: id, audioDir: dir))

            case "permissions":
                return .success(.permissions)

            case "shutdown":
                return .success(.shutdown)

            default:
                return .failure(.unknownCommand(cmd))
            }
        } catch {
            // Un champ connu a un type inattendu (ex. "on":"oui").
            return .failure(.invalidField(command: cmd, field: Self.fieldName(from: error) ?? "?"))
        }
    }

    private static func fieldName(from error: Error) -> String? {
        guard let decodingError = error as? DecodingError else { return nil }
        switch decodingError {
        case let .typeMismatch(_, ctx), let .valueNotFound(_, ctx), let .dataCorrupted(ctx):
            return ctx.codingPath.last?.stringValue
        case let .keyNotFound(key, _):
            return key.stringValue
        @unknown default:
            return nil
        }
    }

    // MARK: Charges utiles

    private struct Envelope: Decodable {
        let cmd: String?
    }

    private struct StartPayload: Decodable {
        let vocabulary: [String]?
        // Double : accepte 1000 comme 1000.0.
        let silenceMs: Double?
        let keepAudio: Bool?
        let audioDir: String?

        enum CodingKeys: String, CodingKey {
            case vocabulary
            case silenceMs = "silence_ms"
            case keepAudio = "keep_audio"
            case audioDir = "audio_dir"
        }
    }

    private struct AsidePayload: Decodable {
        let on: Bool?
    }

    private struct WakeTestPayload: Decodable {
        let seconds: Double?
    }

    private struct DiarizePayload: Decodable {
        let meetingId: String?
        let audioDir: String?

        enum CodingKeys: String, CodingKey {
            case meetingId = "meeting_id"
            case audioDir = "audio_dir"
        }
    }
}

// MARK: - Horodatage

enum Timestamp {
    /// `2026-11-26T10:41:12+01:00` : ISO 8601, heure locale, décalage avec deux-points
    /// (jamais « Z », même en UTC, pour rester homogène avec le reste de la base).
    static func iso8601(_ date: Date, timeZone: TimeZone = .current) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = timeZone
        f.dateFormat = "yyyy-MM-dd'T'HH:mm:ssxxxxx"
        return f.string(from: date)
    }
}
