package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.clockText
import fm.bae.app.currentLocale
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.playback.AndroidSeekBar
import fm.bae.app.ui.playback.SeekBarState
import fm.bae.app.ui.playback.SeekLineLook
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.progressAt
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeTrackBuffered
import uniffi.bae_bridge.BridgeTrackPosition
import uniffi.bae_bridge.BridgeTrackSpan

private const val TRACK_ID = "shot-track"
private const val DURATION_MS = 303_000L
private const val MILLIS_PER_SECOND = 1_000L
private const val SECONDS_PER_MINUTE = 60L

/**
 * A playing track as a player draws it: where its line is, the seconds its clock says, which parts of it are
 * here, whether its load waits on storage. The boards give the line and the clock apart, as the design
 * system's previews do, so the clock is not worked out from the line: the stream holds a position standing at
 * the line, and the clock is the seconds given.
 */
internal class PlayerStaging(
    progress: Double,
    private val elapsedSeconds: Long,
    held: List<BridgeTrackSpan>,
    waitsOnStorage: Boolean = false,
) {
    private val position =
        BridgeTrackPosition(
            positionMs = (progress * DURATION_MS).toLong(),
            rate = 0.0,
            anchoredAtNs = 0uL,
            durationMs = DURATION_MS.toULong(),
        )
    val positions = LatestStream.trackPositions(mapOf(TRACK_ID to position)).source(TRACK_ID)
    val buffered = LatestStream.trackBuffered(mapOf(TRACK_ID to BridgeTrackBuffered(held))).source(TRACK_ID)
    val storageWait =
        LatestStream.trackStorageWaits(if (waitsOnStorage) setOf(TRACK_ID) else emptySet()).source(TRACK_ID)

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
    fun SeekRow(look: SeekLineLook = SeekLineLook.BAR) {
        val locale = LocalContext.current.currentLocale()
        AndroidSeekBar(
            position = positions,
            buffered = buffered,
            state = { shown, frameTimeNanos ->
                SeekBarState(
                    progress = shown?.progressAt(frameTimeNanos) ?: 0.0,
                    leading = clockText(clock(if (shown == null) 0L else elapsedSeconds * MILLIS_PER_SECOND), locale),
                    trailing = clockText(clock(DURATION_MS), locale),
                    seekable = true,
                )
            },
            onSeekRatio = {},
            onToggleRemaining = {},
            look = look,
        )
    }

    private fun clock(ms: Long): BridgeDurationClock {
        val seconds = ms / MILLIS_PER_SECOND
        return BridgeDurationClock(
            negative = false,
            hours = null,
            minutes = (seconds / SECONDS_PER_MINUTE).toUInt(),
            seconds = (seconds % SECONDS_PER_MINUTE).toUInt(),
        )
    }
}
