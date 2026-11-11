package fm.bae.app.ui.components

import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeOpenStep
import uniffi.bae_bridge.bridgeOpenStepKey

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
    StepProgressLine(
        steps = steps,
        title = { context.coreString(bridgeOpenStepKey(it.step)) },
        fraction = { if (it.total > 0uL) it.done.toDouble() / it.total.toDouble() else 0.0 },
        modifier = modifier,
        bar = bar,
        onStepping = onStepping,
        idle = idle,
    )
}

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineMigratingPreview() {
    BaeTheme {
        OpenProgressLine(
            steps = LatestStream.openProgress(BridgeOpenProgress(BridgeOpenStep.MIGRATING, 1uL, 3uL)).only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { AndroidSpinner() }
    }
}

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineRebuildingPreview() {
    BaeTheme {
        OpenProgressLine(
            steps = LatestStream.openProgress(BridgeOpenProgress(BridgeOpenStep.REBUILDING, 4uL, 9uL)).only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { AndroidSpinner() }
    }
}

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineBetweenStepsPreview() {
    BaeTheme {
        OpenProgressLine(
            steps = LatestStream.openProgress().only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { AndroidSpinner() }
    }
}
