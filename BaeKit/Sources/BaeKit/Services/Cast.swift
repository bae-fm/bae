import Foundation
import Observation

/// Cast transport for the playback surfaces: device discovery (run only while
/// the picker or the Casting settings page is open) and switching playback to
/// or from a device. Reads flow through the `CastStore`; this is the write side.
public final class Cast: Sendable, Observable {
    /// Browse for devices for `asker`, the place that opened.
    public let startDiscovery: @Sendable (_ asker: BridgeBrowsingFor) -> Void
    /// `asker` closed. Browsing ends with the last place open.
    public let stopDiscovery: @Sendable (_ asker: BridgeBrowsingFor) -> Void
    /// Switch playback to the device with this id, asking `prompt` for the
    /// code an AirPlay receiver wants.
    public let castTo:
        @Sendable (
            _ deviceId: String,
            _ prompt: any AirPlayCodePromptCallback
        ) async throws -> Void
    /// Stop playing on the receiver and return playback to local output.
    public let stopCasting: @Sendable () -> Void
    /// Whether playing on other devices is available at all. Turning it off
    /// stops discovery and ends a session in flight — core does both off the
    /// write, so the settings switch only has to make this call.
    public let setEnabled: @Sendable (_ enabled: Bool) async throws -> Void
    /// What a track a device can't play directly is converted to. Core reads
    /// it as each track is loaded onto a device.
    public let setTranscodeFormat:
        @Sendable (_ format: BridgeCastTranscodeFormat) async throws -> Void

    public init(
        startDiscovery: @escaping @Sendable (BridgeBrowsingFor) -> Void = { _ in
        },
        stopDiscovery: @escaping @Sendable (BridgeBrowsingFor) -> Void = { _ in
        },
        castTo:
            @escaping @Sendable (String, any AirPlayCodePromptCallback)
            async throws -> Void = { _, _ in },
        stopCasting: @escaping @Sendable () -> Void = {},
        setEnabled: @escaping @Sendable (Bool) async throws -> Void = { _ in },
        setTranscodeFormat:
            @escaping @Sendable (BridgeCastTranscodeFormat) async throws -> Void =
            { _ in }
    ) {
        self.startDiscovery = startDiscovery
        self.stopDiscovery = stopDiscovery
        self.castTo = castTo
        self.stopCasting = stopCasting
        self.setEnabled = setEnabled
        self.setTranscodeFormat = setTranscodeFormat
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            startDiscovery: { handle.startCastDiscovery(asker: $0) },
            stopDiscovery: { handle.stopCastDiscovery(asker: $0) },
            castTo: { try await handle.castTo(deviceId: $0, prompt: $1) },
            stopCasting: { handle.stopCasting() },
            setEnabled: { try await handle.setCastEnabled(enabled: $0) },
            setTranscodeFormat: {
                try await handle.setCastTranscodeFormat(format: $0)
            }
        )
    }

    /// Turning Play on other devices off while playing ends it, so that one
    /// case asks first; every other flip writes straight through. One rule for
    /// every platform's settings surface, which only renders what it returns.
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

/// What flipping the Play on other devices switch should do.
public enum CastToggleAction: Equatable {
    /// Write the setting straight through.
    case apply(Bool)
    /// Turning it off would end playback on this receiver — ask first.
    case confirmDisconnect(device: String)
}
