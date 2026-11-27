// Périphériques audio : énumération (HAL), classement du transport, filtrage, tri, écouteurs.
//
// - Partie pure (testable sans matériel) : `AudioTransport`, `AudioDeviceInfo`,
//   `AudioDeviceList`, `RawAudioDevice`, `AudioDeviceCatalog`.
// - Partie HAL (macOS 14.2+) : `AudioDevices` (lecture des propriétés, recherche par UID,
//   sélection du périphérique d'un nœud AVAudioEngine) et `AudioDeviceWatcher` (écouteurs
//   sur la liste et les périphériques par défaut, anti-rebond ≈ 300 ms).
//
// Message `devices` : docs/contracts.md §2.

import AudioToolbox
import AVFoundation
import CoreAudio
import Foundation

// MARK: - Types du protocole

/// Transport d'un périphérique, d'après `kAudioDevicePropertyTransportType`.
enum AudioTransport: String, Codable, Equatable, CaseIterable {
    case builtin, usb, bluetooth, virtual, aggregate, other
}

/// Entrée de `devices.inputs` / `devices.outputs`.
struct AudioDeviceInfo: Equatable, Encodable {
    /// UID Core Audio (`kAudioDevicePropertyDeviceUID`), stable d'un branchement à l'autre.
    var id: String
    var name: String
    /// Périphérique par défaut du système (entrée pour `inputs`, sortie pour `outputs`).
    var isDefault: Bool
    var transport: AudioTransport

    enum CodingKeys: String, CodingKey {
        case id, name
        case isDefault = "default"
        case transport
    }
}

/// Contenu du message `devices`.
struct AudioDeviceList: Equatable {
    var inputs: [AudioDeviceInfo]
    var outputs: [AudioDeviceInfo]

    static let empty = AudioDeviceList(inputs: [], outputs: [])
}

/// Messages d'erreur liés aux périphériques (champ `message`).
enum DeviceMessages {
    /// Erreur `device_missing` (texte fixé par docs/contracts.md §2).
    static func inputMissing(_ uid: String) -> String {
        "Micro « \(uid) » introuvable : micro par défaut utilisé."
    }
}

// MARK: - Partie pure

/// Propriétés brutes d'un périphérique, telles que lues dans le HAL.
struct RawAudioDevice: Equatable {
    var objectID: UInt32
    var uid: String?
    var name: String?
    /// Valeur brute de `kAudioDevicePropertyTransportType` (code à quatre caractères).
    var transportType: UInt32
    var inputChannels: Int
    var outputChannels: Int
}

enum AudioDeviceCatalog {
    /// Préfixe de l'UID de l'agrégat privé créé par `ProcessTap`.
    static let scribeTapUIDPrefix = "scribe-tap-"
    /// Nom de l'agrégat privé créé par `ProcessTap`.
    static let scribeTapName = "Scribe-Tap"

    /// Agrégats privés à ne jamais proposer : celui du tap de Scribe, et ceux qu'AVAudioEngine
    /// crée dans notre processus (entrée et sortie par défaut séparées, voice processing).
    /// À VÉRIFIER (spike S1, `--list-devices` pendant une capture) : préfixes réels des agrégats
    /// créés par AVAudioEngine (`CADefaultDeviceAggregate-…`, `VPAUAggregateAudioDevice-…`).
    static let privateAggregateUIDPrefixes = [
        scribeTapUIDPrefix,
        "CADefaultDeviceAggregate",
        "VPAUAggregateAudioDevice",
    ]

    // Codes de `kAudioDeviceTransportType…` (AudioHardwareBase.h), recopiés ici pour que le
    // classement reste une fonction pure, testable sans dépendre du type des constantes importées.
    static let transportUnknown: UInt32 = 0
    static let transportBuiltIn = fourCC("bltn")
    static let transportAggregate = fourCC("grup")
    static let transportAutoAggregate = fourCC("fgrp")
    static let transportVirtual = fourCC("virt")
    static let transportPCI = fourCC("pci ")
    static let transportUSB = fourCC("usb ")
    static let transportFireWire = fourCC("1394")
    static let transportBluetooth = fourCC("blue")
    static let transportBluetoothLE = fourCC("blea")
    static let transportHDMI = fourCC("hdmi")
    static let transportDisplayPort = fourCC("dprt")
    static let transportAirPlay = fourCC("airp")
    static let transportAVB = fourCC("eavb")
    static let transportThunderbolt = fourCC("thun")
    static let transportContinuityWired = fourCC("ccwd")
    static let transportContinuityWireless = fourCC("ccwl")

