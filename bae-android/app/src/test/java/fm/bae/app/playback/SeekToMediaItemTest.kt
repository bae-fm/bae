package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Player
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
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.UiEventCallback

/**
 * Skip-to-queue-item ([BaeCorePlayer.handleSeek]'s COMMAND_SEEK_TO_MEDIA_ITEM
 * branch). The Media3 playlist is the now-playing track followed by the up-next
 * entries, so a tapped playlist index resolves to that entry's per-instance id
 * and core is told to skip to it. The now-playing slot has no queue-entry id, so
 * seeking to it is a no-op.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SeekToMediaItemTest {
    private fun entry(id: String) =
        BridgeQueueEntry(
            entryId = "entry-$id",
            trackId = "track-$id",
            title = "Title $id",
            artistNames = "Artist Name",
            durationMs = 180_000L,
            albumTitle = "Album Title",
            coverImageId = null,
        )

    @Test
    fun seekingToAQueueIndexSkipsToThatEntry() {
        val (player, handle) = player()
        playing(player, manual = listOf(entry("a"), entry("b")))

        // Playlist is [cur, track-a, track-b]; index 2 is the second up-next entry.
        (player as Player).seekTo(2, 0L)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals("entry-b", handle.skippedEntryIds.single())
    }

    @Test
    fun seekingAcrossTheManualAndContextLanesResolvesTheFlatIndex() {
        val (player, handle) = player()
        playing(
            player,
            manual = listOf(entry("m")),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.RELEASE,
                    shuffled = false,
                    upcoming = listOf(entry("c")),
                    upcomingTotal = 1uL,
                ),
        )

        // Playlist is [cur, track-m, track-c]; index 2 is the context entry.
        (player as Player).seekTo(2, 0L)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals("entry-c", handle.skippedEntryIds.single())
    }

    @Test
    fun seekingToTheNowPlayingSlotSkipsNothing() {
        val (player, handle) = player()
        playing(player, manual = listOf(entry("a")))

        // Index 0 is the now-playing track, which is not a queue entry.
        (player as Player).seekTo(0, 0L)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(emptyList<String>(), handle.skippedEntryIds)
    }

    private fun playing(
        player: BaeCorePlayer,
        manual: List<BridgeQueueEntry>,
        context: BridgePlaybackContext? = null,
    ) {
        player.onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                trackId = "cur",
                trackTitle = "Title cur",
                artistNames = "Artist Name",
                artistId = "artist-1",
                albumId = "album-1",
                albumTitle = "Album Title",
                coverImageId = null,
                durationMs = 180_000uL,
            ),
        )
        player.onQueueUpdated(manual = manual, context = context, hasNext = true, hasPrevious = false, revision = 1uL)
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun player(): Pair<BaeCorePlayer, SkipRecordingHandle> {
        val handle = SkipRecordingHandle()
        val player =
            BaeCorePlayer(
                applicationLooper = Looper.getMainLooper(),
                appHandle = handle,
                context = RuntimeEnvironment.getApplication(),
                scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
                isAppForeground = { false },
            )
        return player to handle
    }

    private class SkipRecordingHandle : AppHandle(NoHandle) {
        val skippedEntryIds = mutableListOf<String>()

        override fun skipToEntry(entryId: String) {
            skippedEntryIds += entryId
        }

        override suspend fun savePlaybackState() {}

        override fun subscribeUiEvents(callback: UiEventCallback) {}

        override suspend fun fetchCoverImageBytes(releaseId: String): ByteArray? = null
    }
}
