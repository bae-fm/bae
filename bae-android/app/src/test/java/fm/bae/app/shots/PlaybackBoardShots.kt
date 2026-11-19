package fm.bae.app.shots

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.playback.AndroidFullPlayer
import fm.bae.app.ui.playback.AndroidTransport
import fm.bae.app.ui.playback.AndroidVolumeSlider
import fm.bae.app.ui.playback.ExpandedSecondaryControls
import fm.bae.app.ui.playback.SeekLineLook
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackSpan

private val removed =
    PlayerStaging(
        progress = 0.35,
        elapsedSeconds = 102,
        held = listOf(BridgeTrackSpan(0.0, 0.6)),
        removedFromLibrary = true,
    )

/** The playback boards' shots: the bar and the full player in the states the boards draw. */
class PlaybackBoardShots : BoardShot() {
    /** The bar while the track playing has been deleted from the library: its title says so, with the plain tile. */
    @Test
    fun androidPlaybackBarTrackRemoved() =
        shot("android.playback.bar.track-removed") {
            ScreenScene {
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.BottomCenter) { Bar(removed) }
            }
        }

    /** The full player while the track playing has been deleted from the library. */
    @Test
    fun androidPlaybackFullPlayerTrackRemoved() =
        shot("android.playback.full-player.track-removed") {
            ScreenScene {
                CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
                    Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                        AndroidFullPlayer(
                            track = removed.track,
                            failure = null,
                            onCollapse = {},
                            seek = { removed.SeekRow(SeekLineLook.LARGE) },
                            transport = {
                                AndroidTransport(true, removed.waitsOnStorage, onPrevious = {}, onPlayPause = {}, onNext = {}, full = true)
                            },
                            secondaryControls = { ExpandedSecondaryControls(BridgeRepeatMode.OFF, onCycleRepeat = {}) },
                            volumeRow = { AndroidVolumeSlider(0.7f, false, {}, {}, {}) },
                        )
                    }
                }
            }
        }

    @Test
    fun androidPlaybackFullPlayerNoVolume() =
        shot("android.playback.full-player.no-volume") {
            ScreenScene {
                CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
                    Column(modifier = Modifier.verticalScroll(rememberScrollState())) {
                        AndroidFullPlayer(
                            track = playing.track,
                            failure = null,
                            onCollapse = {},
                            seek = { playing.SeekRow(SeekLineLook.LARGE) },
                            transport = {
                                AndroidTransport(true, playing.waitsOnStorage, onPrevious = {}, onPlayPause = {}, onNext = {}, full = true)
                            },
                            secondaryControls = { ExpandedSecondaryControls(BridgeRepeatMode.OFF, onCycleRepeat = {}) },
                            volumeRow = { AndroidVolumeSlider(0.7f, false, {}, {}, {}, receiverWithoutVolume = "Living Room") },
                        )
                    }
                }
            }
        }
}
