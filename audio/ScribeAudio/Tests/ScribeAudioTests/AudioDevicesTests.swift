import XCTest
@testable import ScribeAudioKit

final class AudioDevicesTests: XCTestCase {
    private typealias Catalog = AudioDeviceCatalog

    // MARK: Transport

    func testFourCC() {
        XCTAssertEqual(Catalog.fourCC("bltn"), 0x626C_746E)
        XCTAssertEqual(Catalog.fourCC("usb "), 0x7573_6220)
        XCTAssertEqual(Catalog.fourCCString(0x626C_746E), "bltn")
        XCTAssertEqual(Catalog.fourCCString(0), "0", "code non imprimable : valeur numérique")
    }

    func testTransportClassification() {
        let cases: [(String, AudioTransport)] = [
            ("bltn", .builtin),
            ("usb ", .usb),
            ("blue", .bluetooth),
            ("blea", .bluetooth),
            ("virt", .virtual),
            ("grup", .aggregate),
            ("fgrp", .aggregate),
            ("pci ", .other),
            ("1394", .other),
            ("hdmi", .other),
            ("dprt", .other),
            ("airp", .other),
            ("eavb", .other),
            ("thun", .other),
            ("ccwd", .other),
            ("ccwl", .other),
        ]
        for (code, expected) in cases {
            XCTAssertEqual(Catalog.classify(transportType: Catalog.fourCC(code)), expected, code)
        }
        XCTAssertEqual(Catalog.classify(transportType: 0), .other, "transport inconnu")
        XCTAssertEqual(Catalog.classify(transportType: 0xDEAD_BEEF), .other)
    }

    // MARK: Filtrage

    private func raw(_ id: UInt32, uid: String?, name: String?, transport: String = "usb ",
                     inputs: Int = 0, outputs: Int = 0) -> RawAudioDevice
    {
        RawAudioDevice(objectID: id, uid: uid, name: name, transportType: Catalog.fourCC(transport),
                       inputChannels: inputs, outputChannels: outputs)
    }

    func testBuildFiltersAndFlagsDefaults() {
        let devices = [
            raw(10, uid: "BuiltInMicrophoneDevice", name: "Micro du MacBook Pro", transport: "bltn", inputs: 1),
            raw(11, uid: "BuiltInSpeakerDevice", name: "Haut-parleurs du MacBook Pro", transport: "bltn", outputs: 2),
            raw(20, uid: "AirPodsUID", name: "AirPods Pro", transport: "blue", inputs: 1, outputs: 2),
            raw(30, uid: "scribe-tap-1234", name: "Scribe-Tap", transport: "grup", inputs: 2, outputs: 2),
            raw(31, uid: "CADefaultDeviceAggregate-123-0", name: "CADefaultDeviceAggregate-123-0", transport: "grup", inputs: 1, outputs: 2),
            raw(32, uid: "VPAUAggregateAudioDevice-0x1", name: "VPAUAggregateAudioDevice", transport: "grup", inputs: 1, outputs: 2),
            raw(40, uid: "", name: "Sans UID", inputs: 1),
            raw(41, uid: nil, name: "UID absent", inputs: 1),
            raw(42, uid: "NoName", name: "   ", inputs: 1),
            raw(43, uid: "NilName", name: nil, outputs: 1),
            raw(50, uid: "Silent", name: "Sans canal"),
            raw(60, uid: "BlackHole2ch_UID", name: " BlackHole 2ch ", transport: "virt", inputs: 2, outputs: 2),
            raw(61, uid: "BlackHole2ch_UID", name: "Doublon", transport: "virt", inputs: 2, outputs: 2),
        ]
        let list = Catalog.build(devices, defaultInput: 20, defaultOutput: 11)

        XCTAssertEqual(list.inputs, [
            AudioDeviceInfo(id: "AirPodsUID", name: "AirPods Pro", isDefault: true, transport: .bluetooth),
            AudioDeviceInfo(id: "BlackHole2ch_UID", name: "BlackHole 2ch", isDefault: false, transport: .virtual),
            AudioDeviceInfo(id: "BuiltInMicrophoneDevice", name: "Micro du MacBook Pro", isDefault: false, transport: .builtin),
        ])
        XCTAssertEqual(list.outputs, [
            AudioDeviceInfo(id: "BuiltInSpeakerDevice", name: "Haut-parleurs du MacBook Pro", isDefault: true, transport: .builtin),
            AudioDeviceInfo(id: "AirPodsUID", name: "AirPods Pro", isDefault: false, transport: .bluetooth),
            AudioDeviceInfo(id: "BlackHole2ch_UID", name: "BlackHole 2ch", isDefault: false, transport: .virtual),
        ])
    }

