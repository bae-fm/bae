import BaeKit

/// The keyed stream of one release's transfer that a native view draws:
/// its upload, or its download to be kept.
enum ReleaseTransferStream {
    case upload(KeyedValueSource<BridgeReleaseUploadTransfer>)
    case pin(KeyedValueSource<BridgeDownloadTransferProgress>)
}
