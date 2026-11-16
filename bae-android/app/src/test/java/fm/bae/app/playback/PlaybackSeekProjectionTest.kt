package fm.bae.app.playback

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.plainCreditText
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgeQueueSnapshot
import uniffi.bae_bridge.BridgeTimelinePoint
import java.time.Duration

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackSeekProjectionTest {
    /** What the player's scope reported, as the session scope's handler reports bugs. */
    private val reported = mutableListOf<Throwable>()

    /**
     * After a seek, Media3 shows the seek's target until core sends a line that playback started at a
     * seek. Core sent every other line before playback applied the seek, even one at the target.
     */
    @Test
    fun inTrackSeekPublishesDropPositionUntilALineStartedAtTheSeek() {
        val (player, handle) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onLine(positionMs = 10_000, durationMs = 100_000)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(0.75, handle.seekRatios.single(), 0.0)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onLine(positionMs = 20_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onLine(positionMs = 75_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onSeekLine(positionMs = 74_999, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 74_999, durationMs = 100_000)

        player.onLine(positionMs = 80_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 80_000, durationMs = 100_000)
    }

    /** Each line core sends moves Media3's position, which moves on by itself
     *  from there, and a line playback started at a seek does too. */
    @Test
    fun eachLineIsToldToMedia3() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onLine(positionMs = 10_000, durationMs = 100_000)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(10_000L, player.currentPosition)

        player.onSeekLine(positionMs = 75_000, durationMs = 100_000)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(75_000L, player.currentPosition)
    }

    @Test
    fun aLinePastTheTargetDoesNotClearProjectionUntilALineStartedAtTheSeek() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onLine(positionMs = 10_000, durationMs = 100_000)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()

        player.onLine(positionMs = 80_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onSeekLine(positionMs = 75_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onLine(positionMs = 80_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 80_000, durationMs = 100_000)
    }

    @Test
    fun sameTrackLoadingDuringSeekKeepsProjectedPosition() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onLine(positionMs = 10_000, durationMs = 100_000)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()
        player.applyPlaybackState(
            loadingState(
                prepared = nowPlayingTrack(trackId = "track-1", durationMs = 100_000uL),
            ),
        )

        player.onLine(positionMs = 20_000, durationMs = 100_000)

        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)
    }

    @Test
    fun sameTrackPlayingDuringSeekKeepsProjectedPosition() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onLine(positionMs = 10_000, durationMs = 100_000)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()
        player.startPlaying(durationMs = 100_000uL)

        player.onLine(positionMs = 20_000, durationMs = 100_000)

        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)
    }

    @Test
    fun backwardInTrackSeekRejectsStaleForwardProgress() {
        val (player, handle) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onLine(positionMs = 80_000, durationMs = 100_000)

        player.seekTo(25_000)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(0.25, handle.seekRatios.single(), 0.0)
        assertMedia3Position(player, positionMs = 25_000, durationMs = 100_000)

        player.onLine(positionMs = 82_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 25_000, durationMs = 100_000)

        player.onLine(positionMs = 25_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 25_000, durationMs = 100_000)
    }

    @Test
    fun inTrackSeekWithoutDurationDoesNotChangeProjectedPosition() {
        val (player, handle) = player()

        player.startPlaying(durationMs = null)
        player.onLine(positionMs = 10_000, durationMs = null)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(emptyList<Double>(), handle.seekRatios)
        assertEquals(10_000L, player.contentPosition)
    }

    /** Through a pregap core's line stands at the track's start while the track plays, and Media3's position with it. */
    @Test
    fun aStandingLineHoldsMedia3sPositionWhileTheTrackPlays() {
        val (player, _) = player()
        player.startPlaying(durationMs = 100_000uL)

        player.applyTimeline(timeline(line("track-1", 0, 100_000, moving = false)))
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(1_500))
        assertEquals(0L, player.contentPosition)

        player.applyTimeline(timeline(line("track-1", 0, 100_000)))
        shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(1_000))
        assertEquals(1_000L, player.contentPosition)
    }

    @Test
    fun stoppedPlaybackResetsProjectedPosition() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onLine(positionMs = 25_000, durationMs = 100_000)

        player.applyPlaybackState(BridgePlaybackValueState.Stopped)

        assertEquals(0L, player.contentPosition)
    }

    @Test
    fun aPositionThatComesBeforeItsTrackShowsOnceThatTrackIsCurrent() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.applyTimeline(timeline(line("track-2", 30_000, 200_000)))
        assertMedia3Position(player, positionMs = 0, durationMs = 100_000)

        player.applyPlaybackState(playingState(trackId = "track-2", durationMs = 200_000uL))
        assertMedia3Position(player, positionMs = 30_000, durationMs = 200_000)
    }

    @Test
    fun nextTrackFailureDoesNotEscapeSeekHandler() {
        val (player, handle) = player()
        handle.nextTrackFailure = RuntimeException("next failed")
        player.onQueueValue(
            BridgeQueueSnapshot(
                manual = emptyList(),
                context = null,
                shuffled = false,
                hasNext = true,
                hasPrevious = false,
                upNextCount = 0u,
                isEmpty = true,
            ),
        )

        player.seekToNext()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.nextTrackCalls)
        assertEquals("next failed", reported.single().message)
    }

    @Test
    fun previousTrackFailureDoesNotEscapeSeekHandler() {
        val (player, handle) = player()
        handle.previousTrackFailure = RuntimeException("previous failed")
        player.onQueueValue(
            BridgeQueueSnapshot(
                manual = emptyList(),
                context = null,
                shuffled = false,
                hasNext = false,
                hasPrevious = true,
                upNextCount = 0u,
                isEmpty = true,
            ),
        )

        player.seekToPrevious()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.previousTrackCalls)
        assertEquals("previous failed", reported.single().message)
    }

    private fun player(): Pair<BaeCorePlayer, SeekRecordingHandle> {
        val context = RuntimeEnvironment.getApplication()
        val handle = SeekRecordingHandle()
        val player =
            BaeCorePlayer(
                applicationLooper = Looper.getMainLooper(),
                appHandle = handle,
                system = PlaybackSystem(context, AudioFocus(context), { false }),
                creditLineText = ::plainCreditText,
                scope =
                    CoroutineScope(
                        SupervisorJob() + Dispatchers.Main.immediate + CoroutineExceptionHandler { _, error -> reported += error },
                    ),
            )
        return player to handle
    }

    private fun BaeCorePlayer.startPlaying(durationMs: ULong?) {
        applyPlaybackState(
            playingState(
                trackId = "track-1",
                title = "Track Title",
                artistNames = "Artist Name",
                albumId = "album-1",
                releaseId = "release-1",
                albumTitle = "Album Title",
                coverImage = null,
                durationMs = durationMs,
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()
    }

    /** Applies a timeline whose line for track-1 moves on from [positionMs]. */
    private fun BaeCorePlayer.onLine(
        positionMs: Long,
        durationMs: Long?,
    ) {
        applyTimeline(timeline(line("track-1", positionMs, durationMs)))
    }

    /** Applies a timeline whose line for track-1 playback started at a seek to [positionMs]. */
    private fun BaeCorePlayer.onSeekLine(
        positionMs: Long,
        durationMs: Long?,
    ) {
        applyTimeline(timeline(line("track-1", positionMs, durationMs, startedAtSeek = true)))
    }

    private fun line(
        trackId: String,
        positionMs: Long,
        durationMs: Long?,
        moving: Boolean = true,
        startedAtSeek: Boolean = false,
    ) = BridgeTimelinePoint(trackId, positionMs.toULong(), durationMs?.toULong(), moving, startedAtSeek)

    /** Where Media3 shows the track: the anchor it moves on from, and the length. */
    private fun assertMedia3Position(
        player: BaeCorePlayer,
        positionMs: Long,
        durationMs: Long,
    ) {
        assertEquals(positionMs, player.contentPosition)
        assertEquals(durationMs, player.duration)
    }

    private class SeekRecordingHandle : FakeAppHandle() {
        val seekRatios = mutableListOf<Double>()
        var nextTrackCalls = 0
        var previousTrackCalls = 0
        var nextTrackFailure: RuntimeException? = null
        var previousTrackFailure: RuntimeException? = null

        override fun seekByRatio(ratio: Double) {
            seekRatios += ratio
        }

        override fun nextTrack() {
            nextTrackCalls++
            nextTrackFailure?.let { throw it }
        }

        override fun previousTrack() {
            previousTrackCalls++
            previousTrackFailure?.let { throw it }
        }
    }
}
