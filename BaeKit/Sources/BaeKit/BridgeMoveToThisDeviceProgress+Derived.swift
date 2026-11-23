import Foundation

extension BridgeMoveToThisDeviceProgress {
    /// "3 MB of 221.2 MB" of the whole release.
    public var bytesText: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }

    /// "3 MB of 10 MB" of the file being downloaded.
    public var fileBytesText: String? {
        file.map {
            formattedDownloadBytesProgress(
                done: $0.bytesDone,
                total: $0.bytesTotal
            )
        }
    }
}
