package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.WaitingIndicator
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeRendererKind

/** The receivers bae has found, or a spinner until the first answers: browsing has no end while the page is open. */
@Composable
internal fun CastReceivers(receivers: List<BridgeCastReceiver>) {
    CastingCard {
        if (receivers.isEmpty()) {
            WaitingIndicator(text = stringResource(R.string.settings_casting_looking))
        } else {
            receivers.forEachIndexed { index, receiver ->
                if (index > 0) HorizontalDivider(modifier = Modifier.padding(vertical = ThemeSpace.group))
                Text(text = receiver.name)
                Text(
                    text = castReceiverKindName(receiver.kind),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** A kind by its product's name, the same in every language. */
private fun castReceiverKindName(kind: BridgeRendererKind): String =
    when (kind) {
        BridgeRendererKind.CAST -> "Chromecast"
        BridgeRendererKind.AIR_PLAY -> "AirPlay"
        BridgeRendererKind.DLNA -> "UPnP"
    }
