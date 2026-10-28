import BaeKit
import SwiftUI

/// The two ways a release's files move, each drawn in its own colour
/// wherever the Storage Manager shows one under way.
enum StorageTransferKind {
    /// Files going up to the cloud.
    case upload
    /// Files coming down to stay on this device.
    case pin

    var color: Color {
        switch self {
        case .upload: Theme.activity
        case .pin: Theme.pinning
        }
    }
}
