import Observation

/// Cast state for the playback surfaces: the current cast status (which device,
/// if any) and the discovered device list. The status is driven by the
/// `castStatusChanged` UI event (so it follows a receiver-side end too); the
/// device list by the `castDevices` projection while the picker is open.
@MainActor
@Observable
public final class CastStore {
    public var status: BridgeCastStatus = .notCasting
    public var devices: [BridgeCastDevice] = []

    public init() {}

    /// The device name when casting, else `nil` — drives the cast button's
    /// active state and the "Casting to …" row.
    public var castingDeviceName: String? {
        if case .casting(let deviceName) = status {
            return deviceName
        }
        return nil
    }

    /// Apply a `castStatusChanged` event: `Some(name)` while casting, `nil` back
    /// on local output.
    public func applyStatus(deviceName: String?) {
        status = deviceName.map { .casting(deviceName: $0) } ?? .notCasting
    }
}
