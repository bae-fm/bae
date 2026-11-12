package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Cast
import androidx.compose.material.icons.outlined.Speaker
import androidx.compose.material.icons.outlined.Tv
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import fm.bae.app.R
import fm.bae.app.data.isEmpty
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidMoreMenu
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonKind
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeRendererKind

/** Where a row's receiver stands in the list, and what the row offers. */
internal sealed interface CastReceiverRole {
    /** Playback is cast to it; the row offers Disconnect. */
    data class Casting(
        val onDisconnect: () -> Unit,
    ) : CastReceiverRole

    /** Discovery found it; tapping the row casts to it. */
    data class Found(
        val onCast: () -> Unit,
    ) : CastReceiverRole
}

/** Where receivers are listed, which decides how a row ends. */
internal enum class CastReceiverPlace {
    /** The cast picker, opened to pick a receiver or stop: the receiver cast to ends in Disconnect. */
    PICKER,

    /** The Casting page: every row ends in a ⋮ menu holding Details, and Disconnect on the receiver cast to. */
    PAGE,
}

/**
 * A receiver as the cast picker and the Casting page list it: its kind's icon, its name over its kind, or over
 * "Casting" when playback is on it, which is also the one row that offers Disconnect. Any other row casts to it.
 * On the page every row also has a ⋮ menu whose Details opens a sheet on the receiver.
 */
@Composable
internal fun AndroidCastRow(
    receiver: BridgeCastReceiver,
    role: CastReceiverRole,
    place: CastReceiverPlace,
) {
    val casting = role is CastReceiverRole.Casting
    var showsDetails by remember { mutableStateOf(false) }
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .then(
                    when (role) {
                        is CastReceiverRole.Casting -> Modifier
                        is CastReceiverRole.Found -> Modifier.clickable(onClick = role.onCast)
                    },
                ).padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = deviceIcon(receiver.kind),
            contentDescription = null,
            tint =
                if (casting) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.onSurfaceVariant
                },
        )
        Spacer(modifier = Modifier.width(ThemeSpace.group))
        CastReceiverLabel(receiver = receiver, casting = casting, modifier = Modifier.weight(1f))
        CastReceiverEnd(role = role, place = place, onDetails = { showsDetails = true })
    }
    if (showsDetails) {
        AndroidSheet(title = receiver.name, onDismiss = { showsDetails = false }) {
            CastReceiverDetailsRows(receiver)
        }
    }
}

/** A receiver's name, medium as the design sets it, over its kind, or over "Casting" when playback is on it. */
@Composable
private fun CastReceiverLabel(
    receiver: BridgeCastReceiver,
    casting: Boolean,
    modifier: Modifier,
) {
    Column(modifier = modifier) {
        Text(
            text = receiver.name,
            style = ThemeText.rowTitle.style,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        if (casting) {
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
}

/** How a row ends: the picker's Disconnect on the receiver cast to, or the page's ⋮ menu on every row. */
@Composable
private fun CastReceiverEnd(
    role: CastReceiverRole,
    place: CastReceiverPlace,
    onDetails: () -> Unit,
) {
    when (place) {
        CastReceiverPlace.PICKER -> {
            if (role is CastReceiverRole.Casting) {
                AndroidButton(kind = ButtonKind.TEXT, onClick = role.onDisconnect) {
                    Text(stringResource(R.string.cast_disconnect))
                }
            }
        }

        CastReceiverPlace.PAGE -> {
            AndroidMoreMenu { close ->
                CastReceiverMenuItems(role = role, onDetails = onDetails, close = close)
            }
        }
    }
}

/** The ⋮ menu's items: Details on every row, Disconnect on the receiver cast to. */
@Composable
internal fun CastReceiverMenuItems(
    role: CastReceiverRole,
    onDetails: () -> Unit,
    close: () -> Unit,
) {
    DropdownMenuItem(
        text = { Text(stringResource(R.string.details_menu)) },
        onClick = {
            close()
            onDetails()
        },
    )
    if (role is CastReceiverRole.Casting) {
        DropdownMenuItem(
            text = { Text(stringResource(R.string.cast_disconnect)) },
            onClick = {
                close()
                role.onDisconnect()
            },
        )
    }
}

/** What the Details sheet shows of a receiver: its type, its model when it states one, and its address. */
@Composable
internal fun CastReceiverDetailsRows(receiver: BridgeCastReceiver) {
    AndroidRow(title = stringResource(R.string.cast_receiver_type), subtitle = receiver.kind.productName)
    receiver.model?.let { model ->
        AndroidRow(title = stringResource(R.string.cast_receiver_model), subtitle = model)
    }
    AndroidRow(title = stringResource(R.string.cast_receiver_address), subtitle = receiver.address)
}

/**
 * What the cast picker and the Casting page list: the receiver a cast is connecting to, while one is,
 * then each receiver once, the one cast to first, or [looking] while none has answered. Browsing has no
 * end while either is open.
 */
@Composable
internal fun CastReceiverRows(
    place: CastReceiverPlace,
    receivers: BridgeCastReceiverList,
    connectingName: String?,
    onCast: (BridgeCastReceiver) -> Unit,
    onDisconnect: () -> Unit,
    looking: String,
) {
    if (connectingName != null) {
        WaitingRow(text = stringResource(R.string.cast_connecting, connectingName))
    }
    if (receivers.isEmpty) {
        WaitingRow(text = looking)
    } else {
        receivers.casting?.let { casting ->
            AndroidCastRow(receiver = casting, role = CastReceiverRole.Casting(onDisconnect), place = place)
        }
        receivers.found.forEach { receiver ->
            AndroidCastRow(receiver = receiver, role = CastReceiverRole.Found { onCast(receiver) }, place = place)
        }
    }
}

/** A row for a wait: a spinner and what it waits on. */
@Composable
private fun WaitingRow(text: String) {
    AndroidSpinner(
        text = text,
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
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
        BridgeRendererKind.CAST -> Icons.Outlined.Cast
        BridgeRendererKind.AIR_PLAY -> Icons.Outlined.Speaker
        BridgeRendererKind.DLNA -> Icons.Outlined.Tv
    }