    /// Code à quatre caractères ASCII → entier (`"bltn"` → 0x626C746E).
    static func fourCC(_ code: String) -> UInt32 {
        code.utf8.prefix(4).reduce(UInt32(0)) { ($0 << 8) | UInt32($1) }
    }

    /// Entier → code lisible (`0x626C746E` → `"bltn"`), pour les diagnostics.
    static func fourCCString(_ value: UInt32) -> String {
        let bytes = [UInt8((value >> 24) & 0xFF), UInt8((value >> 16) & 0xFF),
                     UInt8((value >> 8) & 0xFF), UInt8(value & 0xFF)]
        guard bytes.allSatisfy({ $0 >= 32 && $0 < 127 }) else { return String(value) }
        return String(decoding: bytes, as: UTF8.self)
    }

    /// Classement du transport pour le protocole.
    static func classify(transportType raw: UInt32) -> AudioTransport {
        switch raw {
        case transportBuiltIn:
            return .builtin
        case transportUSB:
            return .usb
        case transportBluetooth, transportBluetoothLE:
            return .bluetooth
        case transportVirtual:
            return .virtual
        case transportAggregate, transportAutoAggregate:
            return .aggregate
        default:
            // PCI, FireWire, HDMI, DisplayPort, AirPlay, AVB, Thunderbolt, Continuity, inconnu.
            return .other
        }
    }

    /// Vrai si le périphérique est un agrégat privé (tap de Scribe ou agrégat interne).
    static func isPrivateAggregate(uid: String, name: String) -> Bool {
        name == scribeTapName || privateAggregateUIDPrefixes.contains { uid.hasPrefix($0) }
    }

    /// Raison d'exclure un périphérique des deux listes (nil = acceptable), sans regarder les
    /// canaux (critère propre à chaque liste).
    static func exclusionReason(_ device: RawAudioDevice) -> String? {
        let uid = trimmed(device.uid)
        let name = trimmed(device.name)
        if uid.isEmpty { return "UID vide" }
        if name.isEmpty { return "nom vide" }
        if isPrivateAggregate(uid: uid, name: name) { return "agrégat privé" }
        return nil
    }

    /// Construit le message `devices` : filtre (canaux, UID/nom vides, agrégats privés,
    /// doublons d'UID) et trie (défaut d'abord, puis par nom).
    static func build(_ devices: [RawAudioDevice], defaultInput: UInt32?, defaultOutput: UInt32?) -> AudioDeviceList {
        var inputs: [AudioDeviceInfo] = []
        var outputs: [AudioDeviceInfo] = []
        var seenInputs = Set<String>()
        var seenOutputs = Set<String>()
        for device in devices where exclusionReason(device) == nil {
            let uid = trimmed(device.uid)
            let name = trimmed(device.name)
            let transport = classify(transportType: device.transportType)
            if device.inputChannels > 0, seenInputs.insert(uid).inserted {
                inputs.append(AudioDeviceInfo(id: uid, name: name,
                                              isDefault: defaultInput == device.objectID, transport: transport))
            }
            if device.outputChannels > 0, seenOutputs.insert(uid).inserted {
                outputs.append(AudioDeviceInfo(id: uid, name: name,
                                               isDefault: defaultOutput == device.objectID, transport: transport))
            }
        }
        return AudioDeviceList(inputs: sorted(inputs), outputs: sorted(outputs))
    }

