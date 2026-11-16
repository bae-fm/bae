package fm.bae.app.playback

import androidx.media3.common.MediaItem
import fm.bae.app.data.ListReads
import fm.bae.app.data.ListWindowRead
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.pullEach
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListCount
import uniffi.bae_bridge.BridgeListWindow

/** Reads the database once for a request, and reports a failure to [onError] before it throws the failure again. */
internal suspend fun <Value> reportingFailure(
    onError: (BridgeException) -> Unit,
    read: suspend () -> Value,
): Value =
    try {
        read()
    } catch (error: BridgeException) {
        throw error.also(onError)
    }

/**
 * A controller's page of a browse list, exactly as it asked: [pageSize] items
 * from item `page × pageSize`. Callers answer a request that names no page
 * ([isPage] false) as a bad value rather than reading some other page.
 */
internal object BrowsePaging {
    /** Whether [page] and [pageSize] name a page: one from 0, of at least one item. */
    fun isPage(
        page: Int,
        pageSize: Int,
    ): Boolean = page >= 0 && pageSize >= 1

    private fun offsetOf(
        page: Int,
        pageSize: Int,
    ): ULong = page.toULong() * pageSize.toULong()

    /** The requested page of the root's [categories], a fixed list the app defines rather than rows a read returned. */
    fun categoryPage(
        categories: List<MediaItem>,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val start = offsetOf(page, pageSize)
        if (start >= categories.size.toULong()) return emptyList()
        return categories.drop(start.toInt()).take(pageSize)
    }
}

/** What Media3 takes as a parent's number of children when a change leaves it unknown. */
internal const val UNKNOWN_CHILD_COUNT = Int.MAX_VALUE

/**
 * A live count that a controller's interest holds open: the albums a search lists, or a browse parent's
 * children. The tree waits for the read's first count ([awaitFirst]). Each later count reaches [onCount], and
 * so does the first when [tellsFirst]. A count reaches [onCount] only while [isCurrent] says the tree still
 * holds this read.
 */
internal class LiveCount(
    private val scope: CoroutineScope,
    private val count: PulledSubscription<BridgeListCount>,
    private val tellsFirst: Boolean,
    private val isCurrent: (LiveCount) -> Boolean,
    private val onCount: (Int) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    /** Completes at the read's first count, or fails with its first error or when the tree closes it first. */
    private val first = CompletableDeferred<Unit>()
    private var reader: Job? = null

    /** Start reading. The tree starts the read once it holds it, so that [isCurrent] finds it. */
    fun start() {
        reader = scope.launch(start = CoroutineStart.UNDISPATCHED) { read() }
    }

    /** Wait for the read's first count, throwing its error. */
    suspend fun awaitFirst() = first.await()

    /** Stop reading; a wait for the first count gets [reason] when the read closes before it. */
    suspend fun close(reason: BrowseUnavailable) {
        first.completeExceptionally(reason)
        count.cancel()
        reader?.cancelAndJoin()
    }

    private suspend fun read() {
        count.pullEach(
            onValue = { value ->
                val isFirst = !first.isCompleted
                if ((tellsFirst || !isFirst) && isCurrent(this@LiveCount)) onCount(value.rows.toInt())
                first.complete(Unit)
            },
            onError = { error ->
                onError(error)
                first.completeExceptionally(error)
            },
        )
    }
}

/**
 * A browse parent whose children are a database list, such as an album's tracks or a work's children, which a
 * controller asks for a page at a time by offset. For each page asked for, the tree reads once the window that
 * holds the page ([pageWindow]) and then reads that window live. The window's first rows answer the request, and
 * each later delivery tells the tree, through [onChanged], that the parent's children changed. A new request for
 * the same page replaces its read. The list's live count tells the tree each time the number of children changes.
 */
internal class ChildPages<Row>(
    private val scope: CoroutineScope,
    private val reads: ListReads<Row>,
    private val pageWindow: suspend (offset: ULong, limit: ULong) -> BridgeListWindow?,
    private val item: (Row) -> MediaItem,
    private val onChanged: (itemCount: Int) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    private val lock = Any()
    private var closed = false

    /** The live read of each page a controller asked for, by the page's offset and size. */
    private val pages = mutableMapOf<Pair<Int, Int>, LivePage>()
    private val count =
        LiveCount(
            scope,
            reads.count(),
            tellsFirst = false,
            isCurrent = { synchronized(lock) { !closed } },
            onCount = onChanged,
            onError = onError,
        ).also { it.start() }

    /** Wait for the list's first count, throwing its error. */
    suspend fun awaitReady() = count.awaitFirst()

    /** The [limit] children from [offset], read live from now on; none when the list ends before [offset]. */
    suspend fun rows(
        offset: Int,
        limit: Int,
    ): List<MediaItem> {
        val window = reportingFailure(onError) { pageWindow(offset.toULong(), limit.toULong()) } ?: return emptyList()
        val page = LivePage(window)
        val (opened, replaced) =
            synchronized(lock) {
                if (closed) false to null else true to pages.put(offset to limit, page)
            }
        replaced?.close(windowEvicted())
        if (!opened) throw treeClosedError()
        page.start()
        return page.awaitFirst()
    }

    suspend fun close() {
        val open =
            synchronized(lock) {
                if (closed) return
                closed = true
                pages.values.toList().also { pages.clear() }
            }
        count.close(treeClosedError())
        open.forEach { it.close(treeClosedError()) }
    }

    /** One page's live window: its first rows answer the request, and each later delivery is a change. */
    private inner class LivePage(
        private val window: BridgeListWindow,
    ) {
        private val first = CompletableDeferred<List<MediaItem>>()
        private var read: ListWindowRead<Row>? = null
        private var reader: Job? = null

        fun start() {
            val read = reads.window(window).also { this.read = it }
            reader =
                scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    read.pullEach(
                        onValue = { delivery ->
                            if (!first.isCompleted) {
                                first.complete(delivery.rows.map(item))
                            } else if (isCurrent()) {
                                onChanged(UNKNOWN_CHILD_COUNT)
                            }
                        },
                        onError = { error ->
                            onError(error)
                            first.completeExceptionally(error)
                        },
                    )
                }
        }

        suspend fun awaitFirst(): List<MediaItem> = first.await()

        suspend fun close(reason: BrowseUnavailable) {
            first.completeExceptionally(reason)
            read?.cancel()
            reader?.cancelAndJoin()
        }

        private fun isCurrent(): Boolean = synchronized(lock) { !closed && pages.values.any { it === this } }
    }
}
