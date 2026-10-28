package fm.bae.app.playback

import androidx.media3.common.MediaMetadata
import androidx.media3.common.Timeline
import fm.bae.app.testCoverRef
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeDurationClock
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeTimelinePoint
import uniffi.bae_bridge.BridgeTrackDisplay

/**
 * The MediaSession playlist projection (BaeCorePlayer.orderedMetas). The
 * current track is held separately from core's up-next queue, so the projection
 * must reinsert it at the front — otherwise the lock screen / notification show
 * the wrong (first up-next) track.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class NowPlayingProjectionTest {
    private fun meta(id: String) =
        BaeCorePlayer.Meta(
            entryId = "entry-$id",
            trackId = id,
            display =
                BridgeTrackDisplay(
                    title = "Title $id",
                    artistNames = "Artist Name",
                    albumId = "album-1",
                    releaseId = "release-1",
                    albumTitle = "Album Title",
                    coverImage = null,
                ),
        )

    @Test
    fun prependsCurrentWhenQueueHasUpNextTracks() {
        val result =
            BaeCorePlayer.orderedMetas(
                entries = listOf(meta("up1"), meta("up2")),
                current = meta("cur"),
            )
        assertEquals(listOf("cur", "up1", "up2"), result.map { it.trackId })
    }

    @Test
    fun currentOnlyWhenQueueEmpty() {
        val result = BaeCorePlayer.orderedMetas(entries = emptyList(), current = meta("cur"))
        assertEquals(listOf("cur"), result.map { it.trackId })
    }

    @Test
    fun noDuplicateWhenCurrentAlreadyInQueue() {
        val result =
            BaeCorePlayer.orderedMetas(
                entries = listOf(meta("cur"), meta("up1")),
                current = meta("cur"),
            )
        assertEquals(listOf("cur", "up1"), result.map { it.trackId })
    }

    @Test
    fun emptyWhenNothingPlaying() {
        val result = BaeCorePlayer.orderedMetas(entries = emptyList(), current = null)
        assertEquals(emptyList<String>(), result.map { it.trackId })
    }

    @Test
    fun currentMediaItemCarriesLockScreenMetadata() {
        val player = player()
        player.applyPlaybackState(
            playingState(
                trackId = "cur",
                title = "Title cur",
                artistNames = "Artist Name",
                albumId = "album-1",
                albumTitle = "Album Title",
                coverImage = testCoverRef("cover-1"),
                durationMs = 185_000uL,
            ),
        )

        val window = player.currentTimeline.getWindow(0, Timeline.Window())
        val mediaItem = window.mediaItem
        val metadata = mediaItem.mediaMetadata
        val sessionMetadata = player.mediaMetadata

        assertEquals("cur", mediaItem.mediaId)
        assertEquals(185_000_000L, window.durationUs)
        assertEquals(ArtworkContentProvider.uriFor(RuntimeEnvironment.getApplication(), testCoverRef("cover-1")), metadata.artworkUri)
        assertNull(metadata.artworkData)
        assertEquals("Title cur", sessionMetadata.title)
        assertEquals("Title cur", metadata.title)
        assertEquals("Artist Name", metadata.artist)
        assertEquals("Album Title", metadata.albumTitle)
        assertEquals(185_000L, metadata.durationMs)
        assertEquals(MediaMetadata.MEDIA_TYPE_MUSIC, metadata.mediaType)
        assertEquals(true, metadata.isPlayable)
        assertEquals(false, metadata.isBrowsable)
    }

    @Test
    fun positionKeepsThePlaylistAndAQueueChangeReplacesIt() {
        val player = player()
        player.onQueueValue(manual = listOf(entry("up1")), context = null, hasNext = true, hasPrevious = false)
        player.applyPlaybackState(playingState(trackId = "cur", durationMs = 185_000uL))
        val playing = player.currentTimeline

        player.applyTimeline(timeline(line(trackId = "cur", positionMs = 1_000uL)))
        assertSame(playing, player.currentTimeline)

        player.onQueueValue(manual = listOf(entry("up2")), context = null, hasNext = true, hasPrevious = false)
        assertNotSame(playing, player.currentTimeline)
        assertEquals(
            "up2",
            player.currentTimeline
                .getWindow(1, Timeline.Window())
                .mediaItem.mediaId,
        )
    }

    @Test
    fun replacedCoverChangesTheArtworkUri() {
        val player = player()
        val first = testCoverRef("cover-1")
        val replaced = first.copy(version = "2")
        player.applyPlaybackState(playingState(trackId = "cur", coverImage = first))
        player.applyPlaybackState(pausedState(trackId = "cur", coverImage = replaced))

        assertEquals(
            ArtworkContentProvider.uriFor(RuntimeEnvironment.getApplication(), replaced),
            player.currentMediaItem?.mediaMetadata?.artworkUri,
        )
    }

    private fun entry(trackId: String) =
        BridgeQueueEntry(
            entryId = "entry-$trackId",
            trackId = trackId,
            display = meta(trackId).display,
            durationClock = BridgeDurationClock(negative = false, hours = null, minutes = 3u, seconds = 0u),
        )

    private fun line(
        trackId: String,
        positionMs: ULong,
    ) = BridgeTimelinePoint(trackId = trackId, positionMs = positionMs, durationMs = 185_000uL, moving = true)

    private fun player(): BaeCorePlayer {
        val context = RuntimeEnvironment.getApplication()
        val looper = android.os.Looper.getMainLooper()
        val handle = FakeAppHandle()
        return BaeCorePlayer(
            applicationLooper = looper,
            appHandle = handle,
            system = PlaybackSystem(context, AudioFocus(context), { false }),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        )
    }
}
