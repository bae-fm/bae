import BaeKit

extension BridgeStorageInspectorTransfer {
    /// Which transfer the entry is: a release has at most one entry in each
    /// queue, and at most one fetch by coven of the copies its pins wait for.
    var queueId: Int {
        switch self {
        case .download: 0
        case .pinFetch: 1
        case .output: 2
        case .upload: 3
        }
    }
}
