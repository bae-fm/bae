package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import fm.bae.app.DisplayError
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.sync.ActivityWords
import fm.bae.app.ui.sync.AndroidActivity
import fm.bae.app.ui.sync.activityProblems
import fm.bae.fastvalues.LatestStream
import org.junit.Test
import uniffi.bae_bridge.BridgeActivityLine
import uniffi.bae_bridge.BridgeDownloadQueueActivity
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeTransferBatch

/** The component shots of the Activity sheet's list: up to date, syncing, uploading and pinning, pinning held, and a status it cannot read. */
class ActivityComponentShots : BoardShot() {
    @Test
    fun androidComponentActivityUpToDate() = component("android.component.activity.up-to-date", wide = true) { Sheet(activity = idle) }

    @Test
    fun androidComponentActivityTransfers() =
        component("android.component.activity.transfers", wide = true) {
            Sheet(activity = PreviewData.syncActivity(needsAttention = false), uploads = 0.43, pins = 0.3)
        }

    @Test
    fun androidComponentActivityPinningPaused() =
        component("android.component.activity.pinning-paused", wide = true) {
            val paused =
                idle.copy(
                    held = true,
                    pins = BridgeDownloadQueueActivity(BridgeTransferBatch(done = 1u, total = 3u), wait = null, paused = true),
                )
            Sheet(activity = paused, words = ActivityWords(failure = null, uploadsWait = null, pinsWait = "Paused"))
        }

    @Test
    fun androidComponentActivityStatusUnknown() =
        component("android.component.activity.status-unknown", wide = true) {
            Sheet(activity = null, error = DisplayError("bae couldn’t read the sync status."))
        }
}

private val idle =
    BridgeSyncActivity(
        moving = false,
        held = false,
        failure = null,
        syncing = false,
        uploads = null,
        pins = null,
        movingToThisDevice = null,
    )

@Composable
private fun Sheet(
    activity: BridgeSyncActivity?,
    uploads: Double = 0.0,
    pins: Double = 0.0,
    words: ActivityWords = ActivityWords(failure = null, uploadsWait = null, pinsWait = null),
    error: DisplayError? = null,
) {
    val store =
        remember {
            SyncActivityStore(
                LatestStream.activityBars(
                    mapOf(
                        BridgeActivityLine.UPLOADS to uploads,
                        BridgeActivityLine.PINS to pins,
                    ),
                ),
            )
        }
    AndroidActivity(
        problems = activityProblems(emptyList(), activity, null, emptySet()),
        app = emptyList(),
        activity = activity,
        words = words,
        error = error,
        bar = store::bar,
        onDismissProblem = {},
        onOpenDetails = {},
        onOpenSettings = {},
        onOpenDownloads = {},
        onSetDownloadsPaused = {},
    )
}
