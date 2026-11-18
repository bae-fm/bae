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
 * A list's items as Android Auto's browse tree asks for them, a page at a time by offset; [item] makes each row
 * the item the tree shows.
 * Media3 asks for a page by offset, and core reads each page live: the rows at the page's places, none past the
 * list's end, which move with every write that moves a row across them. A page stays live while it is among the
 * last [MAXIMUM_LIST_PAGES] asked for, and a page or the list's count changing tells the tree its children
 * changed.
 */
internal class ListProjection<Row, Item>(
    private val scope: CoroutineScope,
    private val reads: PageReads<Row>,
    private val item: (Row) -> Item,
    private val onChanged: (Int) -> Unit,
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
    private val ready = CompletableDeferred<Unit>()
    private var closed = false
    private val countRead = reads.count()
    private val consumer: Job = scope.launch(start = CoroutineStart.UNDISPATCHED) { consume() }

    /** Wait for the list's first count, or its failure. */
    suspend fun awaitReady() {
        ready.await()
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
            checkOpen()
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

    suspend fun close() {
        val error = treeClosedError()
        val toClose =
            mutex.withLock {
                if (closed) return
                closed = true
                ready.complete(Unit)
                pages.values.toList().also {
                    it.forEach { page -> page.first.completeExceptionally(error) }
                    pages.clear()
                }
            }
        try {
            toClose.forEach { it.read.cancel() }
            countRead.cancel()
        } finally {
            toClose.forEach { it.job.cancelAndJoin() }
            consumer.cancelAndJoin()
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
                                before != null && before != items && !closed && pages[place] === page
                            }
                        page.first.complete(Unit)
                        if (changed) onChanged(UNKNOWN_CHILD_COUNT)
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

    /** Each count value: the first makes the projection ready, and each later one tells the tree. */
    private suspend fun consume() {
        countRead.pullEach(
            onValue = { value: BridgeListCount ->
                val notify =
                    mutex.withLock {
                        if (closed) return@pullEach
                        val later = ready.isCompleted
                        ready.complete(Unit)
                        later
                    }
                if (notify) onChanged(value.rows.toInt())
            },
            onError = { error ->
                mutex.withLock {
                    if (closed) return@pullEach
                    ready.complete(Unit)
                }
                onError(error)
            },
        )
    }

    private fun checkOpen() {
        if (closed) throw treeClosedError()
    }
}
