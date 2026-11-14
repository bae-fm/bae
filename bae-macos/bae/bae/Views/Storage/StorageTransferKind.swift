import BaeKit
import SwiftUI

/// The two ways a release's files move, each drawn in its own colour
/// wherever the Storage Manager shows one under way.
enum StorageTransferKind {
    /// Files going up to the cloud.
    case upload
    /// Files coming down to stay on this device.
    case pin

    /// What the transfer is doing, in words: the name of its bar.
    var progressName: String {
        switch self {
        case .upload: String(localized: "Uploading")
        case .pin: String(localized: "Pinning")
        }
    }

    var color: Color {
        switch self {
        case .upload: Theme.activity
        case .pin: Theme.pinning
        }
    }
}
