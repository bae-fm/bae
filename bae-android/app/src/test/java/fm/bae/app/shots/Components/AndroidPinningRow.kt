package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.downloads.AndroidPinningRow
import org.junit.Test
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadState

/** The component shots of `AndroidPinningRow`, one for each row of its preview. */
class AndroidPinningRowShots : BoardShot() {
    @Test
    fun androidComponentPinningRowQueued() =
        component("android.component.pinning-row.queued") {
            PinningRow(previewDownload("rel-1", "Winter Kitchen", 10UL, 331_000_000UL, BridgeDownloadState.Queued))
        }

    @Test
    fun androidComponentPinningRowPinning() =
        component("android.component.pinning-row.pinning") {
            PinningRow(previewDownload("rel-2", "Late Arrivals", 10UL, 344_000_000UL, BridgeDownloadState.Active))
        }

    @Test
    fun androidComponentPinningRowFailed() =
        component("android.component.pinning-row.failed") {
            PinningRow(
                previewDownload("rel-3", "Glass Harbor", 9UL, 356_000_000UL, BridgeDownloadState.Failed("Network unreachable")),
            )
        }

    @Test
    fun androidComponentPinningRowReleaseRemoved() =
        component("android.component.pinning-row.release-removed") {
            PinningRow(previewDownload("rel-4", null, 9UL, 356_000_000UL, BridgeDownloadState.Queued))
        }
}

private fun previewDownload(
    releaseId: String,
    title: String?,
    files: ULong,
    bytes: ULong,
    state: BridgeDownloadState,
) = PreviewData.downloadOp(releaseId = releaseId, state = state, title = title, fileCount = files, totalSize = bytes)

@Composable
private fun PinningRow(op: BridgeDownloadOp) =
    AndroidPinningRow(
        op,
        PreviewData.downloadStore(downloadingReleaseId = op.releaseId).latestProgress(op.releaseId),
        onCancel = {},
        onRetry = {},
    )
