import Foundation

extension BridgeMoveToThisDeviceProgress {
    /// How far the whole release has come, as a 0...1 fraction.
    public var fraction: Double {
        guard bytesTotal > 0 else { return 0 }
        return Double(bytesDone) / Double(bytesTotal)
    }

    /// "3 MB of 221.2 MB" of the whole release.
    public var bytesText: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }

    /// "3 MB of 10 MB" of the file being downloaded.
    public var fileBytesText: String {
        formattedDownloadBytesProgress(
            done: fileBytesDone,
            total: fileBytesTotal
        )
    }
}
