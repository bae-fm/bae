package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidDownloadControl
import fm.bae.app.ui.albumdetail.AndroidUploadLine
import fm.bae.app.ui.albumdetail.UnplayableLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.downloads.AndroidDownloadRow
import fm.bae.app.ui.downloads.AndroidDownloadsBar
import fm.bae.fastvalues.LatestStream
import org.junit.Test
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg
import uniffi.bae_bridge.BridgeReleaseDownloadStatus

/** The component shots of pinning: the release's download control and upload line, the downloads list's rows and bar. */
class DownloadComponentShots : BoardShot() {
    @Test
    fun androidComponentDownloadControlAvailable() =
        component("android.component.download-control.available") { Control(BridgeReleaseDownloadStatus.Available) }

    @Test
    fun androidComponentDownloadControlQueued() =
        component("android.component.download-control.queued") { Control(BridgeReleaseDownloadStatus.Queued) }

    @Test
    fun androidComponentDownloadControlDownloading() =
        component("android.component.download-control.downloading", wide = true) {
            Control(BridgeReleaseDownloadStatus.Downloading)
        }

    @Test
    fun androidComponentDownloadControlFailed() =
        component("android.component.download-control.failed", wide = true) {
            Control(BridgeReleaseDownloadStatus.Failed("Network unreachable"))
        }

    @Test
    fun androidComponentDownloadControlDownloaded() =
        component("android.component.download-control.downloaded") { Control(BridgeReleaseDownloadStatus.Downloaded) }

    @Test
    fun androidComponentDownloadControlElsewhere() =
        component("android.component.download-control.elsewhere") {
            UnplayableLine(BridgeMessage("core.release.on_device", listOf(BridgeMessageArg("device", "Ana's Mac"))))
        }

    @Test
    fun androidComponentDownloadRowDefault() =
        component("android.component.download-row.default", wide = true) {
            Column {
                listOf(
                    PreviewData.downloadOp(releaseId = "rel-2", state = BridgeDownloadState.Active),
                    PreviewData.downloadOp(releaseId = "rel-1", state = BridgeDownloadState.Queued),
                    PreviewData.downloadOp(releaseId = "rel-3", state = BridgeDownloadState.Failed("Network unreachable")),
                ).forEach { op ->
                    AndroidDownloadRow(op, noProgress.source(op.releaseId), onCancel = {}, onRetry = {})
                }
            }
        }

    @Test
    fun androidComponentDownloadsBarDefault() =
        component("android.component.downloads-bar.default", wide = true) {
            AndroidDownloadsBar(
                paused = false,
                hasDownloads = true,
                hasFailures = true,
                onBack = {},
                onPauseToggle = {},
                onRetry = {},
            )
        }

    @Test
    fun androidComponentUploadLineQueued() =
        component("android.component.upload-line.queued", wide = true) {
            AndroidUploadLine("Queued to upload", null)
        }

    @Test
    fun androidComponentUploadLinePaused() =
        component("android.component.upload-line.paused", wide = true) {
            AndroidUploadLine("Paused", null)
        }

    @Test
    fun androidComponentUploadLineWaitingForNetwork() =
        component("android.component.upload-line.waiting-for-network", wide = true) { AndroidUploadLine("Waiting for network", null) }

    @Test
    fun androidComponentUploadLineWaitingForWiFi() =
        component("android.component.upload-line.waiting-for-wi-fi", wide = true) { AndroidUploadLine("Waiting for Wi-Fi", null) }

    @Test
    fun androidComponentUploadLineRetryingAt640Pm() =
        component("android.component.upload-line.retrying-at-6-40-pm", wide = true) { AndroidUploadLine("Retrying at 6:40 PM", null) }

    @Test
    fun androidComponentUploadLineWaitingFor01HarborLightsFlac() =
        component("android.component.upload-line.waiting-for-01-harbor-lights-flac", wide = true) {
            AndroidUploadLine("Queued to upload", "01 Harbor Lights.flac")
        }
}

/** Streams hold nothing, so no bar is drawn: a View following a stream that holds a value keeps the test from going idle. */
private val noProgress = LatestStream.downloads()

@Composable
private fun Control(status: BridgeReleaseDownloadStatus) =
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
        AndroidDownloadControl(
            status = status,
            progress = noProgress.source("rel-1"),
            unpinning = false,
            onDownload = {},
            onCancel = {},
            onRetry = {},
            onRemove = {},
        )
    }
