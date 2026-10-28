import BaeKit

extension BridgeStorageInspectorTransfer {
    /// Which queue the entry is in: a release has at most one entry in each.
    var queueId: Int {
        switch self {
        case .download: 0
        case .output: 1
        case .upload: 2
        }
    }
}
