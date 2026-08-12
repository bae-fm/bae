import Observation

/// Cast state for the playback surfaces: the current cast status (which device,
/// if any) and the discovered device list. Retained playback values drive the
/// status, including receiver-side ends; the cast-device value stream drives the
/// device list while the picker is open.
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

    /// Apply the retained playback value: `Some(name)` while casting, `nil` back
    /// on local output.
    public func applyStatus(deviceName: String?) {
        status = deviceName.map { .casting(deviceName: $0) } ?? .notCasting
    }
}
