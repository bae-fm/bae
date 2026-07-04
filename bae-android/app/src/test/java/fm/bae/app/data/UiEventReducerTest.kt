package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.ErrorLines
import fm.bae.app.playback.PlaybackEventSink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLoadingTrackInfo
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeRepeatMode
import uniffi.bae_bridge.BridgeUiEvent

class UiEventReducerTest {
    private fun stores(): OpenLibraryStores =
        OpenLibraryStores(
            library = LibraryStore(),
            config = ConfigStore(BridgeFixtures.config(), initialSyncReady = false),
            downloads = DownloadStore(BridgeFixtures.downloadSnapshot()),
        )

    // These tests exercise the library-routing arm; the playback sink is a no-op
    // (a real BaeCorePlayer needs a live AppHandle, which unit tests don't have —
    // that's why reduce() takes the PlaybackEventSink abstraction).
    private val noopPlayer =
        object : PlaybackEventSink {
            override fun onLoading(
                trackId: String,
                track: BridgeLoadingTrackInfo?,
            ) {}

            override fun onPlaying(event: BridgeUiEvent.PlaybackPlaying) {}

            override fun onPaused(event: BridgeUiEvent.PlaybackPaused) {}

            override fun onStopped() {}

            override fun onProgress(
                trackId: String,
                positionMs: Long,
                durationMs: Long,
                progress: Double,
            ) {}

            override fun onSeeked(
                trackId: String,
                positionMs: Long,
                durationMs: Long,
                progress: Double,
            ) {}

            override fun onRepeatModeChanged(mode: BridgeRepeatMode) {}

            override fun onQueueUpdated(
                manual: List<BridgeQueueEntry>,
                context: BridgePlaybackContext?,
                hasNext: Boolean,
                hasPrevious: Boolean,
            ) {}

            override fun onVolumeChanged(volume: Float) {}

            override fun onMuteChanged(isMuted: Boolean) {}
        }

    // No-op error resolver: these tests exercise the library/sync/playback arms,
    // never the error events, so the localized line is never needed.
    private val noopErrors =
        object : ErrorLines {
            override fun line(reason: BridgePlaybackErrorReason) = ""

            override fun line(error: BridgeException) = ""
        }

    private fun reduce(
        event: BridgeUiEvent,
        stores: OpenLibraryStores,
        player: PlaybackEventSink = noopPlayer,
        errors: ErrorLines = noopErrors,
    ) {
        UiEventReducer.reduce(event, stores, player, errors)
    }

    @Test
    fun albumAddedRoutesIntoLibraryStoreAndBumpsGeneration() {
        val stores = stores()
        val library = stores.library
        val detail = BridgeFixtures.albumDetail(BridgeFixtures.album(id = "alb-1"))
        val generationBefore = library.generation.value
        val composerGenerationBefore = library.composerGeneration.value

        reduce(BridgeUiEvent.AlbumAdded(detail), stores)

        assertNotNull("album should be interned", library.albumDetail("alb-1"))
        assertEquals(detail, library.albumDetail("alb-1"))
        assertEquals(generationBefore + 1, library.generation.value)
        assertEquals(composerGenerationBefore + 1, library.composerGeneration.value)
    }

    @Test
    fun releaseChangesRefreshComposerPagesWithoutRebuildingAlbumPages() {
        val stores = stores()
        val library = stores.library
        val album = BridgeFixtures.album(id = "alb-1", releaseIds = listOf("rel-1", "rel-2"))
        val detail = BridgeFixtures.albumDetail(album, releases = listOf(BridgeFixtures.release(id = "rel-1", albumId = "alb-1")))
        reduce(BridgeUiEvent.AlbumAdded(detail), stores)
        val albumGeneration = library.generation.value
        val composerGeneration = library.composerGeneration.value

        reduce(
            BridgeUiEvent.ReleaseAdded(album = album, release = BridgeFixtures.release(id = "rel-2", albumId = "alb-1")),
            stores,
        )

        assertEquals(albumGeneration, library.generation.value)
        assertEquals(composerGeneration + 1, library.composerGeneration.value)
    }

    @Test
    fun releaseUpdatedForAbsentAlbumIsDroppedWithoutThrowing() {
        val stores = stores()
        val library = stores.library
        val release = BridgeFixtures.release(id = "rel-x", albumId = "alb-absent")

        // No album was ever added, so this targets an un-interned album. The
        // handler logs the skip and leaves the store untouched — it must not throw.
        reduce(
            BridgeUiEvent.ReleaseUpdated(albumId = "alb-absent", release = release),
            stores,
        )

        assertNull(library.albumDetail("alb-absent"))
        assertEquals(emptyMap<String, Any>(), library.albumDetails.value)
    }

    @Test
    fun syncingChangedRoutesIntoConfigStore() {
        val stores = stores()
        val config = stores.config
        assertEquals(false, config.syncing.value)

        // config.syncing is the flow the library screen observes to show or hide
        // its sync indicator, so the reducer must mirror the SyncingChanged
        // signal onto it.
        reduce(BridgeUiEvent.SyncingChanged(syncing = true), stores)
        assertEquals(true, config.syncing.value)

        reduce(BridgeUiEvent.SyncingChanged(syncing = false), stores)
        assertEquals(false, config.syncing.value)
    }

    @Test
    fun downloadQueueChangedRoutesIntoDownloadStore() {
        val stores = stores()
        val downloads = stores.downloads
        val snapshot = BridgeFixtures.downloadSnapshot(queued = 1u, active = 1u, paused = true)

        reduce(
            BridgeUiEvent.DownloadQueueChanged(snapshot),
            stores,
        )

        assertEquals(snapshot, downloads.snapshot.value)
    }

    @Test
    fun playbackLoadingForwardsResolvedTrackMetadataToThePlayer() {
        val stores = stores()
        var received: BridgeLoadingTrackInfo? = null
        val sink =
            object : PlaybackEventSink by noopPlayer {
                override fun onLoading(
                    trackId: String,
                    track: BridgeLoadingTrackInfo?,
                ) {
                    received = track
                }
            }
        val info =
            BridgeLoadingTrackInfo(
                trackTitle = "Track Title",
                artistNames = "Artist Name",
                albumId = "alb-1",
                albumTitle = "Album Title",
                coverImageId = null,
                durationMs = 1uL,
            )

        // The player needs the resolved metadata to project a current track during
        // loading; the reducer must forward it rather than drop it.
        reduce(
            BridgeUiEvent.PlaybackLoading(trackId = "t1", track = info),
            stores,
            sink,
        )

        assertEquals("Track Title", received?.trackTitle)
    }

    @Test
    fun playbackSeekedRoutesIntoThePlayer() {
        val stores = stores()
        var receivedTrackId: String? = null
        var receivedPositionMs: Long? = null
        val sink =
            object : PlaybackEventSink by noopPlayer {
                override fun onSeeked(
                    trackId: String,
                    positionMs: Long,
                    durationMs: Long,
                    progress: Double,
                ) {
                    receivedTrackId = trackId
                    receivedPositionMs = positionMs
                }
            }

        reduce(
            BridgeUiEvent.PlaybackSeeked(
                trackId = "t1",
                positionMs = 75_000uL,
                durationMs = 100_000uL,
                progress = 0.75,
            ),
            stores,
            sink,
        )

        assertEquals("t1", receivedTrackId)
        assertEquals(75_000L, receivedPositionMs)
    }
}
