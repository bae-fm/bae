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
 * round-trip through the store, which reads only the current format, and that
 * a saved track shows as playing only while a live player is playing it.
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

        val content = widgetContent(saved(WidgetSnapshot.from(player.nowPlaying.value)), player)

        assertEquals(
            WidgetContent.Track(
                WidgetTrack(title = "Track Title", artist = "Artist Name", coverImage = testCoverRef("rel-1")),
                isPlaying = true,
            ),
            content,
        )
    }

    @Test
    fun aSavedTrackWithNoOpenLibraryShowsAsStopped() {
        val snapshot = WidgetSnapshot.from(playingPlayer().nowPlaying.value)

        val content = widgetContent(saved(snapshot), livePlayer = null) as WidgetContent.Track

        assertEquals("Track Title", content.track.title)
        assertFalse(content.isPlaying)
    }

    @Test
    fun aPausedPlayerShowsItsTrackAsStopped() {
        val player = playingPlayer()
        player.applyPlaybackState(pausedState(trackId = "t1"))

        val content = widgetContent(saved(WidgetSnapshot.from(player.nowPlaying.value)), player) as WidgetContent.Track
        assertFalse(content.isPlaying)
    }

    @Test
    fun playerStoppedEventMapsToNothingPlaying() {
        val player = player()
        player.applyValues(playbackValues(BridgePlaybackValueState.Stopped))

        assertEquals(WidgetContent.NothingPlaying, widgetContent(saved(WidgetSnapshot.from(player.nowPlaying.value)), player))
    }

    @Test
    fun aDamagedFileShowsSomethingWentWrong() {
        assertEquals(
            WidgetContent.SomethingWentWrong,
            widgetContent(WidgetRead.Damaged(IllegalArgumentException("bad")), livePlayer = null),
        )
    }

    @Test
    fun roundTripsSnapshotThroughStore() =
        withStore { store, _ ->
            val snapshot =
                WidgetSnapshot(
                    track = WidgetTrack(title = "Track Title", artist = "Artist Name", coverImage = testCoverRef("rel-1")),
                )
            store.write(snapshot)
            assertEquals(saved(snapshot), store.read())
        }

    @Test
    fun roundTripsSnapshotWithoutCover() =
        withStore { store, _ ->
            val snapshot =
                WidgetSnapshot(
                    track = WidgetTrack(title = "Track Title", artist = "Artist Name", coverImage = null),
                )
            store.write(snapshot)
            assertEquals(saved(snapshot), store.read())
        }

    @Test
    fun roundTripsEmptySnapshotThroughStore() =
        withStore { store, _ ->
            store.write(WidgetSnapshot.EMPTY)
            assertEquals(saved(WidgetSnapshot.EMPTY), store.read())
        }

    @Test
    fun readsNothingSavedWhenNoFileWritten() =
        withStore { store, _ ->
            assertEquals(WidgetRead.NothingSaved, store.read())
        }

    /** A file an older version wrote — no format, and the play state it used to carry — is deleted, not read. */
    @Test
    fun aFileFromAnOlderVersionIsDeletedAndReadsAsNothingSaved() =
        withStore { store, file ->
            file.writeText("""{"isPlaying":true,"track":{"title":"Track Title","artist":"Artist Name"}}""")

            assertEquals(WidgetRead.NothingSaved, store.read())
            assertFalse(file.exists())
        }

    /** The app writes the current format in one step, so one that doesn't parse is damaged — a bug, not an old file. */
    @Test
    fun aDamagedCurrentFormatFileIsDamaged() =
        withStore { store, file ->
            file.writeText(
                """{"format":2,"track":{"title":"Track Title","artist":"Artist Name","cover":{"type":"NOT_A_KIND","id":"rel-1","version":"1"}}}""",
            )

            assertTrue(store.read() is WidgetRead.Damaged)
            assertTrue("a damaged file is left for the next write, not removed", file.exists())
        }

    private fun saved(snapshot: WidgetSnapshot) = WidgetRead.Saved(snapshot)

    private fun withStore(body: suspend (WidgetSnapshotStore, File) -> Unit) {
        val file = File.createTempFile("widget-snapshot", ".json").also { it.delete() }
        try {
            runBlocking { body(WidgetSnapshotStore(file), file) }
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
