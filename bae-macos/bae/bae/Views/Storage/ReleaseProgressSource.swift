import BaeKit

/// The latest progress of one release's transfer that a native view reads
/// each display frame: its upload, its download to be kept, or its download
/// into the person's folder as it moves to This Mac.
enum ReleaseProgressSource {
    case upload(LatestSource<BridgeReleaseUploadTransfer>)
    case pin(LatestSource<BridgeDownloadTransferProgress>)
    case makeLocal(LatestSource<BridgeMakeLocalProgress>)
}
