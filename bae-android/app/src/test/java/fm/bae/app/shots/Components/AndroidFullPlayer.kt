package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import fm.bae.app.ui.playback.AndroidFullPlayer
import fm.bae.app.ui.playback.AndroidTransport
import fm.bae.app.ui.playback.AndroidVolumeSlider
import fm.bae.app.ui.playback.ExpandedSecondaryControls
import fm.bae.app.ui.playback.SeekLineLook
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode

/** The component shots of `AndroidFullPlayer`, one for each row of its preview. */
class AndroidFullPlayerShots : BoardShot() {
    @Test
    fun androidComponentFullPlayerPlaying() = shot("android.component.full-player.playing") { FullPlayer(playing) }

    @Test
    fun androidComponentFullPlayerPaused() = shot("android.component.full-player.paused") { FullPlayer(playing, isPlaying = false) }

    @Test
    fun androidComponentFullPlayerWaiting() = shot("android.component.full-player.waiting") { FullPlayer(waiting) }

    @Test
    fun androidComponentFullPlayerRepeatOn() =
        shot("android.component.full-player.repeat-on") { FullPlayer(playing, repeatMode = BridgeRepeatMode.CONTEXT) }

    @Test
    fun androidComponentFullPlayerNoCover() = shot("android.component.full-player.no-cover") { FullPlayer(withoutCover) }

    @Test
    fun androidComponentFullPlayerLowVolume() = shot("android.component.full-player.low-volume") { FullPlayer(playing, volume = 0.2f) }

    @Test
    fun androidComponentFullPlayerVolumeOnTheReceiver() =
        shot("android.component.full-player.volume-on-the-receiver") { FullPlayer(playing, receiverWithoutVolume = "Living Room") }
}

/** The full player as the boards draw it: [staging]'s track, playing or not, at [volume], with repeat set to [repeatMode]. */
@Composable
internal fun FullPlayer(
    staging: PlayerStaging,
    isPlaying: Boolean = true,
    repeatMode: BridgeRepeatMode = BridgeRepeatMode.OFF,
    volume: Float = 0.7f,
    receiverWithoutVolume: String? = null,
) = ScreenScene {
    Images {
        Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
            AndroidFullPlayer(
                track = staging.track,
                failure = null,
                onCollapse = {},
                seek = { staging.SeekRow(SeekLineLook.LARGE) },
                transport = {
                    AndroidTransport(isPlaying, staging.waitsOnStorage, onPrevious = {}, onPlayPause = {}, onNext = {}, full = true)
                },
                secondaryControls = { ExpandedSecondaryControls(repeatMode, onCycleRepeat = {}) },
                volumeRow = { AndroidVolumeSlider(volume, false, {}, {}, {}, receiverWithoutVolume) },
            )
        }
    }
}
