package fm.bae.app.widget

import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.NowPlaying
import fm.bae.app.testCoverRef
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeUiEvent
import java.io.File

/**
 * The widget renders from a file-backed snapshot the launcher process reads, so
 * two things must hold: the map from the player's now-playing projection to the
 * snapshot, and the write/read round-trip through the store (a torn or absent
 * file must not crash the render — it falls back to the empty state).
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class WidgetSnapshotTest {
    @Test
    fun mapsNowPlayingIntoSnapshot() {
        val snapshot =
            WidgetSnapshot.from(
                nowPlaying =
                    NowPlaying(
                        trackId = "t1",
                        title = "Track Title",
                        artist = "Artist Name",
                        coverImage = testCoverRef("rel-1"),
                        sidePausePrompt = null,
                    ),
                isPlaying = true,
            )
        assertEquals("Track Title", snapshot.track?.title)
        assertEquals("Artist Name", snapshot.track?.artist)
        assertEquals(testCoverRef("rel-1"), snapshot.track?.coverImage)
        assertTrue(snapshot.isPlaying)
    }

    @Test
    fun mapsNullNowPlayingToEmptyState() {
        val snapshot = WidgetSnapshot.from(nowPlaying = null, isPlaying = false)
        assertNull(snapshot.track)
        assertFalse(snapshot.isPlaying)
    }

    @Test
    fun playerPlayingEventMapsToPlayingSnapshot() {
        val player = player()
        player.onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                trackId = "t1",
                trackTitle = "Track Title",
                artistNames = "Artist Name",
                artistId = "artist-1",
                albumId = "album-1",
                albumTitle = "Album Title",
                coverImage = testCoverRef("rel-1"),
                durationMs = 185_000uL,
            ),
        )

        val snapshot = WidgetSnapshot.from(player.nowPlaying.value, player.isPlaying.value)

        assertEquals("Track Title", snapshot.track?.title)
        assertEquals("Artist Name", snapshot.track?.artist)
        assertEquals(testCoverRef("rel-1"), snapshot.track?.coverImage)
        assertTrue(snapshot.isPlaying)
    }

    @Test
    fun playerStoppedEventMapsToEmptyState() {
        val player = player()
        player.onStopped()

        val snapshot = WidgetSnapshot.from(player.nowPlaying.value, player.isPlaying.value)

        assertNull(snapshot.track)
        assertFalse(snapshot.isPlaying)
    }

    @Test
    fun roundTripsPlayingSnapshotThroughStore() =
        withStore { store ->
            val snapshot =
                WidgetSnapshot(
                    track = WidgetTrack(title = "Track Title", artist = "Artist Name", coverImage = testCoverRef("rel-1")),
                    isPlaying = true,
                )
            store.write(snapshot)
            assertEquals(snapshot, store.read())
        }

    @Test
    fun roundTripsSnapshotWithoutCover() =
        withStore { store ->
            val snapshot =
                WidgetSnapshot(
                    track = WidgetTrack(title = "Track Title", artist = "Artist Name", coverImage = null),
                    isPlaying = false,
                )
            store.write(snapshot)
            assertEquals(snapshot, store.read())
        }

    @Test
    fun roundTripsEmptySnapshotThroughStore() =
        withStore { store ->
            store.write(WidgetSnapshot.EMPTY)
            assertEquals(WidgetSnapshot.EMPTY, store.read())
        }

    @Test
    fun readsEmptyStateWhenNoFileWritten() =
        withStore { store ->
            assertEquals(WidgetSnapshot.EMPTY, store.read())
        }

    private fun withStore(body: suspend (WidgetSnapshotStore) -> Unit) {
        val file = File.createTempFile("widget-snapshot", ".json").also { it.delete() }
        try {
            runBlocking { body(WidgetSnapshotStore(file)) }
        } finally {
            file.delete()
        }
    }

    private fun player(): BaeCorePlayer {
        val context = RuntimeEnvironment.getApplication()
        return BaeCorePlayer(
            applicationLooper = android.os.Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            context = context,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )
    }
}
