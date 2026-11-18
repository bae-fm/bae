import Foundation

extension BridgeAirPlayCodeRequest {
    /// The line under the pairing sheet's title: what to enter, and why bae
    /// asks, naming `receiver` where core's line does.
    public func localizedMessage(receiver: String) -> String {
        localizedCoreString(
            bridgeAirplayCodeMessageKey(request: self),
            receiver
        )
    }

    /// The red line under the field, saying what was wrong with the last
    /// code, or `nil` when nothing was.
    public var localizedFieldLine: String? {
        guard let key = bridgeAirplayCodeFieldKey(request: self) else {
            return nil
        }
        return localizedCoreString(key, Int(digits ?? 0))
    }

    /// How many digits the PIN has, or `nil` for a password.
    public var digits: UInt8? {
        if case .pin(let digits) = code { return digits }
        return nil
    }
}
