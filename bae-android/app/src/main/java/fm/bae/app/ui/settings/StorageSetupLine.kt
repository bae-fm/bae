package fm.bae.app.ui.settings

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.coreString
import fm.bae.app.ui.components.AndroidProgress
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeStorageSetupProgress
import uniffi.bae_bridge.bridgeStorageSetupStepKey

/**
 * The step a storage setup runs over its bar, drawn from [steps] each display
 * frame: counted for loading's steps, moving without a count for a step of
 * one unit. Nothing while the setup runs no step.
 */
@Composable
internal fun StorageSetupLine(steps: LatestSource<BridgeStorageSetupProgress>) {
    val context = LocalContext.current
    AndroidProgress(
        steps = steps,
        title = { context.coreString(bridgeStorageSetupStepKey(it.step)) },
        fraction = { if (it.total > 1uL) it.done.toDouble() / it.total.toDouble() else null },
    ) {}
}
