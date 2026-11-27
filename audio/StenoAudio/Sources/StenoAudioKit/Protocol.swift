// Protocole JSON lines entre le sidecar et le cœur (docs/contracts.md §2).
//
// - Une ligne JSON par message, UTF-8, terminée par "\n".
// - Sidecar → cœur : `OutgoingMessage` (champ "type").
// - Cœur → sidecar : `Command` (champ "cmd"). Les champs inconnus sont ignorés ; une commande
//   inconnue ou illisible produit une erreur `bad_command`.

import Foundation

// MARK: - Constantes

enum SidecarInfo {
    static let version = "0.2.1"
}

// MARK: - Types partagés

enum PermissionValue: String, Codable, Equatable {
    case granted, denied, unknown
}

struct PermissionsSnapshot: Equatable {
    var microphone: PermissionValue
    var systemAudio: PermissionValue
    var accessibility: PermissionValue
    var calendar: PermissionValue = .unknown
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
    /// Micro demandé introuvable (ou débranché) : le micro par défaut est utilisé.
    case deviceMissing = "device_missing"
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
    /// Intervenant reconnu en direct (« S1 »…), canal « them » seulement.
    var speaker: String? = nil
}

/// App de visio surveillée (commande `watch`).
///
/// Un processus appartient à l'app si son bundle ID **commence par** l'un des préfixes
/// (comparaison sans casse) : les processus auxiliaires (`com.microsoft.teams2.helper…`)
/// sont donc couverts.
struct WatchedApp: Equatable {
    /// Identifiant stable (`teams`, `slack`, `zoom`…), repris dans `call.app` et `start.apps`.
    var id: String
    /// Nom affiché (journaux, messages d'erreur).
    var name: String
    /// Préfixes de bundle ID.
    var bundlePrefixes: [String]

    /// Liste surveillée avant le premier `watch` : Teams seul (docs/contracts.md §2).
    static let defaults: [WatchedApp] = [
        WatchedApp(id: "teams", name: "Microsoft Teams", bundlePrefixes: ["com.microsoft.teams2", "com.microsoft.teams"]),
    ]

    /// Apps connues des modes en ligne de commande (`--apps`). En mode protocole, c'est le
    /// cœur qui fournit la liste par `watch` ; ce catalogue ne sert qu'aux spikes.
    /// À VÉRIFIER (spike S1) : préfixes réels de Zoom et des navigateurs (`--list-audio-processes`).
    static let catalog: [WatchedApp] = defaults + [
        WatchedApp(id: "slack", name: "Slack", bundlePrefixes: ["com.tinyspeck.slackmacgap"]),
        WatchedApp(id: "zoom", name: "Zoom", bundlePrefixes: ["us.zoom.xos", "us.zoom.CptHost"]),
        WatchedApp(id: "meet", name: "Google Meet (navigateur)", bundlePrefixes: [
            "com.google.Chrome", "com.microsoft.edgemac", "company.thebrowser.Browser",
            "com.brave.Browser", "org.mozilla.firefox", "com.apple.Safari",
        ]),
    ]

    /// Vrai si `bundleID` commence par l'un des préfixes (sans casse).
    func matches(bundleID: String) -> Bool {
        matchLength(bundleID: bundleID) != nil
    }

    /// Longueur du plus long préfixe correspondant (nil si aucun).
    func matchLength(bundleID: String) -> Int? {
        let bundle = bundleID.lowercased()
        return bundlePrefixes
            .map { $0.lowercased() }
            .filter { !$0.isEmpty && bundle.hasPrefix($0) }
            .map(\.count)
            .max()
    }

    /// App à laquelle appartient `bundleID` : celle dont le préfixe correspondant est le plus
    /// long (ex. `com.microsoft.teams2` l'emporte sur un hypothétique `com.microsoft`) ; à
    /// égalité, la première de la liste.
    static func match(bundleID: String, in apps: [WatchedApp]) -> WatchedApp? {
        var best: (app: WatchedApp, length: Int)?
        for app in apps {
            guard let length = app.matchLength(bundleID: bundleID) else { continue }
            if length > (best?.length ?? 0) { best = (app, length) }
        }
        return best?.app
    }
}

/// Choix d'un périphérique dans une commande (`start.input_device`, `set_input.device`,
/// `play.device`) : `null` = périphérique par défaut du système.
enum DeviceSelection: Equatable {
    case systemDefault
    /// UID Core Audio (`kAudioDevicePropertyDeviceUID`).
    case uid(String)

