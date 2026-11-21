package fm.bae.app.shots

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.albumdetail.AndroidPinControl
import fm.bae.app.ui.albumdetail.AndroidUnplayableLine
import fm.bae.app.ui.appearance.ThemeSpace
import org.junit.Test
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg
import uniffi.bae_bridge.BridgeReleaseDownloadStatus

/** The component shots of `AndroidPinControl`, one for each row of its preview. */
class AndroidPinControlShots : BoardShot() {
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
}

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
