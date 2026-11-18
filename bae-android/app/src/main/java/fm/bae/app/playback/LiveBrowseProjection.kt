package fm.bae.app.playback

import androidx.media3.common.MediaItem
import fm.bae.app.data.PageReads
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

/**
 * What the tree tells Media3 as a parent's number of children, or as a search's number of results, when a change
 * leaves it unknown.
 */
internal const val UNKNOWN_CHILD_COUNT = Int.MAX_VALUE

/**
 * A live count that a controller's interest holds open: the albums a search lists, or a browse parent's
 * children. The tree waits for the read's first count ([awaitFirst]). Each count reaches [onCount], with whether
 * it is the first, only while [isCurrent] says the tree still holds this read.
 */
internal class LiveCount(
    private val scope: CoroutineScope,
    private val count: PulledSubscription<BridgeListCount>,
    private val isCurrent: (LiveCount) -> Boolean,
    private val onCount: (rows: Int, isFirst: Boolean) -> Unit,
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
                if (isCurrent(this@LiveCount)) onCount(value.rows.toInt(), isFirst)
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
 * A database list a controller asks for a page at a time by offset: a browse parent's children, such as an
 * album's tracks or a work's children, or the albums a search lists. For each page asked for, the tree opens the
 * page's live read, whose rows core reads at the page's places, none past the list's end. The page's first rows
 * answer the request, and each later delivery tells the tree, through [onChanged], that the list changed, with the
 * number of rows unknown. A new request for the same page replaces its read. The list's live count tells the tree
 * its first number of rows and each change to it, and says which is the first.
 */
internal class ChildPages<Row>(
    private val scope: CoroutineScope,
    private val reads: PageReads<Row>,
    private val item: (Row) -> MediaItem,
    private val onChanged: (itemCount: Int, isFirstCount: Boolean) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    private val lock = Any()

    /** Why the pages closed, which a request that arrives after they closed fails with; null while they are open. */
    private var closedBy: BrowseUnavailable? = null

    /** The live read of each page a controller asked for, by the page's offset and size. */
    private val pages = mutableMapOf<Pair<Int, Int>, LivePage>()
    private val count =
        LiveCount(
            scope,
            reads.count(),
            isCurrent = { synchronized(lock) { closedBy == null } },
            onCount = onChanged,
            onError = onError,
        )

    /** Start reading the list's count, once the tree holds the pages, so that its callbacks find them. */
    fun start() = count.start()

    /** Wait for the list's first count, throwing its error. */
    suspend fun awaitReady() = count.awaitFirst()

    /** The [limit] children from [offset], read live from now on; none when the list ends before [offset]. */
    suspend fun rows(
        offset: Int,
        limit: Int,
    ): List<MediaItem> {
        val page = LivePage(offset, limit)
        val (refusal, replaced) =
            synchronized(lock) {
                closedBy?.let { it to null } ?: (null to pages.put(offset to limit, page))
            }
        replaced?.close(pageReplaced())
        if (refusal != null) throw refusal
        page.start()
        return page.awaitFirst()
    }

    /** Stop every read; a request waiting on one, and every later request, fails with [reason]. */
    suspend fun close(reason: BrowseUnavailable = treeClosedError()) {
        val open =
            synchronized(lock) {
                if (closedBy != null) return
                closedBy = reason
                pages.values.toList().also { pages.clear() }
            }
        count.close(reason)
        open.forEach { it.close(reason) }
    }

    /** One page's live read: its first rows answer the request, and each later delivery is a change. */
    private inner class LivePage(
        private val offset: Int,
        private val limit: Int,
    ) {
        private val first = CompletableDeferred<List<MediaItem>>()
        private var read: PulledSubscription<List<Row>>? = null
        private var reader: Job? = null

        fun start() {
            val read = reads.page(offset, limit).also { this.read = it }
            reader =
                scope.launch(start = CoroutineStart.UNDISPATCHED) {
                    read.pullEach(
                        onValue = { rows ->
                            if (!first.isCompleted) {
                                first.complete(rows.map(item))
                            } else if (isCurrent()) {
                                onChanged(UNKNOWN_CHILD_COUNT, false)
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

        private fun isCurrent(): Boolean = synchronized(lock) { closedBy == null && pages.values.any { it === this } }
    }
}
