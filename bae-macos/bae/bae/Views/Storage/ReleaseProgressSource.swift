import BaeKit

/// The latest progress of one release's transfer that a native view reads
/// each display frame: its upload, or its download to be kept.
enum ReleaseProgressSource {
    case upload(LatestSource<BridgeReleaseUploadTransfer>)
    case pin(LatestSource<BridgeDownloadTransferProgress>)
}
