package fm.bae.app.shots

import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.components.AndroidPinField
import fm.bae.app.ui.components.focusOnShow
import fm.bae.app.ui.playback.AndroidCastRow
import fm.bae.app.ui.playback.CastReceiverPlace
import fm.bae.app.ui.playback.CastReceiverRole
import fm.bae.app.ui.playback.CastReceiverRows
import org.junit.Test
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeRendererKind

private fun receiver(
    name: String,
    kind: BridgeRendererKind,
) = BridgeCastReceiver(id = "id-$name", name = name, kind = kind, model = null, address = "192.168.1.42", takesVolume = true)

/** The component shots of a cast receiver's row, as the cast picker lists it, and of the field a receiver's PIN is typed in. */
class CastComponentShots : BoardShot() {
    @Test
    fun androidComponentCastRowReceiver() =
        component("android.component.cast-row.receiver", wide = true) {
            AndroidCastRow(receiver("Kitchen", BridgeRendererKind.AIR_PLAY), CastReceiverRole.Found {}, CastReceiverPlace.PICKER)
        }

    @Test
    fun androidComponentCastRowCasting() =
        component("android.component.cast-row.casting", wide = true) {
            AndroidCastRow(receiver("Living Room", BridgeRendererKind.CAST), CastReceiverRole.Casting {}, CastReceiverPlace.PICKER)
        }

    @Test
    fun androidComponentCastRowLooking() =
        component("android.component.cast-row.looking", wide = true) {
            CastReceiverRows(
                place = CastReceiverPlace.PICKER,
                receivers = BridgeCastReceiverList(casting = null, found = emptyList()),
                connectingName = null,
                onCast = {},
                onDisconnect = {},
                looking = stringResource(R.string.cast_looking_for_devices),
            )
        }

    @Test
    fun androidComponentCastRowLongName() =
        component("android.component.cast-row.long-name", wide = true) {
            AndroidCastRow(
                receiver("Denon AVR-X3800H Home Theater Receiver (Basement)", BridgeRendererKind.DLNA),
                CastReceiverRole.Casting {},
                CastReceiverPlace.PICKER,
            )
        }

    @Test
    fun androidComponentPinFieldEmpty() = component("android.component.pin-field.empty") { AndroidPinField(4, "", {}) }

    @Test
    fun androidComponentPinFieldFocused() =
        component("android.component.pin-field.focused") { AndroidPinField(4, "48", {}, Modifier.focusOnShow()) }

    @Test
    fun androidComponentPinFieldFilled() = component("android.component.pin-field.filled") { AndroidPinField(4, "4821", {}) }

    @Test
    fun androidComponentPinFieldError() =
        component("android.component.pin-field.error") { AndroidPinField(4, "4821", {}, error = "That PIN did not match.") }

    @Test
    fun androidComponentPinFieldSixDigits() = component("android.component.pin-field.six-digits") { AndroidPinField(6, "482", {}) }
}
