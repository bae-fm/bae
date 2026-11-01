package fm.bae.app.ui.settings

import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.coreString
import fm.bae.app.ui.components.StepBar
import fm.bae.fastvalues.LatestSource
import fm.bae.fastvalues.changes
import uniffi.bae_bridge.BridgeStorageSetupProgress
import uniffi.bae_bridge.bridgeStorageSetupStepKey

/**
 * The step a storage setup runs, read from [steps] as it changes, over its
 * bar: counted for loading's steps, moving without a count for a step of one
 * unit.
 */
@Composable
internal fun StorageSetupLine(steps: LatestSource<BridgeStorageSetupProgress>) {
    val context = LocalContext.current
    val progress by remember(steps) { steps.changes() }.collectAsState(initial = null)
    progress?.let { shown ->
        StepBar(
            title = context.coreString(bridgeStorageSetupStepKey(shown.step)),
            fraction = if (shown.total > 1uL) shown.done.toFloat() / shown.total.toFloat() else null,
        )
    }
}
