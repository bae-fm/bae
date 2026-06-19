package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.playback.PlaybackEventSink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.bae_bridge.BridgeLoadingTrackInfo
import uniffi.bae_bridge.BridgeQueueItem
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeUiEvent

class UiEventReducerTest {
    private fun stores(): Pair<LibraryStore, ConfigStore> =
        LibraryStore() to ConfigStore(BridgeFixtures.config(), initialSyncReady = false)

    // These tests exercise the library-routing arm; the playback sink is a no-op
    // (a real BaeCorePlayer needs a live AppHandle, which unit tests don't have —
    // that's why reduce() takes the PlaybackEventSink abstraction).
    private val noopPlayer = object : PlaybackEventSink {
        override fun onLoading(trackId: String, track: BridgeLoadingTrackInfo?) {}
        override fun onPlaying(
            trackId: String,
            trackTitle: String,
            artistNames: String,
            albumTitle: String,
            coverImageId: String?,
            durationMs: Long,
        ) {}
        override fun onPaused(
            trackId: String,
            trackTitle: String,
            artistNames: String,
            albumTitle: String,
            coverImageId: String?,
            durationMs: Long,
        ) {}
        override fun onStopped() {}
        override fun onProgress(
            positionMs: Long,
            durationMs: Long,
            progress: Double,
        ) {}
        override fun onRepeatModeChanged(mode: BridgeRepeatMode) {}
        override fun onQueueUpdated(items: List<BridgeQueueItem>, hasNext: Boolean, hasPrevious: Boolean) {}
        override fun onVolumeChanged(volume: Float) {}
        override fun onMuteChanged(isMuted: Boolean) {}
    }

    @Test
    fun albumAddedRoutesIntoLibraryStoreAndBumpsGeneration() {
        val (library, config) = stores()
        val detail = BridgeFixtures.albumDetail(BridgeFixtures.album(id = "alb-1"))
        val generationBefore = library.generation.value

        UiEventReducer.reduce(BridgeUiEvent.AlbumAdded(detail), library, config, noopPlayer)

        assertNotNull("album should be interned", library.albumDetail("alb-1"))
        assertEquals(detail, library.albumDetail("alb-1"))
        assertEquals(generationBefore + 1, library.generation.value)
    }

    @Test
    fun releaseUpdatedForAbsentAlbumIsDroppedWithoutThrowing() {
        val (library, config) = stores()
        val release = BridgeFixtures.release(id = "rel-x", albumId = "alb-absent")

        // No album was ever added, so this targets an un-interned album. The
        // handler logs the skip and leaves the store untouched — it must not throw.
        UiEventReducer.reduce(
            BridgeUiEvent.ReleaseUpdated(albumId = "alb-absent", release = release),
            library,
            config,
            noopPlayer,
        )

        assertNull(library.albumDetail("alb-absent"))
        assertEquals(emptyMap<String, Any>(), library.albumDetails.value)
    }

    @Test
    fun syncingChangedRoutesIntoConfigStore() {
        val (library, config) = stores()
        assertEquals(false, config.syncing.value)

        // config.syncing is the flow the library screen observes to show or hide
        // its sync indicator, so the reducer must mirror the SyncingChanged
        // signal onto it.
        UiEventReducer.reduce(BridgeUiEvent.SyncingChanged(syncing = true), library, config, noopPlayer)
        assertEquals(true, config.syncing.value)

        UiEventReducer.reduce(BridgeUiEvent.SyncingChanged(syncing = false), library, config, noopPlayer)
        assertEquals(false, config.syncing.value)
    }

    @Test
    fun playbackLoadingForwardsResolvedTrackMetadataToThePlayer() {
        val (library, config) = stores()
        var received: BridgeLoadingTrackInfo? = null
        val sink = object : PlaybackEventSink by noopPlayer {
            override fun onLoading(trackId: String, track: BridgeLoadingTrackInfo?) {
                received = track
            }
        }
        val info = BridgeLoadingTrackInfo(
            trackTitle = "Track Title",
            artistNames = "Artist Name",
            albumId = "alb-1",
            albumTitle = "Album Title",
            coverImageId = null,
            durationMs = 1uL,
        )

        // The player needs the resolved metadata to project a current track during
        // loading; the reducer must forward it rather than drop it.
        UiEventReducer.reduce(BridgeUiEvent.PlaybackLoading(trackId = "t1", track = info), library, config, sink)

        assertEquals("Track Title", received?.trackTitle)
    }
}
