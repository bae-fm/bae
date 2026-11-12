import Foundation

extension BridgeDownloadTransferProgress {
    public var bytesText: String {
        formattedDownloadBytesProgress(done: bytesDone, total: bytesTotal)
    }
}

func formattedDownloadBytesProgress(done: UInt64, total: UInt64) -> String {
    String(
        format: localizedCoreString("core.download.bytes_progress"),
        done.fileSizeText,
        total.fileSizeText
    )
}
