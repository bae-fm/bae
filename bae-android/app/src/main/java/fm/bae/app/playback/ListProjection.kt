package fm.bae.app.playback

import fm.bae.app.data.ListReads
import fm.bae.app.data.ListViewRead
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
 * Media3 asks for a page by offset, and a car shows one page at a time, so the projection opens a view of the list
 * with the page's rows on screen, whose window core places around them. A page stays live while it is among the
 * last [MAXIMUM_LIST_PAGES] asked for, and a page or the list's count changing tells the tree its children changed.
 */
internal class ListProjection<Row, Item>(
    private val scope: CoroutineScope,
    private val reads: ListReads<Row>,
    private val item: (Row) -> Item,
    private val onChanged: (Int) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    /** One page: its view's live read, and the items its first delivery held. */
    private class Page<Row, Item>(
        val read: ListViewRead<Row>,
    ) {
        val first = CompletableDeferred<List<Item>>()
        var latest: List<Item>? = null
        lateinit var job: Job
    }

    private val mutex = Mutex()
    private val pages = LinkedHashMap<Pair<Int, Int>, Page<Row, Item>>(16, 0.75f, true)
    private val ready = CompletableDeferred<Unit>()
    private var count: Int? = null
    private var failure: BridgeException? = null
    private var closed = false
    private val countRead = reads.count()
    private val consumer: Job = scope.launch(start = CoroutineStart.UNDISPATCHED) { consume() }

    suspend fun awaitReady() {
        ready.await()
    }

    suspend fun rows(
        offset: Int,
        limit: Int,
    ): List<Item> {
        val page = page(offset, limit) ?: return emptyList()
        page.first.await()
        return mutex.withLock { page.latest }.orEmpty()
    }

    /** The live page at [offset], or null when the list ends before it. */
    private suspend fun page(
        offset: Int,
        limit: Int,
    ): Page<Row, Item>? {
        ready.await()
        return mutex.withLock {
            checkOpen()
            val total = count ?: throw checkNotNull(failure)
            if (offset >= total || limit <= 0) return@withLock null
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
        val page = Page<Row, Item>(reads.view(offset, offset + limit - 1))
        pages[place] = page
        page.job =
            scope.launch(start = CoroutineStart.UNDISPATCHED) {
                page.read.pullEach(
                    onValue = { delivery ->
                        val items = delivery.page(offset, limit).map(item)
                        val changed =
                            mutex.withLock {
                                val before = page.latest
                                page.latest = items
                                before != null && before != items
                            }
                        page.first.complete(items)
                        val total = mutex.withLock { if (closed || pages[place] !== page) null else count }
                        if (changed && total != null) onChanged(total)
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

    /** Each count value: the first makes the projection ready; a changed one drops the pages and tells the tree. */
    private suspend fun consume() {
        countRead.pullEach(
            onValue = { value: BridgeListCount ->
                val total = value.rows.toInt()
                val notify =
                    mutex.withLock {
                        if (closed) return@pullEach
                        val changed = count != null && count != total
                        count = total
                        if (changed) pages.entries.toList().forEach { drop(it.key, it.value) }
                        ready.complete(Unit)
                        changed
                    }
                if (notify) onChanged(total)
            },
            onError = { error ->
                mutex.withLock {
                    if (closed) return@pullEach
                    failure = error
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
