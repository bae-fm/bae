package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.sync.AndroidTransferLine
import fm.bae.app.ui.sync.PausePinning
import fm.bae.app.ui.sync.TransferBatch
import fm.bae.fastvalues.LatestStream
import org.junit.Test
import uniffi.bae_bridge.BridgeActivityLine

/** The component shots of `AndroidTransferLine`, one for each row of its preview. */
class AndroidTransferLineShots : BoardShot() {
    @Test
    fun androidComponentTransferLineSyncing() =
        component("android.component.transfer-line.syncing", wide = true) { AndroidTransferLine("Syncing changes", batch = null) }

    @Test
    fun androidComponentTransferLineUploading() =
        component("android.component.transfer-line.uploading", wide = true) {
            Line("Uploading", "1 of 2 releases", BridgeActivityLine.UPLOADS, BAR_SHARE)
        }

    @Test
    fun androidComponentTransferLinePinning() =
        component("android.component.transfer-line.pinning", wide = true) {
            Line("Pinning", "1 of 3 releases", BridgeActivityLine.PINS, PIN_SHARE, paused = false)
        }

    @Test
    fun androidComponentTransferLineWaiting() =
        component("android.component.transfer-line.waiting", wide = true) {
            Line("Pinning", "Waiting for network", BridgeActivityLine.PINS, PIN_SHARE, paused = false)
        }

    @Test
    fun androidComponentTransferLinePaused() =
        component("android.component.transfer-line.paused", wide = true) {
            Line("Pinning", "Paused", BridgeActivityLine.PINS, PIN_SHARE, paused = true)
        }
}

private const val BAR_SHARE = 0.43

private const val PIN_SHARE = 0.3

/** A transfer line whose bar holds [share] of [line]; [paused] gives it the Pause or Resume button, null leaves it out. */
@Composable
private fun Line(
    title: String,
    count: String,
    line: BridgeActivityLine,
    share: Double,
    paused: Boolean? = null,
) {
    val store = remember { SyncActivityStore(LatestStream.activityBars(mapOf(line to share))) }
    AndroidTransferLine(
        title = title,
        batch = TransferBatch(count, store.bar(line)),
        end = paused?.let { { PausePinning(paused = it, onClick = {}) } },
    )
}