    /// UID demandé, nil pour le périphérique par défaut.
    var uid: String? {
        if case let .uid(value) = self { return value }
        return nil
    }

    /// Libellé pour les journaux.
    var label: String {
        uid.map { "« \($0) »" } ?? "périphérique par défaut"
    }
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
    /// Un appel commence ou se termine dans une app surveillée (remplace `teams_call`).
    case call(active: Bool, app: String, title: String?)
    case segment(SegmentPayload)
    case level(me: Double, them: Double)
    /// Micros et sorties disponibles.
    case devices(AudioDeviceList)
    case state(CaptureState)
    case wakeTest(text: String)
    case diarization(meetingId: String, turns: [DiarizationTurn])
    /// Événements du calendrier (réponse à `calendar_events`).
    case calendar(events: [CalendarEvent])
    /// Phrases retranscrites en fin de réunion (celles qui ne figurent pas gardent le direct).
    case retranscription(meetingId: String, items: [RetranscribedItem])
    case error(code: ErrorCode, message: String, recoverable: Bool)
}

extension OutgoingMessage: Encodable {
    private enum Keys: String, CodingKey {
        case type
        case version, asr
        case microphone
        case systemAudio = "system_audio"
        case accessibility, calendar, events
        case active, app, title
        case ts, t0, t1, channel, text
        case isPrivate = "private"
        case speaker
        case me, them
        case inputs, outputs
        case state
        case meetingId = "meeting_id"
        case turns, items
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
            try c.encode(p.calendar, forKey: .calendar)

        case let .call(active, app, title):
            try c.encode("call", forKey: .type)
            try c.encode(active, forKey: .active)
            try c.encode(app, forKey: .app)
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
            try c.encodeIfPresent(s.speaker, forKey: .speaker)

        case let .level(me, them):
            try c.encode("level", forKey: .type)
            try c.encode(JSONNumber.round(me, decimals: 1), forKey: .me)
            try c.encode(JSONNumber.round(them, decimals: 1), forKey: .them)

        case let .devices(list):
            try c.encode("devices", forKey: .type)
            try c.encode(list.inputs, forKey: .inputs)
            try c.encode(list.outputs, forKey: .outputs)

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

        case let .calendar(events):
            try c.encode("calendar", forKey: .type)
            try c.encode(events, forKey: .events)

        case let .retranscription(meetingId, items):
            try c.encode("retranscription", forKey: .type)
            try c.encode(meetingId, forKey: .meetingId)
            try c.encode(items, forKey: .items)

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
    /// Identifiants des apps dont le son est capté (canal `them`) ; vide = toutes les apps
    /// surveillées.
    var apps: [String] = []
    /// Micro demandé : nil = champ absent (le choix mémorisé par `set_input` s'applique, sinon
    /// le micro par défaut) ; `.systemDefault` = `"input_device": null`.
    var inputDevice: DeviceSelection?
    /// Réunion en présentiel : micro seul (ni tap ni voice processing).
    var room: Bool = false

    /// Bornes raisonnables pour `silence_ms` (le plan vise 0,8 à 1,2 s).
    static let silenceRange = 300...3000
}

enum Command: Equatable {
    /// Liste des apps de visio à surveiller (remplace la précédente).
    case watch([WatchedApp])
    case start(StartOptions)
    /// Change de micro pendant la capture (mémorisé pour le prochain `start` hors capture).
    case setInput(DeviceSelection)
    /// Demande la liste des périphériques (réponse : `devices`).
    case devices
    /// Joue un signal discret sur une sortie.
    case play(sound: FeedbackSound, device: DeviceSelection)
    case stop
    case aside(on: Bool)
    /// Coupe le micro dans Sténo (rien de ce que vous dites n'est transcrit).
    case mute(on: Bool)
    /// Pause / reprise : rien n'est transcrit pendant la pause (silence sur les deux canaux).
    case pause
    case resume
    case wakeTest(seconds: Double)
    /// `file` : fichier de `audioDir` à analyser (`them.wav` par défaut, `me.wav` en présentiel).
    case diarize(meetingId: String, audioDir: String, file: String = "them.wav")
    /// Retranscription de fin de réunion, langue imposée (`fr`…).
    case retranscribe(meetingId: String, audioDir: String, language: String, vocabulary: [String], items: [RetranscribeItem])
    case permissions
    /// Événements du calendrier entre deux instants (ISO 8601 ; défaut : maintenant → +7 jours).
    case calendarEvents(from: Date?, to: Date?)
    /// Déclenche l'invite système d'un droit.
    case requestPermission(PermissionKind)
    case shutdown
}

/// Droits macOS que le cœur peut demander.
enum PermissionKind: String, Equatable {
    case microphone
    case systemAudio = "system_audio"
    case accessibility
    case calendar
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
                opts.apps = uniqueTrimmed(p.apps ?? [])
                opts.inputDevice = p.inputDevice
                opts.room = p.room ?? false
                return .success(.start(opts))

