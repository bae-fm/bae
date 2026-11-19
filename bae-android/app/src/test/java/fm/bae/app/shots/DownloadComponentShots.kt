package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidPinControl
import fm.bae.app.ui.albumdetail.AndroidUnplayableLine
import fm.bae.app.ui.albumdetail.AndroidUploadLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.downloads.AndroidPinningBar
import fm.bae.app.ui.downloads.AndroidPinningRow
import org.junit.Test
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg
import uniffi.bae_bridge.BridgeReleaseDownloadStatus

/** The component shots of pinning: the release's download control and upload line, the downloads list's rows and bar. */
class DownloadComponentShots : BoardShot() {
    @Test
    fun androidComponentPinControlAvailable() =
        component("android.component.pin-control.available") { Control(BridgeReleaseDownloadStatus.Available) }

    @Test
    fun androidComponentPinControlQueued() =
        component("android.component.pin-control.queued") { Control(BridgeReleaseDownloadStatus.Queued) }

    @Test
    fun androidComponentPinControlPinning() =
        component("android.component.pin-control.pinning", wide = true) {
            Control(BridgeReleaseDownloadStatus.Downloading)
        }

    @Test
    fun androidComponentPinControlFailed() =
        component("android.component.pin-control.failed", wide = true) {
            Control(BridgeReleaseDownloadStatus.Failed("Network unreachable"))
        }

    @Test
    fun androidComponentPinControlPinned() =
        component("android.component.pin-control.pinned") { Control(BridgeReleaseDownloadStatus.Downloaded) }

    @Test
    fun androidComponentPinControlElsewhere() =
        component("android.component.pin-control.elsewhere") {
            AndroidUnplayableLine(BridgeMessage("core.release.on_device", listOf(BridgeMessageArg("device", "Ana’s Mac"))))
        }

    @Test
    fun androidComponentPinningRowQueued() =
        component("android.component.pinning-row.queued", wide = true) {
            PinningRow(previewDownload("rel-1", "Winter Kitchen", 10UL, 331_000_000UL, BridgeDownloadState.Queued))
        }

    @Test
    fun androidComponentPinningRowPinning() =
        component("android.component.pinning-row.pinning", wide = true) {
            PinningRow(previewDownload("rel-2", "Late Arrivals", 10UL, 344_000_000UL, BridgeDownloadState.Active))
        }

    @Test
    fun androidComponentPinningRowFailed() =
        component("android.component.pinning-row.failed", wide = true) {
            PinningRow(
                previewDownload("rel-3", "Glass Harbor", 9UL, 356_000_000UL, BridgeDownloadState.Failed("Network unreachable")),
            )
        }

    @Test
    fun androidComponentPinningRowReleaseRemoved() =
        component("android.component.pinning-row.release-removed", wide = true) {
            PinningRow(previewDownload("rel-4", null, 9UL, 356_000_000UL, BridgeDownloadState.Queued))
        }

    @Test
    fun androidComponentPinningBarPinning() =
        component(
            "android.component.pinning-bar.pinning",
            wide = true,
        ) { PinningBar(paused = false, hasDownloads = true, hasFailures = false) }

    @Test
    fun androidComponentPinningBarFailed() =
        component(
            "android.component.pinning-bar.failed",
            wide = true,
        ) { PinningBar(paused = false, hasDownloads = true, hasFailures = true) }

    @Test
    fun androidComponentPinningBarPaused() =
        component(
            "android.component.pinning-bar.paused",
            wide = true,
        ) { PinningBar(paused = true, hasDownloads = true, hasFailures = false) }

    @Test
    fun androidComponentPinningBarNothingToPin() =
        component("android.component.pinning-bar.nothing-to-pin", wide = true) {
            PinningBar(paused = false, hasDownloads = false, hasFailures = false)
        }

    @Test
    fun androidComponentUploadLineQueued() =
        component("android.component.upload-line.queued", wide = true) {
            AndroidUploadLine("Queued to upload", null)
        }

    @Test
    fun androidComponentUploadLineWaitingFor01HarborLightsFlac() =
        component("android.component.upload-line.waiting-for-01-harbor-lights-flac", wide = true) {
            AndroidUploadLine("Queued to upload", "01 Harbor Lights.flac")
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
private fun Control(status: BridgeReleaseDownloadStatus) =
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
        AndroidPinControl(
            status = status,
            progress = PreviewData.downloadStore(downloadingReleaseId = "rel-1").latestProgress("rel-1"),
            unpinning = false,
            asksBeforeUnpinning = true,
            onDownload = {},
            onCancel = {},
            onRetry = {},
            onUnpin = {},
        )
    }

@Composable
private fun PinningRow(op: BridgeDownloadOp) =
    AndroidPinningRow(
        op,
        PreviewData.downloadStore(downloadingReleaseId = op.releaseId).latestProgress(op.releaseId),
        onCancel = {},
        onRetry = {},
    )

@Composable
private fun PinningBar(
    paused: Boolean,
    hasDownloads: Boolean,
    hasFailures: Boolean,
) = AndroidPinningBar(
    paused = paused,
    hasDownloads = hasDownloads,
    hasFailures = hasFailures,
    onBack = {},
    onPauseToggle = {},
    onRetry = {},
)
