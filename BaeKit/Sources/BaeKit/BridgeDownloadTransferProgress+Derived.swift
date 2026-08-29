import Foundation

extension BridgeDownloadTransferProgress {
    public var bytesText: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }
}

func formattedDownloadBytesProgress(done: UInt64, total: UInt64) -> String {
    String(
        format: NSLocalizedString(
            "core.download.bytes_progress",
            tableName: "Core",
            bundle: .module,
            comment: "Transferred bytes out of total bytes"
        ),
        done.formattedDownloadBytes,
        total.formattedDownloadBytes
    )
}

extension UInt64 {
    var formattedDownloadBytes: String {
        precondition(
            self <= UInt64(Int64.max),
            "download byte count exceeds display range"
        )
        return Int64(self).formatted(.byteCount(style: .file))
    }
}