            case "set_input":
                let p = try decoder.decode(SetInputPayload.self, from: data)
                // `device` est obligatoire (null pour le micro par défaut) : une commande sans
                // ce champ est ambiguë.
                guard let device = p.device else { return .failure(.invalidField(command: cmd, field: "device")) }
                return .success(.setInput(device))

            case "devices":
                return .success(.devices)

            case "play":
                let p = try decoder.decode(PlayPayload.self, from: data)
                let name = (p.sound ?? FeedbackSound.detect.rawValue).trimmingCharacters(in: .whitespacesAndNewlines)
                guard let sound = FeedbackSound(rawValue: name) else {
                    return .failure(.invalidField(command: cmd, field: "sound"))
                }
                return .success(.play(sound: sound, device: p.device ?? .systemDefault))

            case "watch":
                let p = try decoder.decode(WatchPayload.self, from: data)
                guard let entries = p.apps else { return .failure(.invalidField(command: cmd, field: "apps")) }
                var seen = Set<String>()
                var apps: [WatchedApp] = []
                // Entrées sans identifiant ou sans préfixe de bundle : ignorées (elles ne
                // pourraient correspondre à aucun processus). Doublons : le premier gagne.
                for entry in entries {
                    guard let id = entry.id?.trimmingCharacters(in: .whitespacesAndNewlines), !id.isEmpty,
                          !seen.contains(id)
                    else { continue }
                    let prefixes = uniqueTrimmed(entry.bundles ?? [])
                    guard !prefixes.isEmpty else { continue }
                    let name = entry.name?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                    seen.insert(id)
                    apps.append(WatchedApp(id: id, name: name.isEmpty ? id : name, bundlePrefixes: prefixes))
                }
                return .success(.watch(apps))

            case "stop":
                return .success(.stop)

            case "aside":
                let p = try decoder.decode(AsidePayload.self, from: data)
                guard let on = p.on else { return .failure(.invalidField(command: cmd, field: "on")) }
                return .success(.aside(on: on))

            case "pause":
                return .success(.pause)

            case "resume":
                return .success(.resume)

            case "mute":
                let p = try decoder.decode(AsidePayload.self, from: data)
                guard let on = p.on else { return .failure(.invalidField(command: cmd, field: "on")) }
                return .success(.mute(on: on))

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
                let file = p.file ?? "them.wav"
                guard ["them.wav", "me.wav"].contains(file) else {
                    return .failure(.invalidField(command: cmd, field: "file"))
                }
                return .success(.diarize(meetingId: id, audioDir: dir, file: file))

            case "retranscribe":
                let p = try decoder.decode(RetranscribePayload.self, from: data)
                guard let id = p.meetingId, !id.isEmpty else {
                    return .failure(.invalidField(command: cmd, field: "meeting_id"))
                }
                guard let dir = p.audioDir, !dir.isEmpty else {
                    return .failure(.invalidField(command: cmd, field: "audio_dir"))
                }
                guard let lang = p.language, !lang.isEmpty else {
                    return .failure(.invalidField(command: cmd, field: "language"))
                }
                let items = p.items ?? []
                guard items.allSatisfy({ ["them.wav", "me.wav"].contains($0.file) }) else {
                    return .failure(.invalidField(command: cmd, field: "items"))
                }
                return .success(.retranscribe(meetingId: id, audioDir: dir, language: lang, vocabulary: p.vocabulary ?? [], items: items))

            case "permissions":
                return .success(.permissions)

            case "calendar_events":
                let p = try decoder.decode(CalendarEventsPayload.self, from: data)
                let iso = ISO8601DateFormatter()
                let from = p.from.flatMap { iso.date(from: $0) }
                let to = p.to.flatMap { iso.date(from: $0) }
                if (p.from != nil && from == nil) || (p.to != nil && to == nil) {
                    return .failure(.invalidField(command: cmd, field: p.from != nil && from == nil ? "from" : "to"))
                }
                return .success(.calendarEvents(from: from, to: to))

