package fm.bae.app.shots

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import fm.bae.app.ui.components.AndroidProgress
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Test
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeOpenStep

/** The component shots of `AndroidProgress`, one for each row of its preview. */
class AndroidProgressShots : BoardShot() {
    @Test
    fun androidComponentProgressLengthUnknown() = component("android.component.progress.length-unknown") { Progress(fraction = null) }

    @Test
    fun androidComponentProgressLengthKnown() = component("android.component.progress.length-known") { Progress(fraction = 0.3) }
}

@Composable
private fun Progress(fraction: Double?) {
    val steps =
        remember {
            LatestStream
                .openProgress(
                    BridgeOpenProgress(BridgeOpenStep.MIGRATING, "core.library.open_step.migrating", 1uL, 3uL, 1.0 / 3),
                ).only
        }
    AndroidProgress(
        steps = steps,
        title = { "Uploading…" },
        fraction = { fraction },
        modifier = Modifier.fillMaxWidth(),
    ) {}
}
