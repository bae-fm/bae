package fm.bae.app.playback

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PlaybackPositionModelTest {
    @Test
    fun inTrackSeekProjectsDroppedPositionUntilSeekedCatchesUp() {
        val model = playingModel(durationMs = 100_000)
        assertEquals(PositionUpdate.Applied, model.progress(positionMs = 10_000))
        assertPosition(model, positionMs = 10_000, durationMs = 100_000)

        val ratio = model.beginInTrackSeek(TRACK, requestedPositionMs = 75_000)
        assertEquals(0.75, ratio!!, 0.0)
        assertEquals(75_000, model.effectivePositionMs)
        assertPosition(model, positionMs = 75_000, durationMs = 100_000)

        // Live progress is held by the projection until core confirms the seek.
        assertEquals(PositionUpdate.HeldByPendingSeek, model.progress(positionMs = 20_000))
        assertPosition(model, positionMs = 75_000, durationMs = 100_000)

        // Progress overshooting the target is still held until the seek confirms.
        assertEquals(PositionUpdate.HeldByPendingSeek, model.progress(positionMs = 80_000))
        assertPosition(model, positionMs = 75_000, durationMs = 100_000)

        assertEquals(PositionUpdate.Applied, model.seeked(positionMs = 75_000))
        assertPosition(model, positionMs = 75_000, durationMs = 100_000)

        // With the seek confirmed, live progress advances the anchor again.
        assertEquals(PositionUpdate.Applied, model.progress(positionMs = 80_000))
        assertPosition(model, positionMs = 80_000, durationMs = 100_000)
    }

    @Test
    fun backwardSeekHoldsStaleForwardProgress() {
        val model = playingModel(durationMs = 100_000)
        model.progress(positionMs = 80_000)

        assertEquals(0.25, model.beginInTrackSeek(TRACK, requestedPositionMs = 25_000)!!, 0.0)
        assertPosition(model, positionMs = 25_000, durationMs = 100_000)

        assertEquals(PositionUpdate.HeldByPendingSeek, model.progress(positionMs = 82_000))
        assertPosition(model, positionMs = 25_000, durationMs = 100_000)
    }

    @Test
    fun sameTrackActivationKeepsProjection() {
        val model = playingModel(durationMs = 100_000)
        model.beginInTrackSeek(TRACK, requestedPositionMs = 75_000)

        model.setActiveTrack(trackChanged = false, durationMs = 100_000)

        assertEquals(PositionUpdate.HeldByPendingSeek, model.progress(positionMs = 20_000))
        assertEquals(75_000, model.effectivePositionMs)
    }

    @Test
    fun trackChangeDropsProjection() {
        val model = playingModel(durationMs = 100_000)
        model.beginInTrackSeek(TRACK, requestedPositionMs = 75_000)

        model.setActiveTrack(trackChanged = true, durationMs = 100_000)

        // The seek projection is gone, so live progress applies immediately.
        assertEquals(PositionUpdate.Applied, model.progress(positionMs = 20_000))
        assertEquals(20_000, model.effectivePositionMs)
    }

    @Test
    fun seekWithoutDurationIsIgnored() {
        val model = playingModel(durationMs = null)
        model.progress(positionMs = 10_000, durationMs = null)

        assertNull(model.beginInTrackSeek(TRACK, requestedPositionMs = 75_000))
        // No projection formed, so the anchor stays where the last progress left it.
        assertEquals(10_000, model.effectivePositionMs)
    }

    @Test
    fun seekClampsRequestedPositionIntoTrack() {
        val model = playingModel(durationMs = 100_000)

        assertEquals(1.0, model.beginInTrackSeek(TRACK, requestedPositionMs = 150_000)!!, 0.0)
        assertEquals(100_000, model.effectivePositionMs)

        assertEquals(0.0, model.beginInTrackSeek(TRACK, requestedPositionMs = -5_000)!!, 0.0)
        assertEquals(0, model.effectivePositionMs)
    }

    @Test
    fun progressForDifferentTrackIsStale() {
        val model = playingModel(durationMs = 100_000)

        assertEquals(
            PositionUpdate.StaleTrack,
            model.onProgress(currentTrackId = TRACK, trackId = "other", positionMs = 30_000, durationMs = 100_000, moving = true),
        )
        assertEquals(
            PositionUpdate.StaleTrack,
            model.onSeeked(currentTrackId = TRACK, trackId = "other", positionMs = 30_000, durationMs = 100_000, moving = true),
        )
        // A stale update never moved the anchor.
        assertEquals(0, model.effectivePositionMs)
    }

    @Test
    fun noCurrentTrackAcceptsAnyProgress() {
        val model = PlaybackPositionModel()

        assertEquals(
            PositionUpdate.Applied,
            model.onProgress(currentTrackId = null, trackId = TRACK, positionMs = 30_000, durationMs = 100_000, moving = true),
        )
        assertEquals(30_000, model.effectivePositionMs)
    }

    @Test
    fun resetClearsPositionState() {
        val model = playingModel(durationMs = 100_000)
        model.progress(positionMs = 25_000)

        model.reset()

        assertEquals(0, model.effectivePositionMs)
        assertNull(model.durationMs)
    }

    private fun playingModel(durationMs: Long?): PlaybackPositionModel =
        PlaybackPositionModel().apply { setActiveTrack(trackChanged = true, durationMs = durationMs) }

    private fun PlaybackPositionModel.progress(
        positionMs: Long,
        durationMs: Long? = 100_000,
    ): PositionUpdate = onProgress(currentTrackId = TRACK, trackId = TRACK, positionMs = positionMs, durationMs = durationMs, moving = true)

    private fun PlaybackPositionModel.seeked(positionMs: Long): PositionUpdate =
        onSeeked(currentTrackId = TRACK, trackId = TRACK, positionMs = positionMs, durationMs = 100_000, moving = true)

    /** Where the model puts Media3's position, and the length it holds. */
    private fun assertPosition(
        model: PlaybackPositionModel,
        positionMs: Long,
        durationMs: Long?,
    ) {
        assertEquals(positionMs, model.effectivePositionMs)
        assertEquals(durationMs, model.durationMs)
    }

    private companion object {
        const val TRACK = "track-1"
    }
}
