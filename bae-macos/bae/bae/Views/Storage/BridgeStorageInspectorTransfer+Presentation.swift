import BaeKit

extension BridgeStorageInspectorTransfer {
    /// Which transfer the entry is: a release has at most one entry in each
    /// queue.
    var queueId: Int {
        switch self {
        case .download: 0
        case .output: 1
        case .upload: 2
        }
    }
}
