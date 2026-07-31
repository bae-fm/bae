package fm.bae.app.playback

import androidx.media3.common.MediaMetadata
import androidx.media3.common.Timeline
import fm.bae.app.testCoverRef
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeUiEvent

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
            title = "Title $id",
            artist = "Artist Name",
            albumTitle = "Album Title",
            durationClock = null,
            coverImage = null,
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
        val coverBytes = byteArrayOf(1, 2, 3, 4)
        // Main.immediate runs the artwork fetch inline (the fake returns without
        // suspending), so the current item already carries its bytes here.
        val player = player(imageBytes = mapOf("cover-1" to coverBytes))
        player.onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                trackId = "cur",
                trackTitle = "Title cur",
                artistNames = "Artist Name",
                artistId = "artist-1",
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
        assertArrayEquals(coverBytes, metadata.artworkData)
        assertEquals("Title cur", sessionMetadata.title)
        assertEquals("Title cur", metadata.title)
        assertEquals("Artist Name", metadata.artist)
        assertEquals("Album Title", metadata.albumTitle)
        assertEquals(185_000L, metadata.durationMs)
        assertEquals(MediaMetadata.MEDIA_TYPE_MUSIC, metadata.mediaType)
        assertEquals(true, metadata.isPlayable)
        assertEquals(false, metadata.isBrowsable)
    }

    private fun player(imageBytes: Map<String, ByteArray>): BaeCorePlayer {
        val context = RuntimeEnvironment.getApplication()
        val looper = android.os.Looper.getMainLooper()
        val handle = FakeAppHandle(imageBytes)
        return BaeCorePlayer(
            applicationLooper = looper,
            appHandle = handle,
            context = context,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )
    }
}
