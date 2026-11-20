import Foundation

/// The time zone the device is in, which the app hands core so that a play's
/// day is the day the device's calendar showed when the play began.
final class DeviceTimeZone: BridgeLocalTimeZone, Sendable {
    func utcOffsetSeconds(unixMillis: Int64) -> Int32 {
        let at = Date(timeIntervalSince1970: TimeInterval(unixMillis) / 1000)
        return Int32(TimeZone.autoupdatingCurrent.secondsFromGMT(for: at))
    }
}

extension AppHandleProtocol {
    /// Tell core the time zone this device is in.
    public func tellDeviceTimeZone() {
        setLocalTimeZone(zone: DeviceTimeZone())
    }
}
