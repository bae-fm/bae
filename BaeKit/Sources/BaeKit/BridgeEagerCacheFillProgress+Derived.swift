import Foundation

extension BridgeEagerCacheFillProgress {
    public var bytesText: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }
}
