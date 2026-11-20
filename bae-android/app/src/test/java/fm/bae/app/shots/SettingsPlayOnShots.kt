package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import fm.bae.app.ui.playback.CastReceiverDetailsRows
import fm.bae.app.ui.playback.LocalOpenReceiverMenu
import fm.bae.app.ui.settings.AndroidSettingsPlayback
import org.junit.Test
import uniffi.bae_bridge.BridgeCastReceiver
import uniffi.bae_bridge.BridgeCastReceiverList
import uniffi.bae_bridge.BridgeCastTranscodeFormat
import uniffi.bae_bridge.BridgeRendererKind
import uniffi.bae_bridge.BridgeSidePauseCountdown

/** The shots of the android.settings.playback.receiver* boards: the Play on group of the Playback page. */
class SettingsPlayOnShots : BoardShot() {
    /** The receivers found: Living Room playing, two more. */
    @Test
    fun androidSettingsPlaybackReceiversFound() =
        shot("android.settings.playback.receivers-found") { ScreenScene { Playback(receivers()) } }

    /** The search before the first receiver answers. */
    @Test
    fun androidSettingsPlaybackReceiversLooking() =
        shot("android.settings.playback.receivers-looking") {
            ScreenScene { Playback(BridgeCastReceiverList(casting = null, found = emptyList(), isEmpty = true)) }
        }

    /** The Details sheet of the receiver playing, over the Playback page. */
    @Test
    fun androidSettingsPlaybackReceiverDetails() =
        shot("android.settings.playback.receiver-details") {
            val playing = checkNotNull(receivers().casting)
            SheetScene(playing.name, behind = { Playback(receivers()) }) {
                CastReceiverDetailsRows(playing)
            }
        }

    /** The menu of the receiver playing, open over the page: Details, then Disconnect. */
    @Test
    fun androidSettingsPlaybackReceiverMenu() =
        shot("android.settings.playback.receiver-menu") {
            val playing = checkNotNull(receivers().casting)
            ScreenScene { CompositionLocalProvider(LocalOpenReceiverMenu provides playing.id) { Playback(receivers()) } }
        }
}

/** The Playback page with Play on other devices on and [receivers] listed, as the boards draw it. */
@Composable
private fun Playback(receivers: BridgeCastReceiverList) =
    AndroidSettingsPlayback(
        pauseBetweenSides = true,
        countdown = BridgeSidePauseCountdown.OFF,
        restoreOnLaunch = true,
        playOnEnabled = true,
        receivers = { receivers },
        connectingName = { null },
        transcodeFormat = BridgeCastTranscodeFormat.MP3,
        onBack = {},
        onPauseBetweenSides = {},
        onCountdown = {},
        onRestoreOnLaunch = {},
        onPlayOnEnabledChange = {},
        onPlayOn = {},
        onDisconnect = {},
        onTranscodeFormatChange = {},
    )

/** Living Room playing, then the rest as core lists them. */
private fun receivers() =
    BridgeCastReceiverList(
        casting = BridgeCastReceiver("cast-living-room", "Living Room", BridgeRendererKind.CAST, "Google Nest Audio", "192.168.1.42", true),
        found =
            listOf(
                BridgeCastReceiver("cast-kitchen", "Kitchen", BridgeRendererKind.AIR_PLAY, "HomePod mini", "192.168.1.63", true),
                BridgeCastReceiver("cast-den", "Den receiver", BridgeRendererKind.DLNA, null, "192.168.1.57", true),
            ),
        isEmpty = false,
    )
