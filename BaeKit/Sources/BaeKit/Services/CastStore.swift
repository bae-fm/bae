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

    /// The device playback is on, else `nil`. The picker marks the device whose
    /// id matches; device names need not be unique.
    public var castingDevice: BridgeRemoteDevice? {
        if case .casting(let device) = status {
            return device
        }
        return nil
    }

    /// The casting device's name, for the cast button's active state and the
    /// "Casting to …" row.
    public var castingDeviceName: String? {
        castingDevice?.name
    }

    /// Apply the retained playback value: the device while casting, `nil` back
    /// on local output.
    public func applyStatus(device: BridgeRemoteDevice?) {
        status = device.map { .casting(device: $0) } ?? .notCasting
    }
}
