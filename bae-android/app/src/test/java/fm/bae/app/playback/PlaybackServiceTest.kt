package fm.bae.app.playback

import android.os.Looper
import androidx.media3.common.Player
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.BugReports
import fm.bae.app.NoErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.data.CastStore
import fm.bae.app.data.ConfigStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.OpenLibraryStores
import fm.bae.app.data.SessionMessages
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.plainCreditText
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AlbumDetailSubscription
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeAlbumDetailSnapshot
import uniffi.bae_bridge.BridgeComposerBrowseSnapshot
import uniffi.bae_bridge.BridgeComposerBrowseWindow
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerDetailRowsSnapshot
import uniffi.bae_bridge.BridgeComposerDetailRowsWindow
import uniffi.bae_bridge.BridgeComposerDetailSnapshot
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import uniffi.bae_bridge.BridgeLibrarySearchSnapshot
import uniffi.bae_bridge.BridgeLiveQueryCause
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseDetailSnapshot
import uniffi.bae_bridge.BridgeSearchResults
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.BridgeWorkDetailSnapshot
import uniffi.bae_bridge.ComposerBrowseSubscription
import uniffi.bae_bridge.ComposerDetailRowsSubscription
import uniffi.bae_bridge.ComposerDetailSubscription
import uniffi.bae_bridge.LibrarySearchSubscription
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.ReleaseDetailSubscription
import uniffi.bae_bridge.WorkDetailSubscription

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackServiceTest {
    @Test
    fun onCreateAddsMediaSessionForPlaybackPlayer() {
        val context = RuntimeEnvironment.getApplication()
        val looper = Looper.getMainLooper()
        val session = openLibrary(context, looper)

        val controller = playbackService(session).create()
        val service = controller.get()

        assertEquals(1, service.sessions.size)
        assertEquals(session.playback, service.sessions.single().player)

        controller.destroy()
    }

    @Test
    fun serviceTheSystemCreatesWithNoLibraryOpenStopsItself() {
        val service = Robolectric.buildService(PlaybackService::class.java).create().get()

        assertEquals(0, service.sessions.size)
        assertTrue(shadowOf(service).isStoppedBySelf)
    }

    @Test
    fun destroyingTheServiceDoesNotReleaseThePlayer() {
        val context = RuntimeEnvironment.getApplication()
        val looper = Looper.getMainLooper()
        val session = openLibrary(context, looper)

        playbackService(session).create().destroy()

        // The player is owned by OpenLibrary and must outlive the service: the
        // service can stop and restart (a fresh session over the same player) or
        // be killed by the system. Had onDestroy released the player, projecting a
        // new event would throw "Player is released".
        session.playback.applyPlaybackState(
            playingState(
                trackId = "t1",
                durationMs = 200_000uL,
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
            diagnostics = FakeDiagnostics(),
            stores =
                OpenLibraryStores(
                    config = ConfigStore(BridgeFixtures.config()),
                    syncStatus = SyncStatusStore(),
                    syncActivity = SyncActivityStore(),
                    downloads = DownloadStore(BridgeFixtures.downloadCounts(), initialUnmeteredOnly = false),
                    cast = CastStore(),
                ),
            runtime =
                fm.bae.app.OpenLibraryRuntime(
                    playback =
                        BaeCorePlayer(
                            applicationLooper = looper,
                            appHandle = handle,
                            system = PlaybackSystem(context, AudioFocus(context), { false }),
                            scope = scope,
                            creditLineText = ::plainCreditText,
                        ),
                    scope = scope,
                    messages = SessionMessages(scope, NoErrorDisplays),
                    bugs = BugReports(),
                    errors = NoErrorDisplays,
                ),
            appContext = context,
        )
    }

    /** The service over [session], driven through the lifecycle the system gives it. */
    private fun playbackService(session: OpenLibrary): ServiceController<PlaybackService> =
        ServiceController.of(PlaybackService { session }, null)
}

/**
 * A detail read that answers each id it is pointed at with what [lookup] finds for it, recording
 * every id and whether it was cancelled.
 */
internal class FakeDetailRead<Value>(
    private val lookup: (String) -> Value?,
) {
    private val events = Channel<Pair<String?, Value?>>(Channel.UNLIMITED)
    val requestedIds = mutableListOf<String>()
    var cancelled = false

    fun setId(id: String) {
        requestedIds += id
        events.trySend(id to lookup(id))
    }

    suspend fun next(): Pair<String?, Value?> = events.receive()

    fun cancel() {
        cancelled = true
        events.close(BridgeException.Cancelled())
    }
}

internal class FakeAlbumDetailSubscription(
    val read: FakeDetailRead<BridgeAlbumDetail>,
) : AlbumDetailSubscription(NoHandle) {
    override fun setId(id: String) = read.setId(id)

    override suspend fun next(): BridgeAlbumDetailSnapshot = read.next().let { (id, value) -> BridgeAlbumDetailSnapshot(id, value) }

    override suspend fun cancel() = read.cancel()

    override fun close() = read.cancel()
}

internal class FakeComposerDetailSubscription(
    val read: FakeDetailRead<BridgeComposerDetail>,
) : ComposerDetailSubscription(NoHandle) {
    override fun setId(id: String) = read.setId(id)

    override suspend fun next(): BridgeComposerDetailSnapshot = read.next().let { (id, value) -> BridgeComposerDetailSnapshot(id, value) }

    override suspend fun cancel() = read.cancel()

    override fun close() = read.cancel()
}

internal class FakeWorkDetailSubscription(
    val read: FakeDetailRead<BridgeWorkDetail>,
) : WorkDetailSubscription(NoHandle) {
    override fun setId(id: String) = read.setId(id)

    override suspend fun next(): BridgeWorkDetailSnapshot = read.next().let { (id, value) -> BridgeWorkDetailSnapshot(id, value) }

    override suspend fun cancel() = read.cancel()

    override fun close() = read.cancel()
}

internal class FakeReleaseDetailSubscription(
    val read: FakeDetailRead<BridgeRelease>,
) : ReleaseDetailSubscription(NoHandle) {
    override fun setId(id: String) = read.setId(id)

    override suspend fun next(): BridgeReleaseDetailSnapshot = read.next().let { (id, value) -> BridgeReleaseDetailSnapshot(id, value) }

    override suspend fun cancel() = read.cancel()

    override fun close() = read.cancel()
}

/**
 * A live search that answers each query it is pointed at with whatever [answer] gives for it, and
 * any value a test emits for the query it holds now.
 */
internal class FakeLibrarySearchSubscription(
    private val answer: (String) -> Result<BridgeSearchResults>?,
) : LibrarySearchSubscription(NoHandle) {
    private val events = Channel<Result<BridgeLibrarySearchSnapshot>>(Channel.UNLIMITED)
    private var query: String? = null
    private var revision = 0uL
    var cancelled = false
        private set
    var closed = false
        private set

    init {
        events.trySend(Result.success(BridgeLibrarySearchSnapshot(null, BridgeFixtures.searchResults(), revision)))
    }

    override fun setQuery(query: String?): ULong {
        if (cancelled) throw BridgeException.Cancelled()
        this.query = query?.trim()?.takeIf { it.isNotEmpty() }
        revision++
        this.query?.let { answer(it)?.let(::emit) }
        return revision
    }

    override suspend fun next(): BridgeLibrarySearchSnapshot = events.receive().getOrThrow()

    override suspend fun cancel() {
        cancelled = true
        events.trySend(Result.failure(BridgeException.Cancelled()))
    }

    override fun close() {
        cancelled = true
        closed = true
    }

    fun emit(
        value: Result<BridgeSearchResults>,
        revision: ULong? = null,
    ) {
        events.trySend(value.map { BridgeLibrarySearchSnapshot(query, it, revision ?: this.revision) })
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
        events.trySend(BridgeComposerBrowseSnapshot(emptyList(), 0uL, revision, BridgeLiveQueryCause.REQUEST))
    }

    override fun setWindows(windows: List<BridgeLibraryPageWindow>) {
        if (windows == this.windows) return
        this.windows = windows
        revision++
        val projected = windows.map { BridgeComposerBrowseWindow(it, rows(it.offset, it.limit), null, null) }
        events.trySend(
            BridgeComposerBrowseSnapshot(
                projected,
                projected.sumOf { it.rows.size }.toULong(),
                revision,
                BridgeLiveQueryCause.REQUEST,
            ),
        )
    }

    override suspend fun next(): BridgeComposerBrowseSnapshot = events.receive()

    override suspend fun cancel() {
        cancelled = true
    }
}

/** A composer's page over fixed rows: each window answers with its slice of them. */
internal class FakeComposerDetailRowsSubscription(
    private val rows: List<BridgeComposerDetailRow>,
) : ComposerDetailRowsSubscription(NoHandle) {
    private val events = Channel<BridgeComposerDetailRowsSnapshot>(Channel.UNLIMITED)
    private var revision = 0uL

    init {
        events.trySend(snapshot(emptyList()))
    }

    override fun setWindows(windows: List<BridgeLibraryPageWindow>) {
        revision++
        events.trySend(snapshot(windows))
    }

    private fun snapshot(windows: List<BridgeLibraryPageWindow>) =
        BridgeComposerDetailRowsSnapshot(
            windows.map { window ->
                BridgeComposerDetailRowsWindow(
                    window,
                    rows.drop(window.offset.toInt()).take(window.limit.toInt()),
                    null,
                    null,
                )
            },
            rows.size.toULong(),
            revision,
            BridgeLiveQueryCause.REQUEST,
        )

    override suspend fun next(): BridgeComposerDetailRowsSnapshot = events.receive()

    override suspend fun cancel() = Unit
}

private fun queryFailure(): BridgeException = BridgeException.Diagnostic(BridgeErrorCategory.Internal, "temporary")
