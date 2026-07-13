package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Player
import fm.bae.app.AppSessionHolder
import fm.bae.app.BridgeFixtures
import fm.bae.app.OpenLibrary
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LibraryStore
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeUiEvent
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.UiEventCallback

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackServiceTest {
    @After
    fun tearDown() {
        setCurrentSession(null)
    }

    @Test
    fun onCreateAddsMediaSessionForPlaybackPlayer() {
        val context = RuntimeEnvironment.getApplication()
        val looper = Looper.getMainLooper()
        val session = openLibrary(context, looper)
        setCurrentSession(session)

        val controller = Robolectric.buildService(PlaybackService::class.java).create()
        val service = controller.get()

        assertEquals(1, service.sessions.size)
        assertEquals(session.playback, service.sessions.single().player)

        controller.destroy()
    }

    @Test
    fun destroyingTheServiceDoesNotReleaseThePlayer() {
        val context = RuntimeEnvironment.getApplication()
        val looper = Looper.getMainLooper()
        val session = openLibrary(context, looper)
        setCurrentSession(session)

        Robolectric.buildService(PlaybackService::class.java).create().destroy()

        // The player is owned by OpenLibrary and must outlive the service: the
        // service can stop and restart (a fresh session over the same player) or
        // be killed by the system. Had onDestroy released the player, projecting a
        // new event would throw "Player is released".
        session.playback.onPlaying(
            BridgeUiEvent.PlaybackPlaying(
                "t1",
                "Track Title",
                "Artist Name",
                "artist-1",
                "album-1",
                "Album Title",
                null,
                200_000uL,
            ),
        )
        shadowOf(looper).idle()
        assertEquals(Player.STATE_READY, session.playback.playbackState)
    }

    @Test
    fun wireUpDoesNotStartTheServiceBeforePlayback() {
        val context = RuntimeEnvironment.getApplication()
        val session = openLibrary(context, Looper.getMainLooper())

        session.wireUp(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        // The service is started when playback begins on screen, not at library
        // open — a non-foreground service started here is reclaimed before the
        // first track plays.
        assertNull(shadowOf(context).nextStartedService)
    }

    private fun openLibrary(
        context: android.content.Context,
        looper: Looper,
    ): OpenLibrary {
        val handle = FakeAppHandle()
        return OpenLibrary(
            libraryId = "lib-1",
            appHandle = handle,
            diagnostics = BridgeDiagnostics(NoHandle),
            stores =
                OpenLibraryStores(
                    library = LibraryStore(),
                    config = ConfigStore(BridgeFixtures.config(), initialSyncReady = false),
                    downloads = DownloadStore(BridgeFixtures.downloadSnapshot()),
                    outbox = OutboxStore(BridgeFixtures.outboxSnapshot()),
                ),
            playback =
                BaeCorePlayer(
                    applicationLooper = looper,
                    appHandle = handle,
                    context = context,
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
                    isAppForeground = { false },
                ),
            appContext = context,
        )
    }

    private fun setCurrentSession(session: OpenLibrary?) {
        val field = AppSessionHolder::class.java.getDeclaredField("current")
        field.isAccessible = true
        field.set(AppSessionHolder, session)
    }
}

internal class FakeAppHandle(
    private val imageBytes: Map<String, ByteArray> = emptyMap(),
    private val albumCount: ULong = 0uL,
    private val composerCount: ULong = 0uL,
    private val albumPages: (offset: ULong, limit: ULong) -> List<BridgeAlbum> = { _, _ -> emptyList() },
    private val composerPages: (offset: ULong, limit: ULong) -> List<BridgeComposerSummary> = { _, _ -> emptyList() },
    private val albumDetails: Map<String, BridgeAlbumDetail> = emptyMap(),
    private val composerDetails: Map<String, BridgeComposerDetail> = emptyMap(),
    private val workDetails: Map<String, BridgeWorkDetail> = emptyMap(),
    private val releaseDetails: Map<String, BridgeRelease> = emptyMap(),
    private val searchResults: (query: String) -> BridgeSearchResults = { BridgeFixtures.searchResults() },
) : AppHandle(NoHandle) {
    var pauseCount = 0
    var resumeCount = 0

    /** Offset/limit each `getAlbumPage` was called with — lets browse-paging
     *  tests assert the requested window reached the bridge unaltered. */
    val albumPageWindows = mutableListOf<Pair<ULong, ULong>>()
    val composerPageWindows = mutableListOf<Pair<ULong, ULong>>()
    val playReleaseCalls = mutableListOf<Triple<String, UInt?, Boolean>>()

    override fun pause() {
        pauseCount++
    }

    override fun resume() {
        resumeCount++
    }

    override suspend fun savePlaybackState() {}

    override fun subscribeUiEvents(callback: UiEventCallback) {}

    override fun triggerSync() {}

    override suspend fun fetchCoverImageBytes(releaseId: String): ByteArray? = imageBytes[releaseId]

    override suspend fun getAlbumCount(): ULong = albumCount

    override suspend fun getComposerCount(): ULong = composerCount

    override suspend fun getAlbumPage(
        sortCriteria: List<BridgeSortCriterion>,
        offset: ULong,
        limit: ULong,
    ): List<BridgeAlbum> {
        albumPageWindows.add(offset to limit)
        return albumPages(offset, limit)
    }

    override suspend fun getComposerPage(
        sortCriteria: List<BridgeComposerSortCriterion>,
        offset: ULong,
        limit: ULong,
    ): List<BridgeComposerSummary> {
        composerPageWindows.add(offset to limit)
        return composerPages(offset, limit)
    }

    override suspend fun getAlbumDetail(albumId: String): BridgeAlbumDetail =
        checkNotNull(albumDetails[albumId]) { "no album detail fixture for $albumId" }

    override suspend fun getComposerDetail(artistId: String): BridgeComposerDetail? = composerDetails[artistId]

    override suspend fun getWorkDetail(workId: String): BridgeWorkDetail? = workDetails[workId]

    override suspend fun findReleaseDetail(releaseId: String): BridgeRelease? = releaseDetails[releaseId]

    override suspend fun searchLibrary(query: String): BridgeSearchResults = searchResults(query)

    override fun playRelease(
        releaseId: String,
        startTrackIndex: UInt?,
        shuffle: Boolean,
    ) {
        playReleaseCalls.add(Triple(releaseId, startTrackIndex, shuffle))
    }
}
