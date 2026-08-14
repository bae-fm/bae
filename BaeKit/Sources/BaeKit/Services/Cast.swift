import Foundation
import Observation

/// Cast transport for the playback surfaces: device discovery (run only while
/// the picker is open) and switching playback to or from a device. Reads flow
/// through the `CastStore`; this is the write side.
public final class Cast: Sendable, Observable {
    /// Begin browsing for devices (the picker opened).
    public let startDiscovery: @Sendable () -> Void
    /// Stop browsing for devices (the picker closed).
    public let stopDiscovery: @Sendable () -> Void
    /// Switch playback to the device with this id.
    public let castTo: @Sendable (_ deviceId: String) async throws -> Void
    /// Stop casting and return playback to local output.
    public let stopCasting: @Sendable () -> Void
    /// Whether casting is available at all. Turning it off is what stops
    /// discovery and ends a session in flight — core does both off the write, so
    /// the settings toggle only has to make this call.
    public let setEnabled: @Sendable (_ enabled: Bool) throws -> Void

    public init(
        startDiscovery: @escaping @Sendable () -> Void = {},
        stopDiscovery: @escaping @Sendable () -> Void = {},
        castTo: @escaping @Sendable (String) async throws -> Void = { _ in },
        stopCasting: @escaping @Sendable () -> Void = {},
        setEnabled: @escaping @Sendable (Bool) throws -> Void = { _ in }
    ) {
        self.startDiscovery = startDiscovery
        self.stopDiscovery = stopDiscovery
        self.castTo = castTo
        self.stopCasting = stopCasting
        self.setEnabled = setEnabled
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            startDiscovery: { handle.startCastDiscovery() },
            stopDiscovery: { handle.stopCastDiscovery() },
            castTo: { try await handle.castTo(deviceId: $0) },
            stopCasting: { handle.stopCasting() },
            setEnabled: { try handle.setCastEnabled(enabled: $0) }
        )
    }

    /// Turning casting off mid-session ends it, so that one case asks first;
    /// every other flip writes straight through. One rule for every platform's
    /// settings surface, which only renders what it returns.
    public static func toggleAction(
        enabled: Bool,
        castingDeviceName: String?
    ) -> CastToggleAction {
        guard !enabled, let device = castingDeviceName else {
            return .apply(enabled)
        }
        return .confirmDisconnect(device: device)
    }

    #if DEBUG
        public static func stub() -> Cast { Cast() }
    #endif
}

/// What flipping the casting toggle should do.
public enum CastToggleAction: Equatable {
    /// Write the setting straight through.
    case apply(Bool)
    /// Turning casting off would end the session on this device — ask first.
    case confirmDisconnect(device: String)
}
