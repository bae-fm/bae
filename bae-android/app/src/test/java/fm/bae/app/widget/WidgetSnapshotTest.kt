package fm.bae.app.widget

import fm.bae.app.playback.BaeCorePlayer
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.NowPlaying
import fm.bae.app.playback.applyPlaybackState
import fm.bae.app.playback.pausedState
import fm.bae.app.playback.playbackValues
import fm.bae.app.playback.playingState
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
import uniffi.bae_bridge.BridgePlaybackValueState
import java.io.File

/**
 * The widget renders the last track from a file-backed snapshot and its play
 * state from the open library's player, so three things must hold: the map
 * from the player's now-playing projection to the snapshot, the write/read
 * round-trip through the store, and that a saved track shows as playing only
 * while a live player is playing it.
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
            )
        assertEquals("Track Title", snapshot.track?.title)
        assertEquals("Artist Name", snapshot.track?.artist)
        assertEquals(testCoverRef("rel-1"), snapshot.track?.coverImage)
    }

    @Test
    fun mapsNullNowPlayingToEmptyState() {
        assertNull(WidgetSnapshot.from(nowPlaying = null).track)
    }

    @Test
    fun aPlayingPlayerShowsItsTrackAsPlaying() {
        val player = playingPlayer()

        val content = widgetContent(WidgetSnapshot.from(player.nowPlaying.value), player)

        assertEquals("Track Title", content.track?.title)
        assertEquals("Artist Name", content.track?.artist)
        assertEquals(testCoverRef("rel-1"), content.track?.coverImage)
        assertTrue(content.isPlaying)
    }

    @Test
    fun aSavedTrackWithNoOpenLibraryShowsAsStopped() {
        val saved = WidgetSnapshot.from(playingPlayer().nowPlaying.value)

        val content = widgetContent(saved, livePlayer = null)

        assertEquals("Track Title", content.track?.title)
        assertFalse(content.isPlaying)
    }

    @Test
    fun aPausedPlayerShowsItsTrackAsStopped() {
        val player = playingPlayer()
        player.applyPlaybackState(pausedState(trackId = "t1"))

        assertFalse(widgetContent(WidgetSnapshot.from(player.nowPlaying.value), player).isPlaying)
    }

    @Test
    fun playerStoppedEventMapsToEmptyState() {
        val player = player()
        player.applyValues(playbackValues(BridgePlaybackValueState.Stopped))

        val content = widgetContent(WidgetSnapshot.from(player.nowPlaying.value), player)

        assertNull(content.track)
        assertFalse(content.isPlaying)
    }

    @Test
    fun roundTripsSnapshotThroughStore() =
        withStore { store ->
            val snapshot =
                WidgetSnapshot(
                    track = WidgetTrack(title = "Track Title", artist = "Artist Name", coverImage = testCoverRef("rel-1")),
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

    private fun playingPlayer(): BaeCorePlayer =
        player().apply {
            applyPlaybackState(
                playingState(
                    trackId = "t1",
                    title = "Track Title",
                    artistNames = "Artist Name",
                    albumId = "album-1",
                    albumTitle = "Album Title",
                    coverImage = testCoverRef("rel-1"),
                    durationMs = 185_000uL,
                ),
            )
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
