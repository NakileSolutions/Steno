// Accès aux propriétés du HAL Core Audio (objets processus, périphériques, taps).

import AudioToolbox
import CoreAudio
import Darwin
import Foundation

struct CoreAudioError: Error, CustomStringConvertible {
    let status: OSStatus
    let operation: String

    var description: String { "\(operation) a échoué (OSStatus \(status) « \(HAL.fourCC(status)) »)" }
}

@available(macOS 14.2, *)
enum HAL {
    static let systemObject = AudioObjectID(kAudioObjectSystemObject)
    static let unknownObject = AudioObjectID(kAudioObjectUnknown)

    static func address(_ selector: AudioObjectPropertySelector,
                        scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal,
                        element: AudioObjectPropertyElement = kAudioObjectPropertyElementMain)
        -> AudioObjectPropertyAddress
    {
        AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: element)
    }

    // MARK: Lecture générique

    static func read<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector,
                        default defaultValue: T) throws -> T
    {
        var addr = address(selector)
        var value = defaultValue
        var size = UInt32(MemoryLayout<T>.size)
        let status = withUnsafeMutablePointer(to: &value) { ptr in
            AudioObjectGetPropertyData(object, &addr, 0, nil, &size, ptr)
        }
        guard status == noErr else { throw CoreAudioError(status: status, operation: "lecture '\(fourCC(OSStatus(bitPattern: selector)))'") }
        return value
    }

    static func readArray<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector,
                             placeholder: T) throws -> [T]
    {
        var addr = address(selector)
        var size: UInt32 = 0
        var status = AudioObjectGetPropertyDataSize(object, &addr, 0, nil, &size)
        guard status == noErr else { throw CoreAudioError(status: status, operation: "taille '\(fourCC(OSStatus(bitPattern: selector)))'") }
        let count = Int(size) / MemoryLayout<T>.stride
        guard count > 0 else { return [] }
        var values = [T](repeating: placeholder, count: count)
        status = values.withUnsafeMutableBytes { raw in
            AudioObjectGetPropertyData(object, &addr, 0, nil, &size, raw.baseAddress!)
        }
        guard status == noErr else { throw CoreAudioError(status: status, operation: "lecture '\(fourCC(OSStatus(bitPattern: selector)))'") }
        return Array(values.prefix(Int(size) / MemoryLayout<T>.stride))
    }

    /// Propriété CFString (la référence renvoyée par le HAL est à libérer : takeRetainedValue).
    static func readString(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> String? {
        var addr = address(selector)
        var ref: Unmanaged<CFString>?
        var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        let status = AudioObjectGetPropertyData(object, &addr, 0, nil, &size, &ref)
        guard status == noErr, let ref else { return nil }
        return ref.takeRetainedValue() as String
    }

    // MARK: Processus

    static func processObjectList() throws -> [AudioObjectID] {
        try readArray(systemObject, kAudioHardwarePropertyProcessObjectList, placeholder: unknownObject)
    }

    static func processPID(_ process: AudioObjectID) -> pid_t? {
        guard let pid = try? read(process, kAudioProcessPropertyPID, default: pid_t(-1)), pid > 0 else {
            return nil
        }
        return pid
    }

    static func processBundleID(_ process: AudioObjectID) -> String? {
        guard let s = readString(process, kAudioProcessPropertyBundleID), !s.isEmpty else { return nil }
        return s
    }

    static func processIsRunningInput(_ process: AudioObjectID) -> Bool {
        ((try? read(process, kAudioProcessPropertyIsRunningInput, default: UInt32(0))) ?? 0) != 0
    }

    static func processIsRunningOutput(_ process: AudioObjectID) -> Bool {
        ((try? read(process, kAudioProcessPropertyIsRunningOutput, default: UInt32(0))) ?? 0) != 0
    }

    /// Objet processus HAL correspondant à un PID (nil si le processus n'a pas de client audio).
    static func processObject(forPID pid: pid_t) -> AudioObjectID? {
        var addr = address(kAudioHardwarePropertyTranslatePIDToProcessObject)
        var qualifier = pid
        var object = unknownObject
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        let status = AudioObjectGetPropertyData(systemObject, &addr,
                                                UInt32(MemoryLayout<pid_t>.size), &qualifier,
                                                &size, &object)
        guard status == noErr, object != unknownObject else { return nil }
        return object
    }

    // MARK: Périphériques et taps

    static func defaultSystemOutputDevice() throws -> AudioDeviceID {
        let id = try read(systemObject, kAudioHardwarePropertyDefaultSystemOutputDevice, default: unknownObject)
        guard id != unknownObject else {
            throw CoreAudioError(status: OSStatus(kAudioHardwareBadDeviceError), operation: "sortie système par défaut")
        }
        return id
    }

    static func deviceUID(_ device: AudioDeviceID) throws -> String {
        guard let uid = readString(device, kAudioDevicePropertyDeviceUID) else {
            throw CoreAudioError(status: OSStatus(kAudioHardwareUnknownPropertyError), operation: "UID du périphérique \(device)")
        }
        return uid
    }

    static func tapFormat(_ tap: AudioObjectID) throws -> AudioStreamBasicDescription {
        try read(tap, kAudioTapPropertyFormat, default: AudioStreamBasicDescription())
    }

    // MARK: Divers

    static func fourCC(_ status: OSStatus) -> String {
        let v = UInt32(bitPattern: status)
        let bytes = [UInt8((v >> 24) & 0xFF), UInt8((v >> 16) & 0xFF), UInt8((v >> 8) & 0xFF), UInt8(v & 0xFF)]
        guard bytes.allSatisfy({ $0 >= 32 && $0 < 127 }) else { return String(status) }
        return String(decoding: bytes, as: UTF8.self)
    }
}

// MARK: - Processus BSD

enum ProcessInfoBSD {
    /// PID du parent (libproc), nil si inaccessible.
    /// À VÉRIFIER : `proc_pidinfo` / `PROC_PIDTBSDINFO` visibles via `import Darwin`.
    static func parentPID(of pid: pid_t) -> pid_t? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        let r = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size)
        guard r == size else { return nil }
        return pid_t(info.pbi_ppid)
    }
}

// MARK: - Écouteur de propriété HAL

/// Abonnement à une propriété HAL ; désabonnement automatique à la libération.
@available(macOS 14.2, *)
final class HALPropertyListener {
    private let object: AudioObjectID
    private var addr: AudioObjectPropertyAddress
    private let queue: DispatchQueue
    private let block: AudioObjectPropertyListenerBlock

    init?(object: AudioObjectID = HAL.systemObject,
          selector: AudioObjectPropertySelector,
          queue: DispatchQueue,
          handler: @escaping () -> Void)
    {
        self.object = object
        self.addr = HAL.address(selector)
        self.queue = queue
        self.block = { _, _ in handler() }
        let status = AudioObjectAddPropertyListenerBlock(object, &addr, queue, block)
        if status != noErr {
            Log.warn("écoute HAL '\(HAL.fourCC(OSStatus(bitPattern: selector)))' impossible : \(status)")
            return nil
        }
    }

    deinit {
        _ = AudioObjectRemovePropertyListenerBlock(object, &addr, queue, block)
    }
}