    func testBuildWithoutDefaults() {
        let list = Catalog.build([raw(1, uid: "A", name: "A", inputs: 1, outputs: 1)], defaultInput: nil, defaultOutput: nil)
        XCTAssertEqual(list.inputs.map(\.isDefault), [false])
        XCTAssertEqual(list.outputs.map(\.isDefault), [false])
        XCTAssertEqual(Catalog.build([], defaultInput: 1, defaultOutput: 2), .empty)
    }

    func testExclusionReasons() {
        XCTAssertNil(Catalog.exclusionReason(raw(1, uid: "A", name: "Micro", inputs: 1)))
        XCTAssertEqual(Catalog.exclusionReason(raw(1, uid: " ", name: "Micro")), "UID vide")
        XCTAssertEqual(Catalog.exclusionReason(raw(1, uid: "A", name: "")), "nom vide")
        XCTAssertEqual(Catalog.exclusionReason(raw(1, uid: "scribe-tap-XYZ", name: "Autre nom")), "agrégat privé")
        XCTAssertEqual(Catalog.exclusionReason(raw(1, uid: "autre", name: "Scribe-Tap")), "agrégat privé")
        // Un agrégat créé par l'utilisateur (Configuration audio et MIDI) reste proposé.
        XCTAssertNil(Catalog.exclusionReason(raw(1, uid: "~:AMS2_Aggregate:0", name: "Appareil agrégé", transport: "grup")))
    }

    // MARK: Tri

    func testSortDefaultFirstThenName() {
        let devices = [
            AudioDeviceInfo(id: "z", name: "zoomAudioDevice", isDefault: false, transport: .virtual),
            AudioDeviceInfo(id: "m", name: "Micro du MacBook Pro", isDefault: false, transport: .builtin),
            AudioDeviceInfo(id: "e", name: "Écouteurs USB", isDefault: false, transport: .usb),
            AudioDeviceInfo(id: "y", name: "Yeti", isDefault: true, transport: .usb),
            AudioDeviceInfo(id: "a", name: "airpods", isDefault: false, transport: .bluetooth),
            AudioDeviceInfo(id: "m10", name: "Micro 10", isDefault: false, transport: .usb),
            AudioDeviceInfo(id: "m2", name: "Micro 2", isDefault: false, transport: .usb),
        ]
        XCTAssertEqual(Catalog.sorted(devices).map(\.id), ["y", "a", "e", "m2", "m10", "m", "z"])
    }

    func testSortTieBreaksOnUID() {
        let devices = [
            AudioDeviceInfo(id: "uid-b", name: "AirPods", isDefault: false, transport: .bluetooth),
            AudioDeviceInfo(id: "uid-a", name: "AirPods", isDefault: false, transport: .bluetooth),
        ]
        XCTAssertEqual(Catalog.sorted(devices).map(\.id), ["uid-a", "uid-b"])
    }

    // MARK: Signal sonore

    func testDetectToneShape() {
        let rate = FeedbackTone.sampleRate
        let samples = FeedbackTone.samples(for: .detect)
        let duration = Double(samples.count) / rate
        XCTAssertEqual(duration, 0.12, accuracy: 0.01, "≈ 120 ms")
        let peak = samples.reduce(Float(0)) { max($0, abs($1)) }
        XCTAssertGreaterThan(peak, 0.05, "audible")
        XCTAssertLessThanOrEqual(peak, FeedbackTone.detectGain + 1e-6, "volume modéré")
        XCTAssertEqual(samples.first ?? 1, 0, accuracy: 1e-6, "pas de clic au début")
        XCTAssertEqual(samples.last ?? 1, 0, accuracy: 1e-6, "pas de clic à la fin")
        // Deux notes : le premier échantillon de la seconde note est nul lui aussi.
        let firstNote = Int((FeedbackTone.detectNotes[0].duration * rate).rounded())
        XCTAssertEqual(samples[firstNote], 0, accuracy: 1e-6)
        XCTAssertEqual(samples[firstNote - 1], 0, accuracy: 1e-6)
    }

    func testSynthesizeIgnoresEmptyNotes() {
        let out = FeedbackTone.synthesize(notes: [.init(frequency: 440, duration: 0)], gain: 0.5, sampleRate: 48_000)
        XCTAssertTrue(out.isEmpty)
    }
}
