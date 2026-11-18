package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.CastStart
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.playback.CastReceiverPlace
import fm.bae.app.ui.playback.CastReceiverRows
import fm.bae.app.ui.playback.PickerFailures
import fm.bae.app.ui.playback.castCodeDialogParts
import org.junit.Test
import uniffi.bae_bridge.BridgeAirPlayCode
import uniffi.bae_bridge.BridgeAirPlayCodeAsked
import uniffi.bae_bridge.BridgeAirPlayCodeRefused
import uniffi.bae_bridge.BridgeAirPlayCodeRequest
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeRendererKind

/**
 * The pairing boards' shots: the dialog that asks for an AirPlay receiver's PIN or password, over the cast picker
 * or over the library, and the picker's notice when the receiver refuses the cast.
 */
class CastPinBoardShots : BoardShot() {
    private val receivers =
        BridgeCastReceiverList(
            casting = null,
            found =
                listOf(
                    receiver("Living Room", BridgeRendererKind.CAST),
                    receiver("Kitchen", BridgeRendererKind.AIR_PLAY),
                    receiver("Den receiver", BridgeRendererKind.DLNA),
                ),
        )

    private val pin =
        BridgeAirPlayCodeRequest(
            code = BridgeAirPlayCode.Pin(digits = 4u),
            why = BridgeAirPlayCodeAsked.FIRST_PAIRING,
            lastTry = null,
        )

    @Test
    fun androidPlaybackCastPin() =
        shot("android.playback.cast-pin") {
            InPlaceDialogScene(behind = { Picker() }) {
                dialog(pin, "core.airplay.code.message.pin_first", typed = "48")
            }
        }

    @Test
    fun androidPlaybackCastPinOverApp() =
        shot("android.playback.cast-pin.over-app") {
            InPlaceDialogScene(behind = { PlayingLibraryScreen() }) {
                dialog(pin, "core.airplay.code.message.pin_first", typed = "48")
            }
        }

    @Test
    fun androidPlaybackCastPinPairing() =
        shot("android.playback.cast-pin.pairing") {
            InPlaceDialogScene(behind = { Picker() }) {
                dialog(pin, "core.airplay.code.message.pin_first", typed = "4821", pairing = true)
            }
        }

    @Test
    fun androidPlaybackCastPinWrongCode() =
        shot("android.playback.cast-pin.wrong-code") {
            InPlaceDialogScene(behind = { Picker() }) {
                dialog(
                    pin.copy(lastTry = BridgeAirPlayCodeRefused.WRONG_CODE),
                    "core.airplay.code.message.pin_after_wrong",
                    typed = "4821",
                    fieldKey = "core.airplay.code.field.pin_wrong",
                )
            }
        }

    @Test
    fun androidPlaybackCastPinPassword() =
        shot("android.playback.cast-pin.password") {
            InPlaceDialogScene(behind = { Picker() }) {
                dialog(
                    pin.copy(code = BridgeAirPlayCode.Password),
                    "core.airplay.code.message.password",
                    typed = "gardenwall",
                )
            }
        }

    @Test
    fun androidPlaybackCastPinRefused() =
        shot("android.playback.cast-pin.refused") {
            Picker(
                failure =
                    CastStart.Failure(
                        DisplayError(
                            LocalContext.current.coreString(
                                "core.error.category.airplay_locked_out",
                                mapOf("receiver" to "Kitchen"),
                            ),
                        ),
                        refusedBy = "Kitchen",
                    ),
            )
        }

    /** The cast picker over the library, as the boards draw it, with [failure] above its receivers. */
    @Composable
    private fun Picker(failure: CastStart.Failure? = null) {
        SheetScene(title = stringResource(R.string.cast), behind = { PlayingLibraryScreen() }) {
            PickerFailures(endedFailure = null, castFailure = failure)
            CastReceiverRows(
                place = CastReceiverPlace.PICKER,
                receivers = receivers,
                connectingName = null,
                onCast = {},
                onDisconnect = {},
                looking = stringResource(R.string.cast_looking_for_devices),
            )
        }
    }

    /** The pairing dialog asking [request] of Kitchen with core's line [messageKey], its field holding [typed]. */
    @Composable
    private fun dialog(
        request: BridgeAirPlayCodeRequest,
        messageKey: String,
        typed: String,
        pairing: Boolean = false,
        fieldKey: String? = null,
    ): DialogParts {
        val context = LocalContext.current
        return castCodeDialogParts(
            receiver = "Kitchen",
            message = context.coreString(messageKey),
            request = request,
            pairing = pairing,
            code = typed,
            fieldLine = fieldKey?.let { context.coreString(it) },
            onCodeChange = {},
            onPair = {},
            onCancel = {},
        )
    }
}

private fun receiver(
    name: String,
    kind: BridgeRendererKind,
) = BridgeCastReceiver(id = "id-$name", name = name, kind = kind, model = null, address = "192.168.1.42", takesVolume = true)
