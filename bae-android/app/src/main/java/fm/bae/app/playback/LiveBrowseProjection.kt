package fm.bae.app.playback

import androidx.media3.common.MediaItem
import fm.bae.app.data.AlbumBrowseQuery
import fm.bae.app.data.CollectionBrowseQuery
import fm.bae.app.data.ComposerBrowseQuery
import fm.bae.app.data.LiveQueryEvent
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.yield
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryPageWindow
import java.util.LinkedHashMap

private const val ACCESS_ORDER_INITIAL_CAPACITY = 16
private const val ACCESS_ORDER_LOAD_FACTOR = 0.75f
private const val MAXIMUM_EXACT_SUBSCRIPTIONS = 24

internal data class CollectionSnapshotReader<Row, Snapshot>(
    val windows: (Snapshot) -> Map<BridgeLibraryPageWindow, List<Row>>,
    val totalCount: (Snapshot) -> Int,
    val requestRevision: (Snapshot) -> ULong,
)

internal typealias ComposerCollectionProjection =
    CollectionProjection<uniffi.bae_bridge.BridgeComposerSummary, uniffi.bae_bridge.BridgeComposerBrowseSnapshot>

internal fun albumCollectionProjection(
    scope: CoroutineScope,
    query: AlbumBrowseQuery,
    onChanged: (Int) -> Unit,
    onError: (BridgeException) -> Unit,
) = CollectionProjection(
    scope,
    query,
    CollectionSnapshotReader(
        windows = { snapshot -> snapshot.windows.associate { it.window to it.rows } },
        totalCount = { it.totalCount.toInt() },
        requestRevision = { it.requestRevision },
    ),
    onChanged,
    onError,
)

internal fun composerCollectionProjection(
    scope: CoroutineScope,
    query: ComposerBrowseQuery,
    onChanged: (Int) -> Unit,
    onError: (BridgeException) -> Unit,
) = CollectionProjection(
    scope,
    query,
    CollectionSnapshotReader(
        windows = { snapshot -> snapshot.windows.associate { it.window to it.rows } },
        totalCount = { it.totalCount.toInt() },
        requestRevision = { it.requestRevision },
    ),
    onChanged,
    onError,
)

internal fun <Key, Value> exactProjectionCache(
    scope: CoroutineScope,
    flow: (Key) -> Flow<LiveQueryEvent<Value>>,
    onError: (BridgeException) -> Unit,
): LiveProjectionCache<Key, Value> =
    LiveProjectionCache(scope, MAXIMUM_EXACT_SUBSCRIPTIONS, flow) { _, event, _ ->
        (event as? LiveQueryEvent.Error)?.let { onError(it.error) }
    }

