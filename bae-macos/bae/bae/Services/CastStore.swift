import BaeKit
import Observation

/// Cast state for the playback bar: the current cast status (which device, if
/// any) and the discovered device list. The status is driven by the
/// `castStatusChanged` UI event (so it follows a receiver-side end too); the
/// device list by the `castDevices` projection while the picker is open.
@MainActor
@Observable
final class CastStore {
    var status: BridgeCastStatus = .notCasting
    var devices: [BridgeCastDevice] = []

    /// The device name when casting, else `nil` — drives the cast button's
    /// active state and the "Casting to …" row.
    var castingDeviceName: String? {
        if case .casting(let deviceName) = status {
            return deviceName
        }
        return nil
    }

    /// Apply a `castStatusChanged` event: `Some(name)` while casting, `nil` back
    /// on local output.
    func applyStatus(deviceName: String?) {
        status = deviceName.map { .casting(deviceName: $0) } ?? .notCasting
    }
}
