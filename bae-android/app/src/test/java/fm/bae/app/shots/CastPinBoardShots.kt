package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.DisplayError
import fm.bae.app.coreString
import fm.bae.app.data.CastStart
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.playback.CastBarButton
import fm.bae.app.ui.playback.CastPickerContent
import fm.bae.app.ui.playback.castCodeDialogParts
import fm.bae.app.ui.playback.castPickerHeading
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
 * or over the library, the picker's notice when the receiver refuses the cast, and the picker while a receiver
 * holds the cast for its owner.
 */
class CastPinBoardShots : BoardShot() {
    private val livingRoom = receiver("Living Room", BridgeRendererKind.CAST)
    private val kitchen = receiver("Kitchen", BridgeRendererKind.AIR_PLAY)
    private val den = receiver("Den receiver", BridgeRendererKind.DLNA)

    private val receivers =
        BridgeCastReceiverList(
            casting = null,
            found = listOf(livingRoom, kitchen, den),
            isEmpty = false,
        )

    private val playingReceivers = receivers.copy(casting = livingRoom, found = listOf(kitchen, den))

    private val pin =
        BridgeAirPlayCodeRequest(
            code = BridgeAirPlayCode.Pin(digits = 4u),
            why = BridgeAirPlayCodeAsked.FIRST_PAIRING,
            lastTry = null,
        )

    /** The Play on picker with Living Room playing and the other receivers available. */
    @Test
    fun androidPlaybackCastPicker() =
        shot("android.playback.cast-picker") {
            Picker(receivers = playingReceivers, attemptState = null)
        }

    /** Kitchen has held bae's request for five seconds while its owner decides, with Living Room playing. */
    @Test
    fun androidPlaybackCastPickerOwnerDeciding() =
        shot("android.playback.cast-picker.owner-deciding") {
            Picker(
                receivers = playingReceivers,
                attemptState = CastStart.Connecting(kitchen, ownerDeciding = true),
            )
        }

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
                attemptState =
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

    /**
     * The cast picker over the library listing [receivers], with the current [attemptState].
     */
    @Composable
    private fun Picker(
        receivers: BridgeCastReceiverList = this.receivers,
        attemptState: CastStart.AttemptState? = CastStart.Connecting(kitchen, ownerDeciding = false),
    ) {
        SheetScene(
            heading = castPickerHeading(),
            behind = {
                PlayingLibraryScreen(
                    nowPlayingBar = {
                        Bar(playing, castButton = { CastBarButton(casting = receivers.casting != null, onClick = {}) })
                    },
                )
            },
        ) {
            CastPickerContent(
                receivers = receivers,
                attemptState = attemptState,
                endedFailure = null,
                onCast = {},
                onDisconnect = {},
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
