package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.playback.CastReceiverDetailsRows
import fm.bae.app.ui.playback.CastReceiverMenuItems
import fm.bae.app.ui.playback.CastReceiverRole
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
            ScreenScene {
                Box(modifier = Modifier.fillMaxSize()) {
                    Playback(receivers())
                    Surface(
                        shape = MenuDefaults.shape,
                        color = MenuDefaults.containerColor,
                        tonalElevation = MenuDefaults.TonalElevation,
                        shadowElevation = MenuDefaults.ShadowElevation,
                        modifier = Modifier.align(Alignment.TopEnd).padding(top = MENU_TOP, end = MENU_END),
                    ) {
                        Column(modifier = Modifier.padding(vertical = ThemeSpace.related).width(MENU_WIDTH)) {
                            CastReceiverMenuItems(role = CastReceiverRole.Casting(onDisconnect = {}), onDetails = {}, close = {})
                        }
                    }
                }
            }
        }
}

private val MENU_TOP = 485.dp
private val MENU_END = 32.dp
private val MENU_WIDTH = 200.dp

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
