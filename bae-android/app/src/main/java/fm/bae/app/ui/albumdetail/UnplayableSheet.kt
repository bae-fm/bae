package fm.bae.app.ui.albumdetail

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSheetHeading
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeReleasePlayability
import uniffi.bae_bridge.bridgeReleasePlayabilityLine

@Composable
internal fun UnplayableSheet(
    playability: BridgeReleasePlayability.OnOtherDevice,
    onDismiss: () -> Unit,
) {
    val device =
        checkNotNull(bridgeReleasePlayabilityLine(playability)) {
            "core words every release on another device"
        }
    AndroidSheet(heading = unavailableReleaseHeading(device), onDismiss = onDismiss) {}
}

/** The heading that names the device holding a release this device can't play. */
@Composable
internal fun unavailableReleaseHeading(device: BridgeMessage): AndroidSheetHeading =
    AndroidSheetHeading(
        title = stringResource(R.string.release_not_on_this_device),
        note = LocalContext.current.coreString(device),
    )