    /// Tri stable et déterministe : défaut d'abord, puis nom (sans casse ni accents, nombres
    /// dans l'ordre naturel), puis UID.
    static func sorted(_ devices: [AudioDeviceInfo]) -> [AudioDeviceInfo] {
        devices.sorted { a, b in
            if a.isDefault != b.isDefault { return a.isDefault }
            let order = a.name.compare(b.name, options: [.caseInsensitive, .diacriticInsensitive, .numeric],
                                       range: nil, locale: Locale(identifier: "fr_FR"))
            if order != .orderedSame { return order == .orderedAscending }
            return a.id < b.id
        }
    }

    private static func trimmed(_ s: String?) -> String {
        (s ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

// MARK: - HAL

@available(macOS 14.2, *)
enum AudioDevices {
    /// Tous les objets périphériques du HAL.
    static func allDeviceIDs() -> [AudioDeviceID] {
        (try? HAL.readArray(HAL.systemObject, kAudioHardwarePropertyDevices, placeholder: HAL.unknownObject)) ?? []
    }

    static func rawDevice(_ device: AudioDeviceID) -> RawAudioDevice {
        RawAudioDevice(
            objectID: device,
            uid: HAL.readString(device, kAudioDevicePropertyDeviceUID),
            name: HAL.readString(device, kAudioObjectPropertyName),
            transportType: (try? HAL.read(device, kAudioDevicePropertyTransportType, default: UInt32(0))) ?? 0,
            inputChannels: channelCount(device, scope: kAudioObjectPropertyScopeInput),
            outputChannels: channelCount(device, scope: kAudioObjectPropertyScopeOutput)
        )
    }

    static func rawDevices() -> [RawAudioDevice] {
        allDeviceIDs().map(rawDevice)
    }

    /// Contenu du message `devices`.
    static func snapshot() -> AudioDeviceList {
        AudioDeviceCatalog.build(rawDevices(),
                                 defaultInput: defaultInputDeviceID(),
                                 defaultOutput: defaultOutputDeviceID())
    }

    static func defaultInputDeviceID() -> AudioDeviceID? {
        defaultDevice(kAudioHardwarePropertyDefaultInputDevice)
    }

    static func defaultOutputDeviceID() -> AudioDeviceID? {
        defaultDevice(kAudioHardwarePropertyDefaultOutputDevice)
    }

    /// Micro dont l'UID est `uid` (nil s'il est absent, sans canal d'entrée ou privé).
    static func inputDeviceID(forUID uid: String) -> AudioDeviceID? {
        findDevice(uid: uid) { $0.inputChannels > 0 }
    }

    /// Sortie dont l'UID est `uid` (nil si elle est absente, sans canal de sortie ou privée).
    static func outputDeviceID(forUID uid: String) -> AudioDeviceID? {
        findDevice(uid: uid) { $0.outputChannels > 0 }
    }

    /// Nom et UID lisibles pour les journaux.
    static func describe(_ device: AudioDeviceID) -> String {
        let name = HAL.readString(device, kAudioObjectPropertyName) ?? "?"
        let uid = HAL.readString(device, kAudioDevicePropertyDeviceUID) ?? "?"
        return "« \(name) » (\(uid))"
    }

    /// Nombre total de canaux d'un sens (`kAudioDevicePropertyStreamConfiguration`).
    static func channelCount(_ device: AudioDeviceID, scope: AudioObjectPropertyScope) -> Int {
        var addr = HAL.address(kAudioDevicePropertyStreamConfiguration, scope: scope)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(device, &addr, 0, nil, &size) == noErr, size > 0 else { return 0 }
        let raw = UnsafeMutableRawPointer.allocate(byteCount: Int(size),
                                                   alignment: MemoryLayout<AudioBufferList>.alignment)
        defer { raw.deallocate() }
        guard AudioObjectGetPropertyData(device, &addr, 0, nil, &size, raw) == noErr else { return 0 }
        let list = UnsafeMutableAudioBufferListPointer(raw.assumingMemoryBound(to: AudioBufferList.self))
        return list.reduce(0) { $0 + Int($1.mNumberChannels) }
    }

    // MARK: Nœuds AVAudioEngine

    /// Règle le périphérique d'un nœud d'entrée ou de sortie (`kAudioOutputUnitProperty_CurrentDevice`
    /// sur l'`AudioUnit` du nœud). À appeler avant `prepare()` / `start()`.
    /// À VÉRIFIER (spike S1) : effet sur `inputNode` avec et sans voice processing, et sur
    /// `outputNode` d'un moteur sans entrée.
    static func setCurrentDevice(_ device: AudioDeviceID, on node: AVAudioIONode) throws {
        guard let unit = node.audioUnit else {
            throw CoreAudioError(status: OSStatus(kAudioHardwareUnspecifiedError), operation: "AudioUnit du nœud")
        }
        var id = device
        let status = AudioUnitSetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0,
                                          &id, UInt32(MemoryLayout<AudioDeviceID>.size))
        guard status == noErr else {
            throw CoreAudioError(status: status, operation: "sélection du périphérique \(device)")
        }
    }

    /// Périphérique actuellement réglé sur le nœud (nil si illisible).
    static func currentDevice(of node: AVAudioIONode) -> AudioDeviceID? {
        guard let unit = node.audioUnit else { return nil }
        var id = HAL.unknownObject
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = AudioUnitGetProperty(unit, kAudioOutputUnitProperty_CurrentDevice, kAudioUnitScope_Global, 0,
                                          &id, &size)
        guard status == noErr, id != HAL.unknownObject else { return nil }
        return id
    }

    // MARK: Interne

    private static func defaultDevice(_ selector: AudioObjectPropertySelector) -> AudioDeviceID? {
        guard let id = try? HAL.read(HAL.systemObject, selector, default: HAL.unknownObject),
              id != HAL.unknownObject
        else { return nil }
        return id
    }

    private static func findDevice(uid: String, where accept: (RawAudioDevice) -> Bool) -> AudioDeviceID? {
        let wanted = uid.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !wanted.isEmpty else { return nil }
        for id in allDeviceIDs() {
            guard HAL.readString(id, kAudioDevicePropertyDeviceUID) == wanted else { continue }
            let raw = rawDevice(id)
            if AudioDeviceCatalog.exclusionReason(raw) == nil, accept(raw) { return id }
        }
        return nil
    }
}

// MARK: - Écouteurs

/// Surveille la liste des périphériques et les périphériques par défaut ; appelle `onChange`
/// (sur sa file) après un anti-rebond, seulement si le contenu de `devices` a changé (la
/// création de l'agrégat privé du tap, par exemple, modifie la liste HAL sans changer le message).
@available(macOS 14.2, *)
final class AudioDeviceWatcher {
    var onChange: ((AudioDeviceList) -> Void)?

    static let debounce: TimeInterval = 0.3

    private let queue = DispatchQueue(label: "scribe.audio.devices")
    private var listeners: [HALPropertyListener] = []
    private var pending: DispatchWorkItem?
    private var lastSent: AudioDeviceList?

    func start() {
        queue.sync {
            guard listeners.isEmpty else { return }
            for selector in [kAudioHardwarePropertyDevices,
                             kAudioHardwarePropertyDefaultInputDevice,
                             kAudioHardwarePropertyDefaultOutputDevice]
            {
                if let l = HALPropertyListener(selector: selector, queue: queue, handler: { [weak self] in
                    self?.changed()
                }) {
                    listeners.append(l)
                }
            }
        }
    }

    func stop() {
        queue.sync {
            pending?.cancel()
            pending = nil
            listeners.removeAll()
        }
    }

    /// Relevé immédiat (après `ready`, commande `devices`) ; sert aussi de référence pour
    /// l'anti-doublon des émissions suivantes.
    func current() -> AudioDeviceList {
        queue.sync {
            let list = AudioDevices.snapshot()
            lastSent = list
            return list
        }
    }

    /// Sur `queue` (les écouteurs HAL y sont rattachés).
    private func changed() {
        pending?.cancel()
        let item = DispatchWorkItem { [weak self] in
            guard let self else { return }
            let list = AudioDevices.snapshot()
            guard list != self.lastSent else { return }
            self.lastSent = list
            Log.info("périphériques : \(list.inputs.count) entrée(s), \(list.outputs.count) sortie(s)")
            self.onChange?(list)
        }
        pending = item
        queue.asyncAfter(deadline: .now() + Self.debounce, execute: item)
    }
}
