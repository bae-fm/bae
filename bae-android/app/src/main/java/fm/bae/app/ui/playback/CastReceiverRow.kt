package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cast
import androidx.compose.material.icons.filled.Speaker
import androidx.compose.material.icons.filled.Tv
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.WaitingIndicator
import fm.bae.app.ui.components.cardRowDivider
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeRendererKind

/**
 * A receiver as the cast picker and the Casting page list it: its kind's icon, its name over its kind, or over
 * "Casting" when playback is on it, which is also the one row that offers Disconnect. Any other row casts to it.
 */
@Composable
internal fun CastReceiverRow(
    receiver: BridgeCastReceiver,
    onCast: () -> Unit,
    onDisconnect: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .cardRowDivider()
                .fillMaxWidth()
                .then(if (receiver.casting) Modifier else Modifier.clickable(onClick = onCast))
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = deviceIcon(receiver.kind),
            contentDescription = null,
            tint =
                if (receiver.casting) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.onSurfaceVariant
                },
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = receiver.name,
                style = ThemeText.rowTitle.style,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            if (receiver.casting) {
                Text(
                    text = stringResource(R.string.cast_receiver_casting),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.primary,
                )
            } else {
                Text(
                    text = receiver.kind.productName,
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        if (receiver.casting) {
            TextButton(onClick = onDisconnect) {
                Text(stringResource(R.string.cast_disconnect))
            }
        }
    }
}

/**
 * What the cast picker and the Casting page list: the receiver a cast is connecting to, while one is,
 * then each receiver once, or [looking] while none has answered. Browsing has no end while either is open.
 */
@Composable
internal fun CastReceiverRows(
    receivers: List<BridgeCastReceiver>,
    connectingName: String?,
    onCast: (BridgeCastReceiver) -> Unit,
    onDisconnect: () -> Unit,
    looking: String,
) {
    if (connectingName != null) {
        WaitingRow(text = stringResource(R.string.cast_connecting, connectingName))
    }
    if (receivers.isEmpty()) {
        WaitingRow(text = looking)
    } else {
        receivers.forEach { receiver ->
            CastReceiverRow(receiver = receiver, onCast = { onCast(receiver) }, onDisconnect = onDisconnect)
        }
    }
}

/** A row for a wait: a spinner and what it waits on. */
@Composable
private fun WaitingRow(text: String) {
    WaitingIndicator(
        text = text,
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
    )
}

/** A kind by its product's name, the same in every language. */
private val BridgeRendererKind.productName: String
    get() =
        when (this) {
            BridgeRendererKind.CAST -> "Chromecast"
            BridgeRendererKind.AIR_PLAY -> "AirPlay"
            BridgeRendererKind.DLNA -> "UPnP"
        }

/** The icon for a receiver, by the kind of renderer it is. */
private fun deviceIcon(kind: BridgeRendererKind): ImageVector =
    when (kind) {
        BridgeRendererKind.CAST -> Icons.Filled.Cast
        BridgeRendererKind.AIR_PLAY -> Icons.Filled.Speaker
        BridgeRendererKind.DLNA -> Icons.Filled.Tv
    }
