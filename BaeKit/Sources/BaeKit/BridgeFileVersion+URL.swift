import BaeBridge
import Foundation

extension BridgeFileVersion {
    /// The file URL this version's path bytes name.
    public var url: URL { URL(filePathBytes: path, isDirectory: false) }
}
