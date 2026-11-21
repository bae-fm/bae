package fm.bae.app.shots

import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.playback.AndroidCastRow
import fm.bae.app.ui.playback.CastReceiverPlace
import fm.bae.app.ui.playback.CastReceiverRole
import fm.bae.app.ui.playback.CastReceiverRows
import fm.bae.app.ui.playback.CastWait
import org.junit.Test
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeRendererKind

/** The component shots of `AndroidCastRow`, one for each row of its preview. */
class AndroidCastRowShots : BoardShot() {
    @Test
    fun androidComponentCastRowReceiver() =
        component("android.component.cast-row.receiver", wide = true) {
            AndroidCastRow(receiver("Kitchen", BridgeRendererKind.AIR_PLAY), CastReceiverRole.Found {}, CastReceiverPlace.PICKER, null)
        }

    @Test
    fun androidComponentCastRowCasting() =
        component("android.component.cast-row.casting", wide = true) {
            AndroidCastRow(receiver("Living Room", BridgeRendererKind.CAST), CastReceiverRole.Casting {}, CastReceiverPlace.PICKER, null)
        }

    @Test
    fun androidComponentCastRowLooking() =
        component("android.component.cast-row.looking", wide = true) {
            CastReceiverRows(
                place = CastReceiverPlace.PICKER,
                receivers = BridgeCastReceiverList(casting = null, found = emptyList(), isEmpty = true),
                connecting = null,
                onCast = {},
                onDisconnect = {},
                looking = stringResource(R.string.cast_looking_for_devices),
            )
        }

    @Test
    fun androidComponentCastRowConnecting() =
        component("android.component.cast-row.connecting", wide = true) {
            AndroidCastRow(
                receiver("Kitchen", BridgeRendererKind.AIR_PLAY),
                CastReceiverRole.Found {},
                CastReceiverPlace.PICKER,
                CastWait.CONNECTING,
            )
        }

    @Test
    fun androidComponentCastRowOwnerDeciding() =
        component("android.component.cast-row.owner-deciding", wide = true) {
            AndroidCastRow(
                receiver("Kitchen", BridgeRendererKind.AIR_PLAY),
                CastReceiverRole.Found {},
                CastReceiverPlace.PICKER,
                CastWait.OWNER_DECIDING,
            )
        }

    @Test
    fun androidComponentCastRowMenu() =
        component("android.component.cast-row.menu", wide = true) {
            AndroidCastRow(receiver("Kitchen", BridgeRendererKind.AIR_PLAY), CastReceiverRole.Found {}, CastReceiverPlace.PAGE, null)
        }

    @Test
    fun androidComponentCastRowLongName() =
        component("android.component.cast-row.long-name", wide = true) {
            AndroidCastRow(
                receiver("Denon AVR-X3800H Home Theater Receiver (Basement)", BridgeRendererKind.DLNA),
                CastReceiverRole.Casting {},
                CastReceiverPlace.PICKER,
                null,
            )
        }
}

private fun receiver(
    name: String,
    kind: BridgeRendererKind,
) = BridgeCastReceiver(id = "id-$name", name = name, kind = kind, model = null, address = "192.168.1.42", takesVolume = true)
