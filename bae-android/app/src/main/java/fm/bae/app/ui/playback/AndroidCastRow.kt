package fm.bae.app.ui.playback

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
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
import fm.bae.app.data.CastStart
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidMoreMenu
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSheet
import fm.bae.app.ui.components.AndroidSheetHeading
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonKind
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeRendererKind

/** Where a row's receiver stands in the list, and what the row offers. */
internal sealed interface CastReceiverRole {
    /** Playback is on it; the row offers Disconnect. */
    data class Casting(
        val onDisconnect: () -> Unit,
    ) : CastReceiverRole

    /** Discovery found it; tapping the row plays on it. */
    data class Found(
        val onCast: () -> Unit,
    ) : CastReceiverRole
}

/** What bae waits on while it connects to a row's receiver. */
internal enum class CastWait {
    /** The receiver to answer. */
    CONNECTING,

    /** The receiver's owner to accept bae on it: an AirPlay 2 receiver has held bae's request for five seconds. */
    OWNER_DECIDING,
}

/** Where receivers are listed, which decides how a row ends. */
internal enum class CastReceiverPlace {
    /** The Play on picker, opened to pick a receiver or stop: the receiver playing ends in Disconnect. */
    PICKER,

    /** The Playback settings: every row ends in a ⋮ menu holding Details, and Disconnect on the receiver playing. */
    PAGE,
}

/**
 * A receiver as the Play on picker and the Playback settings list it: its kind's icon, its name over its kind, or over
 * "Playing" when playback is on it, which is also the one row that offers Disconnect. Any other row plays on it.
 * On the page every row also has a ⋮ menu whose Details opens a sheet on the receiver. While bae connects to the
 * receiver ([wait]), the row says what it waits on in place of its kind, with a spinner at its end.
 */
@Composable
internal fun AndroidCastRow(
    receiver: BridgeCastReceiver,
    role: CastReceiverRole,
    place: CastReceiverPlace,
    wait: CastWait?,
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
        CastReceiverLabel(receiver = receiver, casting = casting, wait = wait, modifier = Modifier.weight(1f))
        if (wait != null) {
            AndroidSpinner(size = ThemeIcon.medium)
        }
        CastReceiverEnd(role = role, place = place, onDetails = { showsDetails = true })
    }
    if (showsDetails) {
        AndroidSheet(heading = AndroidSheetHeading(receiver.name), onDismiss = { showsDetails = false }) {
            CastReceiverDetailsRows(receiver)
        }
    }
}

/**
 * A receiver's name, medium as the design sets it, over what bae waits on while it connects, else over its kind, or
 * over "Playing" when playback is on it.
 */
@Composable
private fun CastReceiverLabel(
    receiver: BridgeCastReceiver,
    casting: Boolean,
    wait: CastWait?,
    modifier: Modifier,
) {
    Column(modifier = modifier) {
        Text(
            text = receiver.name,
            style = ThemeText.rowTitle.style,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        if (wait != null) {
            Text(
                text = waitLine(receiver, wait),
                style = ThemeText.body.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else if (casting) {
            Text(
                text = stringResource(R.string.cast_receiver_playing),
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

/** The words of [wait] on the row of [receiver]. */
@Composable
private fun waitLine(
    receiver: BridgeCastReceiver,
    wait: CastWait,
): String =
    when (wait) {
        CastWait.CONNECTING -> stringResource(R.string.cast_receiver_connecting)
        CastWait.OWNER_DECIDING -> stringResource(R.string.cast_receiver_owner_deciding, receiver.name)
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
 * What the Play on picker and the Playback settings list: each receiver once, the one playing first, the one
 * the cast [connecting] goes to saying so on its row, or [looking] while none has answered. Browsing has no end
 * while either is open.
 */
@Composable
internal fun CastReceiverRows(
    place: CastReceiverPlace,
    receivers: BridgeCastReceiverList,
    connecting: CastStart.Connecting?,
    onCast: (BridgeCastReceiver) -> Unit,
    onDisconnect: () -> Unit,
    looking: String,
) {
    if (receivers.isEmpty) {
        WaitingRow(text = looking)
    } else {
        receivers.casting?.let { casting ->
            AndroidCastRow(
                receiver = casting,
                role = CastReceiverRole.Casting(onDisconnect),
                place = place,
                wait = connecting.waitOn(casting),
            )
        }
        receivers.found.forEach { receiver ->
            AndroidCastRow(
                receiver = receiver,
                role = CastReceiverRole.Found { onCast(receiver) },
                place = place,
                wait = connecting.waitOn(receiver),
            )
        }
    }
}

/** What the row of [receiver] waits on, when this cast connecting is to it. */
private fun CastStart.Connecting?.waitOn(receiver: BridgeCastReceiver): CastWait? =
    when {
        this == null || this.receiver.id != receiver.id -> null
        ownerDeciding -> CastWait.OWNER_DECIDING
        else -> CastWait.CONNECTING
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
        BridgeRendererKind.CAST -> SymbolIcons.Cast
        BridgeRendererKind.AIR_PLAY -> SymbolIcons.Speaker
        BridgeRendererKind.DLNA -> SymbolIcons.Tv
    }
