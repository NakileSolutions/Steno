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
