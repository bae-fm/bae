package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.playback.AndroidFullPlayer
import fm.bae.app.ui.playback.AndroidQueueSection
import fm.bae.app.ui.playback.AndroidTransport
import fm.bae.app.ui.playback.AndroidVolumeRow
import fm.bae.app.ui.playback.ExpandedSecondaryControls
import fm.bae.app.ui.playback.SeekLineLook
import org.junit.Test
import uniffi.bae_bridge.BridgeNowPlayingDisplay
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeTrackSpan

private val removed =
    PlayerStaging(
        elapsedSeconds = 102,
        held = listOf(BridgeTrackSpan(0.0, 0.6)),
        track = NowPlaying(playing.track.trackId, BridgeNowPlayingDisplay.Removed, sidePausePrompt = null),
    )

/** The playback boards' shots: the bar and the full player in the states the boards draw. */
class PlaybackBoardShots : BoardShot() {
    /** The bar while the track playing has been deleted from the library: its title says so, with the plain tile. */
    @Test
    fun androidPlaybackBarTrackRemoved() =
        shot("android.playback.bar.track-removed") {
            ScreenScene { PlayingLibraryScreen(nowPlayingBar = { Bar(removed) }) }
        }

    /** The full player while the track playing has been deleted from the library. */
    @Test
    fun androidPlaybackFullPlayerTrackRemoved() =
        shot("android.playback.full-player.track-removed") {
            FullPlayerOverLibrary(removed) { AndroidVolumeRow(0.7f, false, {}, {}, {}) }
        }

    @Test
    fun androidPlaybackFullPlayerNoVolume() =
        shot("android.playback.full-player.no-volume") {
            FullPlayerOverLibrary(playing) { AndroidVolumeRow(0.7f, false, {}, {}, {}, receiverWithoutVolume = "Living Room") }
        }
}

/**
 * The full player as the boards draw it: a sheet over the library, which the scrim dims, with [staging]'s track in the
 * bar behind it and the queue's Up next under the player.
 */
@Composable
private fun FullPlayerOverLibrary(
    staging: PlayerStaging,
    volumeRow: @Composable () -> Unit,
) = SheetScene(behind = { PlayingLibraryScreen(nowPlayingBar = { Bar(staging) }) }) {
    Covers {
        AndroidFullPlayer(
            track = staging.track,
            failure = null,
            onCollapse = {},
            seek = { staging.SeekRow(SeekLineLook.LARGE) },
            transport = {
                AndroidTransport(true, staging.waitsOnStorage, onPrevious = {}, onPlayPause = {}, onNext = {}, full = true)
            },
            secondaryControls = { ExpandedSecondaryControls(BridgeRepeatMode.OFF, onCycleRepeat = {}) },
            volumeRow = volumeRow,
        )
        AndroidQueueSection(
            title = stringResource(R.string.queue_section_up_next),
            clearDescription = stringResource(R.string.queue_clear_up_next),
            onClear = {},
        )
    }
}
