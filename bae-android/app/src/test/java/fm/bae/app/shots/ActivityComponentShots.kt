package fm.bae.app.shots

import androidx.compose.runtime.remember
import fm.bae.app.DisplayError
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.sync.ActivityWords
import fm.bae.app.ui.sync.AndroidActivity
import fm.bae.app.ui.sync.activityProblems
import fm.bae.fastvalues.LatestStream
import org.junit.Test

/** The component shot of the Activity sheet's list: syncing, uploading and pinning, each bar part full. */
class ActivityComponentShots : BoardShot() {
    @Test
    fun androidComponentActivityDefault() =
        component("android.component.activity.default", wide = true) {
            val store = remember { SyncActivityStore(LatestStream.activityBars()) }
            val activity = PreviewData.syncActivity(needsAttention = false)
            AndroidActivity(
                problems = activityProblems(emptyList(), activity, null, emptySet()),
                app = emptyList(),
                activity = activity,
                words = ActivityWords(failure = null, uploadsWait = null, pinsWait = null),
                error = null as DisplayError?,
                bar = store::bar,
                onDismissProblem = {},
                onOpenDetails = {},
                onOpenSettings = {},
                onOpenDownloads = {},
                onSetDownloadsPaused = {},
            )
        }
}
