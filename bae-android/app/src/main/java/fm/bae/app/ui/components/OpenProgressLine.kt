package fm.bae.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.coreString
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeOpenProgress

/**
 * What shows while a library opens: [idle] until the open runs a step, then
 * the step's name over a bar of how much of it is done, in its place. With no
 * open to follow, [idle] alone. With [bar] false the step shows as its name alone.
 */
@Composable
internal fun OpenProgressLine(
    steps: LatestSource<BridgeOpenProgress>?,
    modifier: Modifier = Modifier,
    bar: Boolean = true,
    onStepping: (Boolean) -> Unit = {},
    idle: @Composable () -> Unit,
) {
    val context = LocalContext.current
    AndroidProgress(
        steps = steps,
        title = { context.coreString(it.key) },
        fraction = { if (it.total > 0uL) it.done.toDouble() / it.total.toDouble() else 0.0 },
        modifier = modifier,
        bar = bar,
        onStepping = onStepping,
        idle = idle,
    )
}
