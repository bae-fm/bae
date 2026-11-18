package fm.bae.app.playback

import fm.bae.app.data.PageReads
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.pullEach
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListCount

private const val MAXIMUM_LIST_PAGES = 12

/**
 * A database list that a controller asks for a page at a time by offset: the albums, the composers, a composer's
 * page, an album's tracks, a work's children, or the albums a search lists. [item] makes each row the item the
 * tree shows.
 * Core reads each page live: the rows at the page's places, none past the list's end, which move with every write
 * that moves a row across them. A page stays live while it is among the last [MAXIMUM_LIST_PAGES] asked for, and a
 * repeat request for it answers with its latest rows. Each later delivery of a live page tells the tree, through
 * [onChanged], that the list changed, with the number of rows unknown. The list's live count tells the tree its
 * first number of rows and each change to it, and says which is the first.
 */
internal class ListProjection<Row, Item>(
    private val scope: CoroutineScope,
    private val reads: PageReads<Row>,
    private val item: (Row) -> Item,
    private val onChanged: (itemCount: Int, isFirstCount: Boolean) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    /** One page: its live read, and the items of its latest delivery, set before [first] completes. */
    private class Page<Row, Item>(
        val read: PulledSubscription<List<Row>>,
    ) {
        val first = CompletableDeferred<Unit>()
        var latest: List<Item>? = null
        lateinit var job: Job
    }

    private val mutex = Mutex()
    private val pages = LinkedHashMap<Pair<Int, Int>, Page<Row, Item>>(16, 0.75f, true)

    /**
     * The count's first result: completes at its first value or fails with its first error. A value after a
     * failure replaces it with one that has completed, so the list is ready again once its count reads again.
     */
    private var ready = CompletableDeferred<Unit>()

    /** Why the projection closed, which every later request fails with; null while it is open. */
    private var closedBy: BrowseUnavailable? = null
    private val countRead = reads.count()
    private var consumer: Job? = null

    /** Start reading the list's count, once the tree holds the projection, so that [onChanged] finds it. */
    fun start() {
        consumer = scope.launch(start = CoroutineStart.UNDISPATCHED) { consume() }
    }

    /** Wait for the list's first count, throwing its error. */
    suspend fun awaitReady() {
        mutex
            .withLock {
                closedBy?.let { throw it }
                ready
            }.await()
    }

    /** The items at the [limit] places from [offset], as their page's latest delivery holds them. */
    suspend fun rows(
        offset: Int,
        limit: Int,
    ): List<Item> {
        val page = page(offset, limit)
        page.first.await()
        return mutex.withLock { checkNotNull(page.latest) { "a page answered before its rows arrived" } }
    }

    /** The live page at [offset] of [limit] places, opened when no live page holds them. */
    private suspend fun page(
        offset: Int,
        limit: Int,
    ): Page<Row, Item> =
        mutex.withLock {
            closedBy?.let { throw it }
            val place = offset to limit
            val held = pages[place]
            if (held != null && !held.first.isCancelled) return@withLock held
            held?.let { drop(place, it) }
            open(place).also {
                while (pages.size > MAXIMUM_LIST_PAGES) {
                    pages.entries.first().let { oldest -> drop(oldest.key, oldest.value) }
                }
            }
        }

    /** Stop every read; a request waiting on one, and every later request, fails with [reason]. */
    suspend fun close(reason: BrowseUnavailable = treeClosedError()) {
        val toClose =
            mutex.withLock {
                if (closedBy != null) return
                closedBy = reason
                ready.completeExceptionally(reason)
                pages.values.toList().also {
                    it.forEach { page -> page.first.completeExceptionally(reason) }
                    pages.clear()
                }
            }
        try {
            toClose.forEach { it.read.cancel() }
            countRead.cancel()
        } finally {
            toClose.forEach { it.job.cancelAndJoin() }
            consumer?.cancelAndJoin()
        }
    }

    private fun open(place: Pair<Int, Int>): Page<Row, Item> {
        val (offset, limit) = place
        val page = Page<Row, Item>(reads.page(offset, limit))
        pages[place] = page
        page.job =
            scope.launch(start = CoroutineStart.UNDISPATCHED) {
                page.read.pullEach(
                    onValue = { rows ->
                        val items = rows.map(item)
                        val changed =
                            mutex.withLock {
                                val before = page.latest
                                page.latest = items
                                before != null && before != items && closedBy == null && pages[place] === page
                            }
                        page.first.complete(Unit)
                        if (changed) onChanged(UNKNOWN_CHILD_COUNT, false)
                    },
                    onError = { error ->
                        page.first.completeExceptionally(error)
                        onError(error)
                    },
                )
            }
        return page
    }

    private suspend fun drop(
        place: Pair<Int, Int>,
        page: Page<Row, Item>,
    ) {
        pages.remove(place)
        page.first.completeExceptionally(pageReplaced())
        page.read.cancel()
        page.job.cancel()
    }

    /** Each count value reaches [onChanged], with whether it is the count's first result. */
    private suspend fun consume() {
        countRead.pullEach(
            onValue = { value: BridgeListCount ->
                val isFirst =
                    mutex.withLock {
                        if (closedBy != null) return@pullEach
                        val isFirst = !ready.isCompleted
                        if (isFirst) {
                            ready.complete(Unit)
                        } else if (ready.isCancelled) {
                            ready = CompletableDeferred(Unit)
                        }
                        isFirst
                    }
                onChanged(value.rows.toInt(), isFirst)
            },
            onError = { error ->
                mutex.withLock {
                    if (closedBy != null) return@pullEach
                    ready.completeExceptionally(error)
                }
                onError(error)
            },
        )
    }
}
