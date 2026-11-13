package fm.bae.app.shots

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.playback.AndroidFullPlayer
import fm.bae.app.ui.playback.ExpandedSecondaryControls
import fm.bae.app.ui.playback.ExpandedTransportRow
import fm.bae.app.ui.playback.ExpandedVolumeRow
import fm.bae.app.ui.playback.SeekLineLook
import org.junit.Test
import uniffi.bae_bridge.BridgeRepeatMode

/** The playback boards' shots: the full player in the states the boards draw. */
class PlaybackBoardShots : BoardShot() {
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
                                ExpandedTransportRow(true, playing.storageWait, onPrevious = {}, onPlayPause = {}, onNext = {})
                            },
                            secondaryControls = { ExpandedSecondaryControls(BridgeRepeatMode.OFF, onCycleRepeat = {}) },
                            volumeRow = { ExpandedVolumeRow(0.7f, false, {}, {}, {}, receiverWithoutVolume = "Living Room") },
                        )
                    }
                }
            }
        }
}
