package fm.bae.app.shots

import android.content.res.Configuration
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.DisplayError
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.AppProblem
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.library.LibraryGridScene
import fm.bae.app.ui.library.SceneGrid
import fm.bae.app.ui.signInSettingsProblem
import fm.bae.app.ui.sync.ActivityButton
import fm.bae.app.ui.sync.ActivityList
import fm.bae.app.ui.sync.ActivityWords
import fm.bae.app.ui.sync.activityProblems
import fm.bae.fastvalues.LatestStream
import uniffi.bae_bridge.BridgeDownloadQueueActivity
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.BridgeSyncActivity
import uniffi.bae_bridge.BridgeTransferBatch
import uniffi.bae_bridge.BridgeUploadQueueActivity
import uniffi.bae_bridge.BridgeUploadQueueWait

private val NO_WORDS = ActivityWords(failure = null, uploadsWait = null, pinsWait = null)

/** The Activity sheet's list with sync's failure and a transfer under way. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun ActivityNeedingAttention() {
    val activity =
        PreviewData
            .syncActivity(needsAttention = false)
            .copy(
                needsAttention = true,
                failure = BridgeException.Diagnostic(BridgeErrorCategory.Database, "401"),
                syncing = false,
                uploads = BridgeUploadQueueActivity(BridgeTransferBatch(done = 0u, total = 3u), wait = null, paused = false),
                pins = null,
            )
    ActivityListShot(
        activity,
        ActivityWords(DisplayError("Dropbox signed this device out.", "401"), uploadsWait = null, pinsWait = null),
        app = emptyList(),
        bars = mapOf("uploads" to 0.0),
    )
}

/** The Activity sheet's list with a problem of the app's own: the sign-in settings it couldn't read. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun ActivityLaunchProblem() {
    val activity =
        PreviewData
            .syncActivity(needsAttention = false)
            .copy(
                syncing = false,
                uploads = BridgeUploadQueueActivity(BridgeTransferBatch(done = 0u, total = 3u), wait = null, paused = false),
                pins = null,
            )
    val context = LocalContext.current
    ActivityListShot(
        activity,
        NO_WORDS,
        app = listOf(signInSettingsProblem(context, "OAuthConfig: missing client id")),
        bars = mapOf("uploads" to 0.0),
    )
}

/** The Activity sheet's list while syncing, uploading and pinning, each bar part full. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun ActivityTransfers() {
    ActivityListShot(
        PreviewData.syncActivity(needsAttention = false),
        NO_WORDS,
        app = emptyList(),
        bars = mapOf("uploads" to 0.43, "pins" to 0.3),
    )
}

/** The Activity sheet's list with both queues held: Paused in place of each count, and Resume. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun ActivityTransfersHeld() {
    val batch = BridgeTransferBatch(done = 0u, total = 2u)
    val activity =
        PreviewData
            .syncActivity(needsAttention = false)
            .copy(
                syncing = false,
                uploads = BridgeUploadQueueActivity(batch, BridgeUploadQueueWait.Paused, paused = true),
                pins = BridgeDownloadQueueActivity(batch, BridgeQueueWait.PAUSED, paused = true),
            )
    ActivityListShot(
        activity,
        ActivityWords(failure = null, uploadsWait = "Paused", pinsWait = "Paused"),
        app = emptyList(),
        bars = mapOf("uploads" to 0.0, "pins" to 0.0),
    )
}

/** The library's top bar while both queues are paused and nothing moves: the Activity button's pause symbol. */
@Preview(device = PHONE_SPEC, uiMode = Configuration.UI_MODE_NIGHT_YES)
@Composable
fun ActivityButtonHeld() {
    val batch = BridgeTransferBatch(done = 0u, total = 2u)
    val activity =
        PreviewData
            .syncActivity(needsAttention = false)
            .copy(
                moving = false,
                held = true,
                syncing = false,
                uploads = BridgeUploadQueueActivity(batch, BridgeUploadQueueWait.Paused, paused = true),
                pins = BridgeDownloadQueueActivity(batch, BridgeQueueWait.PAUSED, paused = true),
            )
    val store = remember { SyncActivityStore().apply { apply(activity) } }
    LibraryGridScene(grid = SceneGrid.FLAT, firstItem = 0, openMenu = null) {
        ActivityButton(
            store = store,
            app = emptyList(),
            dismissed = emptySet(),
            onDismissProblem = {},
            onOpenSettings = {},
            onOpenDownloads = {},
            onSetDownloadsPaused = {},
        )
    }
}

@Composable
private fun ActivityListShot(
    activity: BridgeSyncActivity,
    words: ActivityWords,
    app: List<AppProblem>,
    bars: Map<String, Double>,
) {
    val store = remember { SyncActivityStore(LatestStream.activityBars(bars)) }
    BaeTheme {
        Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            ActivityList(
                problems = activityProblems(app, activity, null, emptySet()),
                app = app,
                activity = activity,
                words = words,
                error = null,
                bar = store::bar,
                onDismissProblem = {},
                onOpenDetails = {},
                onOpenSettings = {},
                onOpenDownloads = {},
                onSetDownloadsPaused = {},
            )
        }
    }
}
