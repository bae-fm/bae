package fm.bae.app.playback

import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgePlaybackValueState
import uniffi.bae_bridge.BridgeTimelinePoint
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackSeekProjectionTest {
    @Test
    fun inTrackSeekPublishesDropPositionUntilCoreProgressCatchesUp() {
        val (player, handle) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(0.75, handle.seekRatios.single(), 0.0)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onProgress(positionMs = 20_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onProgress(positionMs = 75_050, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onProgress(positionMs = 80_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onSeeked(positionMs = 75_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onProgress(positionMs = 80_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 80_000, durationMs = 100_000)
    }

    /** Each line core tells moves Media3's position, which moves on by itself
     *  from there; a seek's line does too. */
    @Test
    fun eachLineIsToldToMedia3() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(10_000L, player.currentPosition)

        player.onSeeked(positionMs = 75_000, durationMs = 100_000)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(75_000L, player.currentPosition)
    }

    @Test
    fun forwardProgressPastTargetDoesNotClearProjectionUntilSeeked() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()

        player.onProgress(positionMs = 80_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)

        player.onSeeked(positionMs = 75_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)
    }

    @Test
    fun sameTrackLoadingDuringSeekKeepsProjectedPosition() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()
        player.applyPlaybackState(
            loadingState(
                prepared = nowPlayingTrack(trackId = "track-1", durationMs = 100_000uL),
            ),
        )

        player.onProgress(positionMs = 20_000, durationMs = 100_000)

        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)
    }

    @Test
    fun sameTrackPlayingDuringSeekKeepsProjectedPosition() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()
        player.startPlaying(durationMs = 100_000uL)

        player.onProgress(positionMs = 20_000, durationMs = 100_000)

        assertMedia3Position(player, positionMs = 75_000, durationMs = 100_000)
    }

    @Test
    fun backwardInTrackSeekRejectsStaleForwardProgress() {
        val (player, handle) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 80_000, durationMs = 100_000)

        player.seekTo(25_000)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(0.25, handle.seekRatios.single(), 0.0)
        assertMedia3Position(player, positionMs = 25_000, durationMs = 100_000)

        player.onProgress(positionMs = 82_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 25_000, durationMs = 100_000)

        player.onProgress(positionMs = 25_000, durationMs = 100_000)
        assertMedia3Position(player, positionMs = 25_000, durationMs = 100_000)
    }

    @Test
    fun inTrackSeekWithoutDurationDoesNotChangeProjectedPosition() {
        val (player, handle) = player()

        player.startPlaying(durationMs = null)
        player.onProgress(positionMs = 10_000, durationMs = null)

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
        player.onProgress(positionMs = 25_000, durationMs = 100_000)

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
        player.onQueueValue(manual = emptyList(), context = null, hasNext = true, hasPrevious = false)

        player.seekToNext()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.nextTrackCalls)
    }

    @Test
    fun previousTrackFailureDoesNotEscapeSeekHandler() {
        val (player, handle) = player()
        handle.previousTrackFailure = RuntimeException("previous failed")
        player.onQueueValue(manual = emptyList(), context = null, hasNext = false, hasPrevious = true)

        player.seekToPrevious()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.previousTrackCalls)
    }

    private fun player(): Pair<BaeCorePlayer, SeekRecordingHandle> {
        val context = RuntimeEnvironment.getApplication()
        val handle = SeekRecordingHandle()
        val player =
            BaeCorePlayer(
                applicationLooper = Looper.getMainLooper(),
                appHandle = handle,
                system = PlaybackSystem(context, AudioFocus(context), { false }),
                scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
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

    private fun BaeCorePlayer.onProgress(
        positionMs: Long,
        durationMs: Long?,
    ) {
        applyTimeline(timeline(line("track-1", positionMs, durationMs)))
    }

    private fun BaeCorePlayer.onSeeked(
        positionMs: Long,
        durationMs: Long?,
    ) {
        applyTimeline(timeline(line("track-1", positionMs, durationMs), seekRevision = 1u))
    }

    private fun line(
        trackId: String,
        positionMs: Long,
        durationMs: Long?,
        moving: Boolean = true,
    ) = BridgeTimelinePoint(trackId, positionMs.toULong(), durationMs?.toULong(), moving)

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
