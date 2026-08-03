import BaeKit
import Foundation

/// Cast transport for the playback bar: device discovery (run only while the
/// picker is open) and switching playback to or from a device. Reads flow
/// through the `CastStore`; this is the write side.
final class Cast: Sendable, Observable {
    /// Begin browsing for devices (the picker opened).
    let startDiscovery: @Sendable () -> Void
    /// Stop browsing for devices (the picker closed).
    let stopDiscovery: @Sendable () -> Void
    /// Switch playback to the device with this id.
    let castTo: @Sendable (_ deviceId: String) throws -> Void
    /// Stop casting and return playback to local output.
    let stopCasting: @Sendable () -> Void
    /// Whether casting is available at all. Turning it off is what stops
    /// discovery and ends a session in flight — core does both off the write, so
    /// the settings toggle only has to make this call.
    let setEnabled: @Sendable (_ enabled: Bool) throws -> Void

    init(
        startDiscovery: @escaping @Sendable () -> Void = {},
        stopDiscovery: @escaping @Sendable () -> Void = {},
        castTo: @escaping @Sendable (String) throws -> Void = { _ in },
        stopCasting: @escaping @Sendable () -> Void = {},
        setEnabled: @escaping @Sendable (Bool) throws -> Void = { _ in }
    ) {
        self.startDiscovery = startDiscovery
        self.stopDiscovery = stopDiscovery
        self.castTo = castTo
        self.stopCasting = stopCasting
        self.setEnabled = setEnabled
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            startDiscovery: { handle.startCastDiscovery() },
            stopDiscovery: { handle.stopCastDiscovery() },
            castTo: { try handle.castTo(deviceId: $0) },
            stopCasting: { handle.stopCasting() },
            setEnabled: { try handle.setCastEnabled(enabled: $0) }
        )
    }

    #if DEBUG
        // periphery:ignore
        static let stub = Cast()
    #endif
}
