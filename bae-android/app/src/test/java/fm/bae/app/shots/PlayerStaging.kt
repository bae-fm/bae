package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.clockText
import fm.bae.app.currentLocale
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.playback.AndroidSeekBar
import fm.bae.app.ui.playback.SeekBarState
import fm.bae.fastvalues.LatestStream
import uniffi.bae_bridge.BridgeDurationClock

private const val TRACK_ID = "shot-track"
private const val TRACK_MINUTES = 5u
private const val TRACK_SECONDS = 3u

/**
 * A playing track as a player draws it. Its streams hold nothing: a View following a stream that holds a
 * value asks the Choreographer for a frame after every frame, so the test never goes idle. The seek row
 * shows its total time and no bar, and the play and pause control no wait arc.
 */
internal class PlayerStaging {
    val positions = LatestStream.trackPositions().source(TRACK_ID)
    val buffered = LatestStream.trackBuffered().source(TRACK_ID)
    val storageWait = LatestStream.trackStorageWaits().source(TRACK_ID)

    val track =
        NowPlaying(
            trackId = TRACK_ID,
            title = "Low Tide Hours",
            artist = "Mara Quill",
            coverImage = PreviewData.imageRef("rel-1"),
            sidePausePrompt = null,
        )

    /** The seek row, with its clocks as the bar's words give them. */
    @Composable
    fun SeekRow() {
        val locale = LocalContext.current.currentLocale()
        AndroidSeekBar(
            position = positions,
            buffered = buffered,
            state = {
                SeekBarState(
                    progress = 0.0,
                    leading = "",
                    trailing = clockText(BridgeDurationClock(false, null, TRACK_MINUTES, TRACK_SECONDS), locale),
                    seekable = true,
                )
            },
            onSeekRatio = {},
            onToggleRemaining = {},
        )
    }
}
