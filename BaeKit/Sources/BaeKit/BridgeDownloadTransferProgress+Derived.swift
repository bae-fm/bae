import Foundation

extension BridgeDownloadTransferProgress {
    public var bytesText: String {
        String(
            format: NSLocalizedString(
                "core.download.bytes_progress",
                tableName: "Core",
                bundle: .main,
                comment:
                    "Download progress as transferred bytes out of total bytes"
            ),
            bytesDone.formattedDownloadBytes,
            bytesTotal.formattedDownloadBytes
        )
    }
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
