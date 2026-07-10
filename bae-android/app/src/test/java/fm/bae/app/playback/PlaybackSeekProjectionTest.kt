package fm.bae.app.playback

import android.os.Looper
import fm.bae.app.formatDurationMs
import fm.bae.app.formatRemainingMs
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
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeLoadingTrackInfo
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.UiEventCallback

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackSeekProjectionTest {
    @Test
    fun inTrackSeekPublishesDropPositionUntilCoreProgressCatchesUp() {
        val (player, handle) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000, progress = 0.10)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(0.75, handle.seekRatios.single(), 0.0)
        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )

        player.onProgress(positionMs = 20_000, durationMs = 100_000, progress = 0.20)
        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )

        player.onProgress(positionMs = 75_050, durationMs = 100_000, progress = 0.7505)
        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )

        player.onProgress(positionMs = 80_000, durationMs = 100_000, progress = 0.80)
        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )

        player.onSeeked(positionMs = 75_000, durationMs = 100_000, progress = 0.75)
        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )

        player.onProgress(positionMs = 80_000, durationMs = 100_000, progress = 0.80)
        assertPosition(
            player.position.value,
            progress = 0.80,
            elapsed = formatDurationMs(80_000),
            remaining = formatRemainingMs(80_000, 100_000),
        )
    }

    @Test
    fun forwardProgressPastTargetDoesNotClearProjectionUntilSeeked() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000, progress = 0.10)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()

        player.onProgress(positionMs = 80_000, durationMs = 100_000, progress = 0.80)
        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )

        player.onSeeked(positionMs = 75_000, durationMs = 100_000, progress = 0.75)
        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )
    }

    @Test
    fun sameTrackLoadingDuringSeekKeepsProjectedPosition() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000, progress = 0.10)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()
        player.onLoading(
            trackId = "track-1",
            track =
                BridgeLoadingTrackInfo(
                    trackTitle = "Track Title",
                    artistNames = "Artist Name",
                    albumId = "album-1",
                    albumTitle = "Album Title",
                    coverImageId = null,
                    durationMs = 100_000uL,
                ),
        )

        player.onProgress(positionMs = 20_000, durationMs = 100_000, progress = 0.20)

        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )
    }

    @Test
    fun sameTrackPlayingDuringSeekKeepsProjectedPosition() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 10_000, durationMs = 100_000, progress = 0.10)

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()
        player.startPlaying(durationMs = 100_000uL)

        player.onProgress(positionMs = 20_000, durationMs = 100_000, progress = 0.20)

        assertPosition(
            player.position.value,
            progress = 0.75,
            elapsed = formatDurationMs(75_000),
            remaining = formatRemainingMs(75_000, 100_000),
        )
    }

    @Test
    fun backwardInTrackSeekRejectsStaleForwardProgress() {
        val (player, handle) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 80_000, durationMs = 100_000, progress = 0.80)

        player.seekTo(25_000)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(0.25, handle.seekRatios.single(), 0.0)
        assertPosition(
            player.position.value,
            progress = 0.25,
            elapsed = formatDurationMs(25_000),
            remaining = formatRemainingMs(25_000, 100_000),
        )

        player.onProgress(positionMs = 82_000, durationMs = 100_000, progress = 0.82)
        assertPosition(
            player.position.value,
            progress = 0.25,
            elapsed = formatDurationMs(25_000),
            remaining = formatRemainingMs(25_000, 100_000),
        )

        player.onProgress(positionMs = 25_000, durationMs = 100_000, progress = 0.25)
        assertPosition(
            player.position.value,
            progress = 0.25,
            elapsed = formatDurationMs(25_000),
            remaining = formatRemainingMs(25_000, 100_000),
        )
    }

    @Test
    fun inTrackSeekWithoutDurationDoesNotChangeProjectedPosition() {
        val (player, handle) = player()

        player.startPlaying(durationMs = 0uL)
        player.onProgress(positionMs = 10_000, durationMs = 0, progress = 0.0)
        val positionBeforeSeek = player.position.value

        player.seekTo(75_000)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(emptyList<Double>(), handle.seekRatios)
        assertEquals(positionBeforeSeek, player.position.value)
    }

    @Test
    fun stoppedPlaybackResetsProjectedPositionLabels() {
        val (player, _) = player()

        player.startPlaying(durationMs = 100_000uL)
        player.onProgress(positionMs = 25_000, durationMs = 100_000, progress = 0.25)

        player.onStopped()

        assertPosition(
            player.position.value,
            progress = 0.0,
            elapsed = "",
            remaining = "",
        )
    }

    @Test
    fun nextTrackFailureDoesNotEscapeSeekHandler() {
        val (player, handle) = player()
        handle.nextTrackFailure = RuntimeException("next failed")
        player.onQueueUpdated(manual = emptyList(), context = null, hasNext = true, hasPrevious = false, revision = 1uL)

        player.seekToNext()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, handle.nextTrackCalls)
    }

    @Test
    fun previousTrackFailureDoesNotEscapeSeekHandler() {
        val (player, handle) = player()
        handle.previousTrackFailure = RuntimeException("previous failed")
        player.onQueueUpdated(manual = emptyList(), context = null, hasNext = false, hasPrevious = true, revision = 1uL)

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
                context = context,
                scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
                isAppForeground = { false },
            )
        return player to handle
    }

    private fun BaeCorePlayer.startPlaying(durationMs: ULong) {
        onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                trackId = "track-1",
                trackTitle = "Track Title",
                artistNames = "Artist Name",
                artistId = "artist-1",
                albumId = "album-1",
                albumTitle = "Album Title",
                coverImageId = null,
                durationMs = durationMs,
            ),
        )
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun BaeCorePlayer.onProgress(
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    ) {
        onProgress("track-1", positionMs, durationMs, progress)
    }

    private fun BaeCorePlayer.onSeeked(
        positionMs: Long,
        durationMs: Long,
        progress: Double,
    ) {
        onSeeked("track-1", positionMs, durationMs, progress)
    }

    private fun assertPosition(
        position: PlaybackPosition,
        progress: Double,
        elapsed: String,
        remaining: String,
    ) {
        assertEquals(progress, position.progress, 0.0)
        assertEquals(elapsed, position.elapsedLabel)
        assertEquals(remaining, position.remainingLabel)
    }

    private class SeekRecordingHandle : AppHandle(NoHandle) {
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

        override suspend fun savePlaybackState() {}

        override fun subscribeUiEvents(callback: UiEventCallback) {}
    }
}
