package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.downloads.AndroidPinningBar
import org.junit.Test

/** The component shots of `AndroidPinningBar`, one for each row of its preview. */
class AndroidPinningBarShots : BoardShot() {
    @Test
    fun androidComponentPinningBarPinning() =
        component(
            "android.component.pinning-bar.pinning",
        ) { PinningBar(paused = false, hasDownloads = true, hasFailures = false) }

    @Test
    fun androidComponentPinningBarFailed() =
        component(
            "android.component.pinning-bar.failed",
        ) { PinningBar(paused = false, hasDownloads = true, hasFailures = true) }

    @Test
    fun androidComponentPinningBarPaused() =
        component(
            "android.component.pinning-bar.paused",
        ) { PinningBar(paused = true, hasDownloads = true, hasFailures = false) }

    @Test
    fun androidComponentPinningBarNothingToPin() =
        component("android.component.pinning-bar.nothing-to-pin") {
            PinningBar(paused = false, hasDownloads = false, hasFailures = false)
        }
}

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
