import BaeKit
import SwiftUI

extension BridgeSubsonicNotRunning {
    /// Why the server isn't running, as a line of the alert the Activity
    /// panel's row opens.
    func line(names: NetworkNames) -> String {
        switch self {
        case .missingUsername:
            String(localized: "The server needs a username to start.")
        case .missingPassword:
            String(localized: "The server needs a password to start.")
        case .portOutOfRange:
            String(localized: "Use a number from 1 to 65535")
        case .portTaken:
            String(localized: "Another app is using this port")
        case .networkGone(let interface):
            String(localized: "\(names.name(of: interface)) is not connected")
        case .failed(let detail):
            detail
        }
    }
}

extension BridgeNotRunningTone {
    /// The color the server not running reads in.
    var statusTone: StatusTone {
        switch self {
        case .warning: .warning
        case .danger: .danger
        }
    }
}
