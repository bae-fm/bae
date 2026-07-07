import Foundation

/// What the album-detail download control shows for one release, joined from
/// the release summary (pinned / available actions) and the download-queue
/// snapshot (this release's in-flight entry). `nil` means no control at all
/// (no cloud home, or a Local release — neither occurs on a playback-only
/// device in practice, but core owns that decision via `storageActions`).
enum ReleaseDownloadStatus: Equatable {
    case downloaded
    case queued
    case downloading(BridgeDownloadTransferProgress)
    case failed(String)
    case available

    /// A queue entry wins over `pinned`: on pin success core emits the release
    /// invalidation (flipping `pinned`) and the queue-drop snapshot in either
    /// order, so honoring the live queue entry keeps the control showing Cancel
    /// until the download actually leaves the queue. Only `.pin` is inspected —
    /// `.makeLocal`/`.makeRemote` are desktop transitions this playback-only
    /// control ignores, and unpin is driven by `pinned`, matching core's own
    /// gating where `Unpin` appears exactly when `pinned`.
    init?(
        pinned: Bool,
        storageActions: [BridgeReleaseStorageAction],
        queueState: BridgeDownloadState?
    ) {
        switch queueState {
        case .queued:
            self = .queued
        case .active(let progress):
            self = .downloading(progress)
        case .failed(let error):
            self = .failed(error)
        case nil:
            if pinned {
                self = .downloaded
            }
            else if storageActions.contains(.pin) {
                self = .available
            }
            else {
                return nil
            }
        }
    }
}

extension BridgeDownloadSnapshot {
    /// This release's queue entry state, or nil when it isn't queued.
    func state(forRelease releaseId: String) -> BridgeDownloadState? {
        downloads.first { $0.releaseId == releaseId }?.state
    }
}
