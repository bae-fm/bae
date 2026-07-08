package fm.bae.app.ui

import uniffi.bae_bridge.BridgeDownloadSnapshot
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeReleaseStorageAction

/**
 * What the album-detail download control shows for one release, joined from the
 * release (its `pinned` flag and available storage actions) and this release's
 * download-queue entry. A null result means no control at all — no cloud home,
 * or a local release — which core decides via `storageActions`.
 */
internal sealed interface ReleaseDownloadStatus {
    data object Downloaded : ReleaseDownloadStatus

    data object Queued : ReleaseDownloadStatus

    data class Downloading(
        val progress: BridgeDownloadTransferProgress,
    ) : ReleaseDownloadStatus

    data class Failed(
        val error: String,
    ) : ReleaseDownloadStatus

    data object Available : ReleaseDownloadStatus
}

/**
 * A live queue entry wins over `pinned`: on pin success core emits the release
 * invalidation (flipping `pinned`) and the queue-drop snapshot in either order,
 * so honoring the queue entry keeps the control showing Cancel until the
 * download actually leaves the queue. Only [BridgeReleaseStorageAction.PIN] is
 * inspected — MakeLocal/MakeRemote are desktop transitions this playback-only
 * control ignores, and unpin is driven by `pinned`, matching core's own gating
 * where Unpin appears exactly when `pinned`.
 */
internal fun releaseDownloadStatus(
    pinned: Boolean,
    storageActions: List<BridgeReleaseStorageAction>,
    queueState: BridgeDownloadState?,
): ReleaseDownloadStatus? =
    when (queueState) {
        BridgeDownloadState.Queued -> {
            ReleaseDownloadStatus.Queued
        }

        is BridgeDownloadState.Active -> {
            ReleaseDownloadStatus.Downloading(queueState.progress)
        }

        is BridgeDownloadState.Failed -> {
            ReleaseDownloadStatus.Failed(queueState.error)
        }

        null -> {
            when {
                pinned -> ReleaseDownloadStatus.Downloaded
                storageActions.contains(BridgeReleaseStorageAction.PIN) -> ReleaseDownloadStatus.Available
                else -> null
            }
        }
    }

/** This release's queue entry state, or null when it isn't queued. */
internal fun BridgeDownloadSnapshot.stateForRelease(releaseId: String): BridgeDownloadState? =
    downloads.firstOrNull { it.releaseId == releaseId }?.state