            case "request_permission":
                let p = try decoder.decode(RequestPermissionPayload.self, from: data)
                guard let raw = p.kind, let kind = PermissionKind(rawValue: raw) else {
                    return .failure(.invalidField(command: cmd, field: "kind"))
                }
                return .success(.requestPermission(kind))

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

    /// Chaînes nettoyées, non vides, sans doublon (ordre conservé).
    private static func uniqueTrimmed(_ values: [String]) -> [String] {
        var seen = Set<String>()
        var out: [String] = []
        for value in values {
            let v = value.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !v.isEmpty, seen.insert(v).inserted else { continue }
            out.append(v)
        }
        return out
    }

    private static func fieldName(from error: Error) -> String? {
        guard let decodingError = error as? DecodingError else { return nil }
        switch decodingError {
        case let .typeMismatch(_, ctx), let .valueNotFound(_, ctx), let .dataCorrupted(ctx):
            // Premier niveau : `"apps":[{"id":3}]` est signalé comme le champ « apps ».
            return ctx.codingPath.first?.stringValue
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

    private struct RequestPermissionPayload: Decodable {
        let kind: String?
    }

    /// Champ de périphérique : absent → nil ; `null` ou chaîne vide → `.systemDefault` ;
    /// chaîne → `.uid` ; autre type → erreur de décodage (`bad_command` sur ce champ).
    fileprivate static func decodeSelection<K: CodingKey>(_ c: KeyedDecodingContainer<K>, forKey key: K) throws
        -> DeviceSelection?
    {
        guard c.contains(key) else { return nil }
        if try c.decodeNil(forKey: key) { return .systemDefault }
        let uid = try c.decode(String.self, forKey: key).trimmingCharacters(in: .whitespacesAndNewlines)
        return uid.isEmpty ? .systemDefault : .uid(uid)
    }

    private struct StartPayload: Decodable {
        let vocabulary: [String]?
        // Double : accepte 1000 comme 1000.0.
        let silenceMs: Double?
        let keepAudio: Bool?
        let audioDir: String?
        let apps: [String]?
        /// nil = absent (≠ `null`, qui demande le micro par défaut).
        let inputDevice: DeviceSelection?
        let room: Bool?

        enum CodingKeys: String, CodingKey {
            case vocabulary
            case silenceMs = "silence_ms"
            case keepAudio = "keep_audio"
            case audioDir = "audio_dir"
            case apps
            case inputDevice = "input_device"
            case room
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            vocabulary = try c.decodeIfPresent([String].self, forKey: .vocabulary)
            silenceMs = try c.decodeIfPresent(Double.self, forKey: .silenceMs)
            keepAudio = try c.decodeIfPresent(Bool.self, forKey: .keepAudio)
            audioDir = try c.decodeIfPresent(String.self, forKey: .audioDir)
            apps = try c.decodeIfPresent([String].self, forKey: .apps)
            inputDevice = try CommandParser.decodeSelection(c, forKey: .inputDevice)
            room = try c.decodeIfPresent(Bool.self, forKey: .room)
        }
    }

    private struct SetInputPayload: Decodable {
        let device: DeviceSelection?

        enum CodingKeys: String, CodingKey {
            case device
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            device = try CommandParser.decodeSelection(c, forKey: .device)
        }
    }

    private struct PlayPayload: Decodable {
        let sound: String?
        let device: DeviceSelection?

        enum CodingKeys: String, CodingKey {
            case sound, device
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            sound = try c.decodeIfPresent(String.self, forKey: .sound)
            device = try CommandParser.decodeSelection(c, forKey: .device)
        }
    }

    private struct WatchPayload: Decodable {
        let apps: [WatchAppPayload]?
    }

    private struct WatchAppPayload: Decodable {
        let id: String?
        let name: String?
        let bundles: [String]?
    }

    private struct AsidePayload: Decodable {
        let on: Bool?
    }

    private struct WakeTestPayload: Decodable {
        let seconds: Double?
    }

    private struct CalendarEventsPayload: Decodable {
        let from: String?
        let to: String?
    }

    private struct RetranscribePayload: Decodable {
        let meetingId: String?
        let audioDir: String?
        let language: String?
        let vocabulary: [String]?
        let items: [RetranscribeItem]?

        enum CodingKeys: String, CodingKey {
            case meetingId = "meeting_id"
            case audioDir = "audio_dir"
            case language, vocabulary, items
        }
    }

    private struct DiarizePayload: Decodable {
        let meetingId: String?
        let audioDir: String?
        let file: String?

        enum CodingKeys: String, CodingKey {
            case meetingId = "meeting_id"
            case audioDir = "audio_dir"
            case file
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
