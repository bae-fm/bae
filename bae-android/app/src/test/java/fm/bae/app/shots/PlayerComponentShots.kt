package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.ui.playback.AndroidNowPlayingTrack
import fm.bae.app.ui.playback.AndroidQueueSection
import fm.bae.app.ui.playback.AndroidTransport
import fm.bae.app.ui.playback.AndroidVolumeSlider
import fm.bae.app.ui.playback.CastBarButton
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackSpan

private val withoutCover =
    PlayerStaging(progress = 0.35, elapsedSeconds = 102, held = listOf(BridgeTrackSpan(0.0, 0.6)), noCover = true)
private val longTitle = playing

/** The component shots of the player's pieces: the bar's variants, the playing track, transport, volume slider and the full player's variants. */
class PlayerComponentShots : BoardShot() {
    @Test
    fun androidComponentNowPlayingBarNoCover() = component("android.component.now-playing-bar.no-cover", wide = true) { Bar(withoutCover) }

    @Test
    fun androidComponentNowPlayingBarCasting() =
        component("android.component.now-playing-bar.casting", wide = true) {
            Bar(playing, castButton = { CastBarButton(casting = true, onClick = {}) })
        }

    @Test
    fun androidComponentNowPlayingBarCastOff() =
        component("android.component.now-playing-bar.cast-off", wide = true) { Bar(playing, castButton = {}) }

    @Test
    fun androidComponentNowPlayingBarRepeat() =
        component("android.component.now-playing-bar.repeat", wide = true) { Bar(playing, repeatMode = BridgeRepeatMode.CONTEXT) }

    @Test
    fun androidComponentNowPlayingBarError() =
        component("android.component.now-playing-bar.error", wide = true) {
            Bar(playing, failure = DisplayError("Could not read what is playing."))
        }

    @Test
    fun androidComponentNowPlayingTrackPlaying() = component("android.component.now-playing-track.playing", wide = true) { Track(playing) }

    @Test
    fun androidComponentNowPlayingTrackNoCover() =
        component("android.component.now-playing-track.no-cover", wide = true) { Track(withoutCover) }

    @Test
    fun androidComponentNowPlayingTrackLongTitle() =
        component("android.component.now-playing-track.long-title", wide = true) {
            Row(modifier = Modifier.width(260.dp)) { Track(longTitle) }
        }

    @Test
    fun androidComponentTransportPaused() = component("android.component.transport.paused") { AndroidTransport(false, false, {}, {}, {}) }

    @Test
    fun androidComponentTransportPlaying() = component("android.component.transport.playing") { AndroidTransport(true, false, {}, {}, {}) }

    @Test
    fun androidComponentTransportWaiting() = component("android.component.transport.waiting") { AndroidTransport(true, true, {}, {}, {}) }

    @Test
    fun androidComponentTransportLarge() =
        component("android.component.transport.large", wide = true) { AndroidTransport(true, false, {}, {}, {}, full = true) }

    @Test
    fun androidComponentVolumeSliderMostOfTheWay() =
        component("android.component.volume-slider.most-of-the-way", wide = true) { Volume(0.7f) }

    @Test
    fun androidComponentVolumeSliderLow() = component("android.component.volume-slider.low", wide = true) { Volume(0.25f) }

    @Test
    fun androidComponentVolumeSliderSilent() = component("android.component.volume-slider.silent", wide = true) { Volume(0f) }

    @Test
    fun androidComponentVolumeSliderFull() = component("android.component.volume-slider.full", wide = true) { Volume(1f) }

    @Test
    fun androidComponentQueueSectionShuffleOn() =
        component("android.component.queue-section.shuffle-on", wide = true) {
            AndroidQueueSection("Playing from", "Clear playing from", {}, shuffled = true)
        }

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

@Composable
private fun Track(staging: PlayerStaging) = Images { Column { AndroidNowPlayingTrack(staging.track, onExpand = {}) } }

@Composable
private fun Volume(level: Float) = AndroidVolumeSlider(level, false, {}, {}, {})
