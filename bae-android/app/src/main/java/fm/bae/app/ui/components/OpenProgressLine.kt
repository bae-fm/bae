package fm.bae.app.ui.components

import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.coreString
import fm.bae.app.ui.PreviewTheme
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import uniffi.bae_bridge.BridgeOpenProgress
import uniffi.bae_bridge.BridgeOpenStep

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

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineMigratingPreview() {
    PreviewTheme {
        OpenProgressLine(
            steps =
                LatestStream
                    .openProgress(
                        BridgeOpenProgress(BridgeOpenStep.MIGRATING, "core.library.open_step.migrating", 1uL, 3uL),
                    ).only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { AndroidSpinner() }
    }
}

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineRebuildingPreview() {
    PreviewTheme {
        OpenProgressLine(
            steps =
                LatestStream
                    .openProgress(
                        BridgeOpenProgress(BridgeOpenStep.REBUILDING, "core.library.open_step.rebuilding", 4uL, 9uL),
                    ).only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { AndroidSpinner() }
    }
}

@Preview(showBackground = true)
@Composable
private fun OpenProgressLineBetweenStepsPreview() {
    PreviewTheme {
        OpenProgressLine(
            steps = LatestStream.openProgress().only,
            modifier = Modifier.widthIn(max = 360.dp).padding(16.dp),
        ) { AndroidSpinner() }
    }
}
