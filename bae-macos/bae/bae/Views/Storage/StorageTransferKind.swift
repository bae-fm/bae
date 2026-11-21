import Foundation

/// The two ways a release's files move, which the Storage Manager names
/// wherever it shows one under way. Both draw in the person's accent.
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
}