internal class CollectionProjection<Row : Any, Snapshot : Any>(
    scope: CoroutineScope,
    private val query: CollectionBrowseQuery<Snapshot>,
    private val snapshotReader: CollectionSnapshotReader<Row, Snapshot>,
    private val onChanged: (Int) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    private data class Waiter<Row>(
        val window: BridgeLibraryPageWindow,
        val result: CompletableDeferred<List<Row>>,
    )

    private val mutex = Mutex()
    private val requested =
        LinkedHashMap<BridgeLibraryPageWindow, Unit>(
            ACCESS_ORDER_INITIAL_CAPACITY,
            ACCESS_ORDER_LOAD_FACTOR,
            true,
        )
    private val waiters = mutableListOf<Waiter<Row>>()
    private val ready = CompletableDeferred<Unit>()
    private var delivered: Map<BridgeLibraryPageWindow, List<Row>> = emptyMap()
    private var deliveredRequest: Set<BridgeLibraryPageWindow> = emptySet()
    private var deliveredRevision: ULong? = null
    private var lastCount: Int? = null
    private var closed = false
    private val consumer: Job =
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            consume()
        }

    suspend fun awaitReady() {
        ready.await()
    }

    suspend fun rows(window: BridgeLibraryPageWindow): List<Row> {
        awaitReady()
        val waiter = CompletableDeferred<List<Row>>()
        mutex.withLock {
            checkOpen()
            requested[window] = Unit
            val evicted = mutableListOf<BridgeLibraryPageWindow>()
            while (requested.size > MAXIMUM_WINDOWS) {
                requested.entries.first().key.also {
                    requested.remove(it)
                    evicted += it
                }
            }
            waiters.removeAll { pending ->
                if (pending.window in evicted) {
                    pending.result.completeExceptionally(windowEvicted())
                    true
                } else {
                    false
                }
            }
            val absolute = requested.keys.toList()
            if (deliveredRequest == absolute.toSet()) {
                delivered[window]?.let {
                    waiter.complete(it)
                    return@withLock
                }
            }
            waiters += Waiter(window, waiter)
            try {
                query.setWindows(absolute)
            } catch (error: BridgeException) {
                waiters.removeAll { it.result === waiter }
                waiter.completeExceptionally(error)
            }
        }
        return waiter.await()
    }

    suspend fun close() {
        val error = treeClosedError()
        val shouldClose =
            mutex.withLock {
                if (closed) {
                    false
                } else {
                    closed = true
                    waiters.forEach { it.result.completeExceptionally(error) }
                    waiters.clear()
                    ready.complete(Unit)
                    true
                }
            }
        if (!shouldClose) return
        query.cancel()
        consumer.cancelAndJoin()
    }

    private suspend fun consume() {
        while (true) {
            try {
                val snapshot = query.next()
                val notify =
                    mutex.withLock {
                        if (closed) return
                        val value = snapshotReader.windows(snapshot)
                        val count = snapshotReader.totalCount(snapshot)
                        delivered = value
                        deliveredRequest = value.keys
                        lastCount = count
                        waiters.removeAll { waiter ->
                            value[waiter.window]?.let(waiter.result::complete) ?: false
                        }
                        ready.complete(Unit)
                        val revision = snapshotReader.requestRevision(snapshot)
                        (deliveredRevision == revision).also { deliveredRevision = revision }
                    }
                if (notify) onChanged(snapshotReader.totalCount(snapshot))
            } catch (error: BridgeException) {
                val count =
                    mutex.withLock {
                        if (closed) return
                        waiters.forEach { it.result.completeExceptionally(error) }
                        waiters.clear()
                        ready.complete(Unit)
                        lastCount
                    }
                onError(error)
                count?.let(onChanged)
                yield()
            }
        }
    }

    private fun checkOpen() {
        if (closed) throw treeClosedError()
    }

    private companion object {
        const val MAXIMUM_WINDOWS = 12
    }
}

internal object BrowsePaging {
    private const val MAX_PAGE_SIZE = 500

    fun window(
        page: Int,
        pageSize: Int,
    ) = BridgeLibraryPageWindow(offsetOf(page, pageSize), limitOf(pageSize))

    fun limitOf(pageSize: Int): ULong = pageSize.coerceIn(1, MAX_PAGE_SIZE).toULong()

    fun offsetOf(
        page: Int,
        pageSize: Int,
    ): ULong = (page.toLong().coerceAtLeast(0) * pageSize.toLong().coerceAtLeast(0)).toULong()

    fun paginate(
        items: List<MediaItem>,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val limit = pageSize.coerceIn(1, MAX_PAGE_SIZE)
        val start = page.toLong().coerceAtLeast(0) * pageSize.toLong().coerceAtLeast(0)
        if (start >= items.size) return emptyList()
        return items.drop(start.toInt()).take(limit)
    }
}

internal class FixedProjection<Value : Any>(
    scope: CoroutineScope,
    flow: Flow<LiveQueryEvent<Value>>,
    private val onChanged: (Value) -> Unit,
    private val onError: (BridgeException) -> Unit,
    private val notifyInitialValue: Boolean = false,
    startImmediately: Boolean = true,
) {
    private val lock = Any()
    private val ready = CompletableDeferred<Unit>()
    private var latest: LiveQueryEvent<Value>? = null
    private var initial = true
    private var closed = false
    private val job =
        scope.launch(start = CoroutineStart.LAZY) {
            flow.collect { event ->
                val notify =
                    synchronized(lock) {
                        if (closed) return@collect
                        latest = event
                        ready.complete(Unit)
                        (notifyInitialValue || !initial).also { initial = false }
                    }
                when (event) {
                    is LiveQueryEvent.Value -> if (notify) onChanged(event.value)
                    is LiveQueryEvent.Error -> onError(event.error)
                }
            }
        }

    init {
        if (startImmediately) job.start()
    }

    fun start() {
        job.start()
    }

    suspend fun value(): Value {
        ready.await()
        return synchronized(lock) {
            when (val event = latest ?: throw treeClosedError()) {
                is LiveQueryEvent.Value -> event.value
                is LiveQueryEvent.Error -> throw event.error
            }
        }
    }

    suspend fun close() {
        synchronized(lock) {
            if (closed) return
            closed = true
            ready.complete(Unit)
        }
        job.cancelAndJoin()
    }
}
