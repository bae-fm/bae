package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.sync.AndroidActivityButton
import org.junit.Test
import uniffi.bae_bridge.BridgeSyncActivity

/** The component shots of `AndroidActivityButton`, one for each row of its preview. */
class AndroidActivityButtonShots : BoardShot() {
    @Test
    fun androidComponentActivityButtonMoving() =
        component("android.component.activity-button.moving") { ActivityButton(PreviewData.syncActivity(needsAttention = false)) }

    @Test
    fun androidComponentActivityButtonAttention() =
        component("android.component.activity-button.attention") {
            ActivityButton(idleActivity.copy(failure = PreviewData.syncActivity(true).failure))
        }

    @Test
    fun androidComponentActivityButtonPaused() =
        component("android.component.activity-button.paused") {
            ActivityButton(idleActivity.copy(held = true))
        }
}

/** The Activity button over [activity], as the library's header holds it. */
@Composable
internal fun ActivityButton(activity: BridgeSyncActivity) {
    val store = remember { SyncActivityStore().also { it.apply(activity) } }
    AndroidActivityButton(
        store = store,
        app = emptyList(),
        dismissed = emptySet(),
        onDismissProblem = {},
        onOpenSettings = {},
        onOpenDownloads = {},
        onSetDownloadsPaused = {},
    )
}
