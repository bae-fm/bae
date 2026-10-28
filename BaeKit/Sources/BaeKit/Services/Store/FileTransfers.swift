import Observation

/// One file's share moved, observed on its own: bytes moving change only
/// this value, so only a view that draws that file redraws.
@Observable
public final class FileTransferValue {
    /// The share of the file's object moved, from 0 to 1.
    public internal(set) var fraction: Double

    init(_ fraction: Double) {
        self.fraction = fraction
    }
}

/// How far each file in flight one way has got, by file id, as a file
/// transfer subscription lands it. The map changes only as files start and
/// end; each file's share moves in its own value.
@Observable
public final class FileTransfers {
    private var files: [String: FileTransferValue] = [:]

    public init() {}

    /// Apply one change the subscription delivered.
    public func apply(_ change: BridgeFileTransferChange) {
        switch change {
        case .updated(let fileId, let fraction):
            if let value = files[fileId] {
                value.fraction = fraction
            }
            else {
                files[fileId] = FileTransferValue(fraction)
            }
        case .removed(let fileId):
            files.removeValue(forKey: fileId)
        case .reset(let keyed):
            files = Dictionary(
                uniqueKeysWithValues: keyed.map {
                    ($0.fileId, FileTransferValue($0.fraction))
                }
            )
        }
    }

    /// The share of a file moved; `nil` while nothing of it is in flight.
    public func fraction(forFile fileId: String) -> Double? {
        files[fileId]?.fraction
    }
}
