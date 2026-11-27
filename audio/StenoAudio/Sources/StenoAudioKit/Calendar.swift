// Calendrier macOS (EventKit) : événements à venir et leurs participants invités.
//
// Sert à préparer les réunions et à nommer les intervenants : les invités d'un événement
// aident l'agent du compte rendu à remplacer « Intervenant 1, 2… » par de vrais noms. Lecture
// seule ; rien n'est écrit dans le calendrier. Permission « Calendriers » (accès complet,
// macOS 14+), facultative.

import EventKit
import Foundation

struct CalendarAttendee: Codable, Equatable {
    var name: String?
    var email: String?
    /// `accepted | declined | tentative | pending | unknown`.
    var status: String
    var organizer: Bool
    var me: Bool
}

struct CalendarEvent: Codable, Equatable {
    var id: String
    var title: String
    var start: String
    var end: String
    var allDay: Bool
    var location: String?
    var url: String?
    var notes: String?
    var calendar: String?
    var attendees: [CalendarAttendee]

    enum CodingKeys: String, CodingKey {
        case id, title, start, end, location, url, notes, calendar, attendees
        case allDay = "all_day"
    }
}

enum CalendarReader {
    private static let store = EKEventStore()

    static func status() -> PermissionValue {
        switch EKEventStore.authorizationStatus(for: .event) {
        case .fullAccess, .authorized:
            return .granted
        case .denied, .restricted, .writeOnly:
            return .denied
        case .notDetermined:
            return .unknown
        @unknown default:
            return .unknown
        }
    }

    /// Affiche l'invite système (sans effet si l'utilisateur a déjà répondu).
    static func request() async -> Bool {
        (try? await store.requestFullAccessToEvents()) ?? false
    }

    /// Adresse e-mail d'un participant (`mailto:…`).
    static func email(from url: URL?) -> String? {
        guard let s = url?.absoluteString, s.lowercased().hasPrefix("mailto:") else { return nil }
        let e = String(s.dropFirst("mailto:".count)).removingPercentEncoding ?? ""
        return e.contains("@") ? e : nil
    }

    static func statusLabel(_ s: EKParticipantStatus) -> String {
        switch s {
        case .accepted: return "accepted"
        case .declined: return "declined"
        case .tentative: return "tentative"
        case .pending: return "pending"
        default: return "unknown"
        }
    }

    /// Événements entre `from` et `to` (hors journées entières), triés par début.
    static func events(from: Date, to: Date) -> [CalendarEvent] {
        guard status() == .granted else { return [] }
        store.refreshSourcesIfNecessary()
        let predicate = store.predicateForEvents(withStart: from, end: to, calendars: nil)
        return store.events(matching: predicate)
            .filter { !$0.isAllDay }
            .sorted { $0.startDate < $1.startDate }
            .prefix(200)
            .map { ev in
                let organizerURL = ev.organizer?.url
                let people = (ev.attendees ?? []).map { p in
                    CalendarAttendee(
                        name: p.name?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty,
                        email: email(from: p.url),
                        status: statusLabel(p.participantStatus),
                        organizer: organizerURL != nil && p.url == organizerURL,
                        me: p.isCurrentUser
                    )
                }
                return CalendarEvent(
                    id: ev.calendarItemExternalIdentifier ?? ev.eventIdentifier ?? UUID().uuidString,
                    title: ev.title ?? "",
                    start: Timestamp.iso8601(ev.startDate),
                    end: Timestamp.iso8601(ev.endDate),
                    allDay: ev.isAllDay,
                    location: ev.location?.nilIfEmpty,
                    url: ev.url?.absoluteString,
                    notes: ev.notes.map { String($0.prefix(4000)) },
                    calendar: ev.calendar?.title,
                    attendees: people
                )
            }
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
