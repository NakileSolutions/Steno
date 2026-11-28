@testable import StenoAudioKit
import XCTest

final class CalendarTests: XCTestCase {
    func testCommandeEtDates() {
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"calendar_events"}"#), .success(.calendarEvents(from: nil, to: nil)))
        guard case let .success(.calendarEvents(from, to)) = CommandParser.parse(#"{"cmd":"calendar_events","from":"2026-11-27T08:00:00Z","to":"2026-11-28T08:00:00Z"}"#) else {
            return XCTFail("calendar_events attendu")
        }
        XCTAssertEqual(to!.timeIntervalSince(from!), 86_400)
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"calendar_events","from":"hier"}"#),
                       .failure(.invalidField(command: "calendar_events", field: "from")))
        XCTAssertEqual(CommandParser.parse(#"{"cmd":"request_permission","kind":"calendar"}"#), .success(.requestPermission(.calendar)))
    }

    func testEmailDesParticipants() {
        XCTAssertEqual(CalendarReader.email(from: URL(string: "mailto:marie.dupont@exemple.fr")), "marie.dupont@exemple.fr")
        XCTAssertNil(CalendarReader.email(from: URL(string: "https://exemple.fr")))
        XCTAssertNil(CalendarReader.email(from: nil))
    }

    func testRecurringOccurrencesGetDistinctIds() {
        let monday = Date(timeIntervalSince1970: 1_795_852_800)
        let tuesday = monday.addingTimeInterval(86_400)
        let a = CalendarReader.eventId(external: "daily", local: "local-1", recurring: true, occurrence: monday)
        let b = CalendarReader.eventId(external: "daily", local: "local-1", recurring: true, occurrence: tuesday)
        XCTAssertEqual(a, "daily-1795852800")
        XCTAssertNotEqual(a, b, "two occurrences of a daily must not share an id")
        XCTAssertEqual(CalendarReader.eventId(external: nil, local: "local-1", recurring: true, occurrence: monday),
                       "local-1-1795852800")
        // One-off events keep their identifier (meetings already linked to them stay linked).
        XCTAssertEqual(CalendarReader.eventId(external: "once", local: "local-2", recurring: false, occurrence: monday), "once")
    }

    func testCancelledAndAllDayEventsAreNotListed() {
        XCTAssertTrue(CalendarReader.listed(allDay: false, status: .confirmed))
        XCTAssertTrue(CalendarReader.listed(allDay: false, status: .tentative))
        XCTAssertTrue(CalendarReader.listed(allDay: false, status: .none))
        XCTAssertFalse(CalendarReader.listed(allDay: false, status: .canceled))
        XCTAssertFalse(CalendarReader.listed(allDay: true, status: .confirmed))
    }

    func testEncodage() throws {
        let ev = CalendarEvent(id: "e1", title: "Point Nova", start: "2026-11-27T10:00:00+01:00", end: "2026-11-27T10:30:00+01:00",
                               allDay: false, location: nil, url: nil, notes: nil, calendar: "Travail",
                               attendees: [CalendarAttendee(name: "Marie", email: "marie@exemple.fr", status: "accepted", organizer: true, me: false)])
        let data = try JSONEncoder().encode(OutgoingMessage.calendar(events: [ev]))
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(json["type"] as? String, "calendar")
        let first = try XCTUnwrap((json["events"] as? [[String: Any]])?.first)
        XCTAssertEqual(first["all_day"] as? Bool, false)
        XCTAssertEqual((first["attendees"] as? [[String: Any]])?.first?["email"] as? String, "marie@exemple.fr")
        let perm = try JSONSerialization.jsonObject(with: JSONEncoder().encode(OutgoingMessage.permissions(
            PermissionsSnapshot(microphone: .granted, systemAudio: .unknown, accessibility: .denied, calendar: .granted)))) as? [String: Any]
        XCTAssertEqual(perm?["calendar"] as? String, "granted")
    }
}
