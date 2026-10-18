import Foundation

extension BridgeEagerCacheFillProgress {
    public var bytesText: String {
        formattedDownloadBytesProgress(done: bytesCached, total: bytesTotal)
    }
}
