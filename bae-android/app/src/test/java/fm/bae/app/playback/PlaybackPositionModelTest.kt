package fm.bae.app.playback

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PlaybackPositionModelTest {
    @Test
    fun inTrackSeekProjectsDroppedPositionUntilSeekedCatchesUp() {
        val model = playingModel(durationMs = 100_000)
        assertEquals(PositionUpdate.Applied, model.progress(positionMs = 10_000, progress = 0.10))
        assertPosition(model, progress = 0.10, positionMs = 10_000, durationMs = 100_000)

        val ratio = model.beginInTrackSeek(TRACK, requestedPositionMs = 75_000)
        assertEquals(0.75, ratio!!, 0.0)
        assertEquals(75_000, model.effectivePositionMs)
        assertPosition(model, progress = 0.75, positionMs = 75_000, durationMs = 100_000)

        // Live progress is held by the projection until core confirms the seek.
        assertEquals(PositionUpdate.HeldByPendingSeek, model.progress(positionMs = 20_000, progress = 0.20))
        assertPosition(model, progress = 0.75, positionMs = 75_000, durationMs = 100_000)

        // Progress overshooting the target is still held until the seek confirms.
        assertEquals(PositionUpdate.HeldByPendingSeek, model.progress(positionMs = 80_000, progress = 0.80))
        assertPosition(model, progress = 0.75, positionMs = 75_000, durationMs = 100_000)

        assertEquals(PositionUpdate.Applied, model.seeked(positionMs = 75_000, progress = 0.75))
        assertPosition(model, progress = 0.75, positionMs = 75_000, durationMs = 100_000)

        // With the seek confirmed, live progress advances the anchor again.
        assertEquals(PositionUpdate.Applied, model.progress(positionMs = 80_000, progress = 0.80))
        assertPosition(model, progress = 0.80, positionMs = 80_000, durationMs = 100_000)
    }

    @Test
    fun backwardSeekHoldsStaleForwardProgress() {
        val model = playingModel(durationMs = 100_000)
        model.progress(positionMs = 80_000, progress = 0.80)

        assertEquals(0.25, model.beginInTrackSeek(TRACK, requestedPositionMs = 25_000)!!, 0.0)
        assertPosition(model, progress = 0.25, positionMs = 25_000, durationMs = 100_000)

        assertEquals(PositionUpdate.HeldByPendingSeek, model.progress(positionMs = 82_000, progress = 0.82))
        assertPosition(model, progress = 0.25, positionMs = 25_000, durationMs = 100_000)
    }

    @Test
    fun sameTrackActivationKeepsProjection() {
        val model = playingModel(durationMs = 100_000)
        model.beginInTrackSeek(TRACK, requestedPositionMs = 75_000)

        model.setActiveTrack(trackChanged = false, rawDurationMs = 100_000)

        assertEquals(PositionUpdate.HeldByPendingSeek, model.progress(positionMs = 20_000, progress = 0.20))
        assertEquals(75_000, model.effectivePositionMs)
    }

    @Test
    fun trackChangeDropsProjection() {
        val model = playingModel(durationMs = 100_000)
        model.beginInTrackSeek(TRACK, requestedPositionMs = 75_000)

        model.setActiveTrack(trackChanged = true, rawDurationMs = 100_000)

        // The seek projection is gone, so live progress applies immediately.
        assertEquals(PositionUpdate.Applied, model.progress(positionMs = 20_000, progress = 0.20))
        assertEquals(20_000, model.effectivePositionMs)
    }

    @Test
    fun seekWithoutDurationIsIgnored() {
        val model = playingModel(durationMs = 0)
        model.progress(positionMs = 10_000, durationMs = 0, progress = 0.0)

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
            model.onProgress(currentTrackId = TRACK, trackId = "other", positionMs = 30_000, durationMs = 100_000, progress = 0.30),
        )
        assertEquals(
            PositionUpdate.StaleTrack,
            model.onSeeked(currentTrackId = TRACK, trackId = "other", positionMs = 30_000, durationMs = 100_000, progress = 0.30),
        )
        // A stale update never moved the anchor.
        assertEquals(0, model.effectivePositionMs)
    }

    @Test
    fun noCurrentTrackAcceptsAnyProgress() {
        val model = PlaybackPositionModel()

        assertEquals(
            PositionUpdate.Applied,
            model.onProgress(currentTrackId = null, trackId = TRACK, positionMs = 30_000, durationMs = 100_000, progress = 0.30),
        )
        assertEquals(30_000, model.effectivePositionMs)
    }

    @Test
    fun unknownDurationUsesRawProgressForSlider() {
        val model = playingModel(durationMs = 0)
        model.progress(positionMs = 10_000, durationMs = 0, progress = 0.33)

        val position = model.position(hasCurrentTrack = true)
        assertEquals(0.33, position.progress, 0.0)
        assertEquals(10_000L, position.positionMs)
        assertNull(position.durationMs)
    }

    @Test
    fun resetClearsPositionState() {
        val model = playingModel(durationMs = 100_000)
        model.progress(positionMs = 25_000, progress = 0.25)

        model.reset()

        assertEquals(0, model.effectivePositionMs)
        assertNull(model.durationMs)
        assertPosition(model, progress = 0.0, positionMs = 0, durationMs = null, hasCurrentTrack = false)
    }

    private fun playingModel(durationMs: Long): PlaybackPositionModel =
        PlaybackPositionModel().apply { setActiveTrack(trackChanged = true, rawDurationMs = durationMs) }

    private fun PlaybackPositionModel.progress(
        positionMs: Long,
        durationMs: Long = 100_000,
        progress: Double,
    ): PositionUpdate =
        onProgress(currentTrackId = TRACK, trackId = TRACK, positionMs = positionMs, durationMs = durationMs, progress = progress)

    private fun PlaybackPositionModel.seeked(
        positionMs: Long,
        progress: Double,
    ): PositionUpdate =
        onSeeked(currentTrackId = TRACK, trackId = TRACK, positionMs = positionMs, durationMs = 100_000, progress = progress)

    private fun assertPosition(
        model: PlaybackPositionModel,
        progress: Double,
        positionMs: Long,
        durationMs: Long?,
        hasCurrentTrack: Boolean = true,
    ) {
        val position = model.position(hasCurrentTrack)
        assertEquals(progress, position.progress, 0.0)
        assertEquals(if (hasCurrentTrack) positionMs else null, position.positionMs)
        assertEquals(if (hasCurrentTrack) durationMs else null, position.durationMs)
    }

    private companion object {
        const val TRACK = "track-1"
    }
}
