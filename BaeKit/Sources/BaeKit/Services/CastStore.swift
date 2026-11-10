import Foundation
import Observation

/// Cast state for the playback surfaces: where playback is going (this device,
/// a renderer, or back here after a renderer's link was lost) and the
/// receivers the picker lists. Retained playback values drive the status,
/// including receiver-side ends; the receivers' value stream drives the list,
/// the one cast to apart from the rest.
@MainActor
@Observable
public final class CastStore {
    public var status: BridgeRemotePlayback = .local
    public var receivers = BridgeCastReceiverList(casting: nil, found: [])

    public init() {}

    /// The device playback is on, else `nil`.
    public var castingDevice: BridgeCastReceiver? {
        if case .on(let device) = status {
            return device
        }
        return nil
    }

    /// The casting device's name, for the cast button's active state and the
    /// "Casting to …" row.
    public var castingDeviceName: String? {
        castingDevice?.name
    }

    /// Why the last device stopped playing, when its link was lost: the line
    /// names the device, the detail is core's reason for the disclosure.
    public var endedFailure: DisplayError? {
        guard case .ended(let device, let detail) = status else { return nil }
        return DisplayError(
            line: String(
                format: localizedCoreString("core.cast.ended"),
                locale: Locale.current,
                device.name
            ),
            detail: detail
        )
    }

    /// Apply the retained playback value.
    public func applyStatus(_ remote: BridgeRemotePlayback) {
        status = remote
    }
}

extension BridgeCastReceiverList {
    /// Whether the list holds no receiver: none cast to, and none found.
    public var isEmpty: Bool {
        casting == nil && found.isEmpty
    }
}
