package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import fm.bae.app.clockText
import fm.bae.app.creditLineNamed
import fm.bae.app.currentLocale
import fm.bae.app.playback.NowPlaying
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.playback.AndroidSeekBar
import fm.bae.app.ui.playback.SeekBarState
import fm.bae.app.ui.playback.SeekLineLook
import fm.bae.fastvalues.LatestStream
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeNowPlayingDisplay
import uniffi.bae_bridge.BridgeTrackBuffered
import uniffi.bae_bridge.BridgeTrackDisplay
import uniffi.bae_bridge.BridgeTrackPosition
import uniffi.bae_bridge.BridgeTrackSpan

private const val TRACK_ID = "shot-track"
private const val DEFAULT_DURATION_SECONDS = 303L
private const val MILLIS_PER_SECOND = 1_000L
private const val SECONDS_PER_MINUTE = 60L

/**
 * A playing track as a player draws it: the seconds its clock says, which parts of it are here, whether its
 * load waits on storage. The line sits at the clock's share of the track, as the design system's seek bar
 * draws it from its elapsed time and its duration. With [removedFromLibrary], the library no longer holds the track, and with [noCover] it has no cover. The
 * track is called [title] and lasts [durationSeconds].
 */
internal class PlayerStaging(
    elapsedSeconds: Long,
    held: List<BridgeTrackSpan>,
    val waitsOnStorage: Boolean = false,
    removedFromLibrary: Boolean = false,
    noCover: Boolean = false,
    title: String = "Low Tide Hours",
    durationSeconds: Long = DEFAULT_DURATION_SECONDS,
) {
    private val durationMs = durationSeconds * MILLIS_PER_SECOND

    private val position =
        BridgeTrackPosition(
            positionMs = elapsedSeconds * MILLIS_PER_SECOND,
            durationMs = durationMs.toULong(),
            progress = (elapsedSeconds * MILLIS_PER_SECOND).toDouble() / durationMs,
            seeks = 0uL,
        )
    val positions = LatestStream.trackPositions(mapOf(TRACK_ID to position)).source(TRACK_ID)
    val buffered = LatestStream.trackBuffered(mapOf(TRACK_ID to BridgeTrackBuffered(held))).source(TRACK_ID)

    val cover = PreviewData.imageRef("rel-1")

    val track =
        NowPlaying(
            trackId = TRACK_ID,
            display =
                if (removedFromLibrary) {
                    BridgeNowPlayingDisplay.Removed
                } else {
                    BridgeNowPlayingDisplay.Library(
                        BridgeTrackDisplay(
                            title = title,
                            artistLine = creditLineNamed("Mara Quill"),
                            albumId = "alb-1",
                            releaseId = "rel-1",
                            albumTitle = "Northbound",
                            coverImage = if (noCover) null else cover,
                        ),
                    )
                },
            sidePausePrompt = null,
        )

    /** The seek row, with its clocks as the bar's words give them. */
    @Composable
    fun SeekRow(look: SeekLineLook = SeekLineLook.BAR) {
        val locale = LocalContext.current.currentLocale()
        AndroidSeekBar(
            position = positions,
            buffered = buffered,
            state = { shown ->
                SeekBarState(
                    progress = shown?.progress ?: 0.0,
                    leading = clockText(clock(shown?.positionMs ?: 0L), locale),
                    trailing = clockText(clock(durationMs), locale),
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

/** A track a third of the way in, with its first 60% buffered. */
internal val playing =
    PlayerStaging(elapsedSeconds = 102, held = listOf(BridgeTrackSpan(0.0, 0.6)))

/** A track that has just started, with little buffered, waiting on storage for the rest. */
internal val waiting =
    PlayerStaging(
        elapsedSeconds = 5,
        held = listOf(BridgeTrackSpan(0.0, 0.04)),
        waitsOnStorage = true,
    )

/** [playing], while its load waits on storage, as the full player's preview draws it. */
internal val playingWaitingOnStorage =
    PlayerStaging(elapsedSeconds = 102, held = listOf(BridgeTrackSpan(0.0, 0.6)), waitsOnStorage = true)

/** [playing], for a track with no cover. */
internal val withoutCover =
    PlayerStaging(elapsedSeconds = 102, held = listOf(BridgeTrackSpan(0.0, 0.6)), noCover = true)

/** [playing], for a track whose title is long. */
internal val longTitled =
    PlayerStaging(
        elapsedSeconds = 102,
        held = listOf(BridgeTrackSpan(0.0, 0.6)),
        title = "Low Tide Hours, the long version recorded live at the harbor",
    )
