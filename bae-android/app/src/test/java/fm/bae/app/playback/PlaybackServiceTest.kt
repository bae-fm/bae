package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Player
import fm.bae.app.AppSessionHolder
import fm.bae.app.BridgeFixtures
import fm.bae.app.OpenLibrary
import fm.bae.app.data.ArtworkLoadingStore
import fm.bae.app.data.CastStore
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LibraryStore
import fm.bae.app.data.LibraryTransferStores
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.OutboxStore
import fm.bae.app.data.SyncStatusStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
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
import uniffi.bae_bridge.AlbumBrowseSubscription
import uniffi.bae_bridge.AlbumDetailCallback
import uniffi.bae_bridge.AlbumPageCallback
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumBrowseSnapshot
import uniffi.bae_bridge.BridgeAlbumBrowseWindow
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumPage
import uniffi.bae_bridge.BridgeComposerBrowseSnapshot
import uniffi.bae_bridge.BridgeComposerBrowseWindow
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerPage
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeDiagnostics
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeImageRef
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeLiveQueryCause
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.CastDevicesCallback
import uniffi.bae_bridge.ComposerBrowseSubscription
import uniffi.bae_bridge.ComposerDetailCallback
import uniffi.bae_bridge.ComposerPageCallback
import uniffi.bae_bridge.ConfigCallback
import uniffi.bae_bridge.DownloadCallback
import uniffi.bae_bridge.EagerCacheFillStatusCallback
import uniffi.bae_bridge.LibrarySearchCallback
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.OutboxCallback
import uniffi.bae_bridge.PlaybackValuesCallback
import uniffi.bae_bridge.QueueCallback
import uniffi.bae_bridge.ReleaseDetailCallback
import uniffi.bae_bridge.SyncStatusCallback
import uniffi.bae_bridge.UiEventCallback
import uniffi.bae_bridge.WorkDetailCallback

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
        session.playback.applyPlaybackState(
            playingState(
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
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        return OpenLibrary(
            libraryId = "lib-1",
            appHandle = handle,
            diagnostics = BridgeDiagnostics(NoHandle),
            stores =
                OpenLibraryStores(
                    library = LibraryStore(),
                    config = ConfigStore(BridgeFixtures.config()),
                    syncStatus = SyncStatusStore(),
                    transfers =
                        LibraryTransferStores(
                            artworkLoading = ArtworkLoadingStore {},
                            downloads = DownloadStore(BridgeFixtures.downloadSnapshot()),
                            outbox = OutboxStore(BridgeFixtures.outboxSnapshot()),
                        ),
                    cast = CastStore(),
                ),
            runtime =
                fm.bae.app.OpenLibraryRuntime(
                    playback =
                        BaeCorePlayer(
                            applicationLooper = looper,
                            appHandle = handle,
                            context = context,
                            scope = scope,
                            isAppForeground = { false },
                        ),
                    scope = scope,
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
    private val albumPages: (offset: ULong, limit: ULong) -> List<BridgeAlbum> = { _, _ -> emptyList() },
    private val composerPages: (offset: ULong, limit: ULong) -> List<BridgeComposerSummary> = { _, _ -> emptyList() },
    private val albumDetails: Map<String, BridgeAlbumDetail> = emptyMap(),
    private val composerDetails: Map<String, BridgeComposerDetail> = emptyMap(),
    private val workDetails: Map<String, BridgeWorkDetail> = emptyMap(),
    private val releaseDetails: Map<String, BridgeRelease> = emptyMap(),
    private val searchResults: (query: String) -> BridgeSearchResults = { BridgeFixtures.searchResults() },
    private val initialAlbumPageError: uniffi.bae_bridge.BridgeException? = null,
    private val initialSearchError: (query: String) -> uniffi.bae_bridge.BridgeException? = { null },
    var deliverAlbumPagesImmediately: Boolean = true,
    var deliverAlbumBaselineImmediately: Boolean = true,
    var deliverSearchResultsImmediately: Boolean = true,
) : AppHandle(NoHandle) {
    var pauseCount = 0
    var resumeCount = 0

    /** Offset/limit each `getAlbumPage` was called with — lets browse-paging
     *  tests assert the requested window reached the bridge unaltered. */
    val albumPageWindows = mutableListOf<Pair<ULong, ULong>>()
    val composerPageWindows = mutableListOf<Pair<ULong, ULong>>()
    val playReleaseCalls = mutableListOf<Triple<String, UInt?, Boolean>>()
    val liveSubscriptions = mutableListOf<FakeLiveSubscription>()
    private val uiAlbumPageCallbacks = mutableListOf<AlbumPageCallback>()
    private val uiAlbumPageSubscriptions = mutableListOf<FakeLiveSubscription>()
    val searchCallbacks = mutableListOf<LibrarySearchCallback>()
    val albumBrowseSubscriptions = mutableListOf<FakeAlbumBrowseSubscription>()
    val composerBrowseSubscriptions = mutableListOf<FakeComposerBrowseSubscription>()
    val albumDetailSubscriptions = mutableListOf<FakeLiveSubscription>()
    val searchSubscriptions = mutableListOf<FakeLiveSubscription>()

    private fun liveSubscription(): FakeLiveSubscription = FakeLiveSubscription().also(liveSubscriptions::add)

    override fun pause() {
        pauseCount++
    }

    override fun resume() {
        resumeCount++
    }

    override suspend fun savePlaybackState() {}

    override fun subscribeUiEvents(callback: UiEventCallback) {}

    override fun subscribeConfig(callback: ConfigCallback): LiveSubscription = liveSubscription()

    override fun subscribeSyncStatus(callback: SyncStatusCallback): LiveSubscription = liveSubscription()

    override fun subscribeEagerCacheFillStatus(callback: EagerCacheFillStatusCallback): LiveSubscription = liveSubscription()

    override fun subscribeDownloads(callback: DownloadCallback): LiveSubscription = liveSubscription()

    override fun subscribeOutbox(callback: OutboxCallback): LiveSubscription = liveSubscription()

    override fun subscribeCastDevices(callback: CastDevicesCallback): LiveSubscription = liveSubscription()

    override fun subscribeQueue(callback: QueueCallback): LiveSubscription = liveSubscription()

    override fun subscribePlaybackValues(callback: PlaybackValuesCallback): LiveSubscription = liveSubscription()

    override fun triggerSync() {}

    override suspend fun fetchLibraryImageBytes(image: BridgeImageRef): ByteArray? = imageBytes[image.id]

    override fun subscribeAlbumPage(
        sortCriteria: List<BridgeSortCriterion>,
        offset: ULong,
        limit: ULong,
        callback: AlbumPageCallback,
    ): LiveSubscription {
        albumPageWindows.add(offset to limit)
        uiAlbumPageCallbacks += callback
        val subscription = liveSubscription().also(uiAlbumPageSubscriptions::add)
        if (!deliverAlbumPagesImmediately) {
            return subscription
        }
        if (initialAlbumPageError == null) {
            val rows = albumPages(offset, limit)
            callback.onValue(BridgeAlbumPage(rows, rows.size.toULong()))
        } else {
            callback.onError(initialAlbumPageError)
        }
        return subscription
    }

    fun emitSearchResults(
        subscription: Int,
        value: BridgeSearchResults,
    ) {
        searchCallbacks[subscription].onValue(value)
    }

    override fun subscribeComposerPage(
        sortCriteria: List<BridgeComposerSortCriterion>,
        offset: ULong,
        limit: ULong,
        callback: ComposerPageCallback,
    ): LiveSubscription {
        composerPageWindows.add(offset to limit)
        val rows = composerPages(offset, limit)
        callback.onValue(BridgeComposerPage(rows, rows.size.toULong()))
        return liveSubscription()
    }

    override fun subscribeAlbumBrowse(sortCriteria: List<BridgeSortCriterion>): AlbumBrowseSubscription =
        FakeAlbumBrowseSubscription(
            albumPages,
            albumPageWindows,
            deliverAlbumPagesImmediately,
            deliverAlbumBaselineImmediately,
            initialAlbumPageError,
        ).also(albumBrowseSubscriptions::add)

    override fun subscribeComposerBrowse(sortCriteria: List<BridgeComposerSortCriterion>): ComposerBrowseSubscription =
        FakeComposerBrowseSubscription(composerPages).also(composerBrowseSubscriptions::add)

    override fun subscribeAlbumDetail(
        albumId: String,
        callback: AlbumDetailCallback,
    ): LiveSubscription {
        callback.onValue(albumDetails[albumId])
        return liveSubscription().also(albumDetailSubscriptions::add)
    }

    override fun subscribeComposerDetail(
        artistId: String,
        callback: ComposerDetailCallback,
    ): LiveSubscription {
        callback.onValue(composerDetails[artistId])
        return liveSubscription()
    }

    override fun subscribeWorkDetail(
        workId: String,
        callback: WorkDetailCallback,
    ): LiveSubscription {
        callback.onValue(workDetails[workId])
        return liveSubscription()
    }

    override fun subscribeReleaseDetail(
        releaseId: String,
        callback: ReleaseDetailCallback,
    ): LiveSubscription {
        callback.onValue(releaseDetails[releaseId])
        return liveSubscription()
    }

    override fun subscribeLibrarySearch(
        query: String,
        callback: LibrarySearchCallback,
    ): LiveSubscription {
        searchCallbacks += callback
        val subscription = liveSubscription().also(searchSubscriptions::add)
        if (!deliverSearchResultsImmediately) {
            return subscription
        }
        val error = initialSearchError(query)
        if (error == null) {
            callback.onValue(searchResults(query))
        } else {
            callback.onError(error)
        }
        return subscription
    }

    fun failSearchResults(
        subscription: Int,
        error: uniffi.bae_bridge.BridgeException,
    ) {
        searchCallbacks[subscription].onError(error)
    }

    override fun playRelease(
        releaseId: String,
        startTrackIndex: UInt?,
        shuffle: Boolean,
    ) {
        playReleaseCalls.add(Triple(releaseId, startTrackIndex, shuffle))
    }
}

internal class FakeLiveSubscription : LiveSubscription(NoHandle) {
    var cancelled = false

    override fun cancel() {
        cancelled = true
    }
}

internal class FakeAlbumBrowseSubscription(
    private val rows: (ULong, ULong) -> List<BridgeAlbum>,
    private val observedWindows: MutableList<Pair<ULong, ULong>>,
    private val deliverWindowsImmediately: Boolean,
    deliverBaselineImmediately: Boolean,
    private val initialError: BridgeException?,
) : AlbumBrowseSubscription(NoHandle) {
    private val events = Channel<Result<BridgeAlbumBrowseSnapshot>>(Channel.UNLIMITED)
    private var windows = emptyList<BridgeLibraryPageWindow>()
    private var revision = 0uL
    var cancelled = false
    val requestedWindows: List<BridgeLibraryPageWindow>
        get() = windows

    init {
        if (deliverBaselineImmediately) emitSnapshot(0uL, BridgeLiveQueryCause.INITIAL)
    }

    override fun setWindows(windows: List<BridgeLibraryPageWindow>) {
        if (windows == this.windows) return
        this.windows = windows
        revision++
        observedWindows += windows.map { it.offset to it.limit }
        if (deliverWindowsImmediately) {
            if (initialError == null) {
                emitSnapshot(cause = BridgeLiveQueryCause.REQUEST_CHANGED)
            } else {
                events.trySend(Result.failure(initialError))
            }
        }
    }

    override suspend fun next(): BridgeAlbumBrowseSnapshot = events.receive().getOrThrow()

    override suspend fun cancel() {
        cancelled = true
    }

    fun emitRows(
        rows: List<BridgeAlbum>,
        totalCount: ULong = rows.size.toULong(),
        cause: BridgeLiveQueryCause = BridgeLiveQueryCause.DATABASE_CHANGED,
    ) {
        val window = windows.lastOrNull() ?: BridgeLibraryPageWindow(0uL, rows.size.toULong().coerceAtLeast(1uL))
        events.trySend(
            Result.success(
                BridgeAlbumBrowseSnapshot(
                    windows = listOf(BridgeAlbumBrowseWindow(window, rows)),
                    totalCount = totalCount,
                    requestRevision = revision,
                    cause = cause,
                ),
            ),
        )
    }

    fun emitCount(totalCount: ULong) = emitSnapshot(totalCount, BridgeLiveQueryCause.DATABASE_CHANGED)

    private fun emitSnapshot(
        totalCount: ULong? = null,
        cause: BridgeLiveQueryCause,
    ) {
        val projected =
            windows.map { window ->
                val values = rows(window.offset, window.limit)
                BridgeAlbumBrowseWindow(window, values)
            }
        events.trySend(
            Result.success(
                BridgeAlbumBrowseSnapshot(
                    windows = projected,
                    totalCount = totalCount ?: projected.sumOf { it.rows.size }.toULong(),
                    requestRevision = revision,
                    cause = cause,
                ),
            ),
        )
    }
}

internal class FakeComposerBrowseSubscription(
    private val rows: (ULong, ULong) -> List<BridgeComposerSummary>,
) : ComposerBrowseSubscription(NoHandle) {
    private val events = Channel<BridgeComposerBrowseSnapshot>(Channel.UNLIMITED)
    private var windows = emptyList<BridgeLibraryPageWindow>()
    private var revision = 0uL
    var cancelled = false

    init {
        events.trySend(BridgeComposerBrowseSnapshot(emptyList(), 0uL, revision, BridgeLiveQueryCause.INITIAL))
    }

    override fun setWindows(windows: List<BridgeLibraryPageWindow>) {
        if (windows == this.windows) return
        this.windows = windows
        revision++
        val projected = windows.map { BridgeComposerBrowseWindow(it, rows(it.offset, it.limit)) }
        events.trySend(
            BridgeComposerBrowseSnapshot(
                projected,
                projected.sumOf { it.rows.size }.toULong(),
                revision,
                BridgeLiveQueryCause.REQUEST_CHANGED,
            ),
        )
    }

    override suspend fun next(): BridgeComposerBrowseSnapshot = events.receive()

    override suspend fun cancel() {
        cancelled = true
    }
}

private fun queryFailure(): BridgeException = BridgeException.Diagnostic(BridgeErrorCategory.Internal, "temporary")
