package fm.bae.app.playback

import fm.bae.app.data.ListReads
import fm.bae.app.data.ListWindowRead
import fm.bae.screensubscription.pullEach
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListCount
import uniffi.bae_bridge.BridgeListWindow
import uniffi.bae_bridge.BridgeSortValue

private const val MAXIMUM_ALBUM_WINDOWS = 12

/**
 * The album grid's albums as Android Auto's browse tree asks for them, a page at a time by offset.
 * Media3 names a page by offset, which is a jump: the key of the album at that offset is a plain read, and
 * the page is a window read live from that key. A window stays live while it is among the last
 * [MAXIMUM_ALBUM_WINDOWS] asked for, and a window or the list's count changing tells the tree its children
 * changed.
 */
internal class AlbumListProjection(
    private val scope: CoroutineScope,
    private val reads: ListReads<BridgeAlbumAppearance>,
    private val onChanged: (Int) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    /** One page: the key it starts at, its live read, and the albums its first delivery held. */
    private class PageWindow(
        val key: List<BridgeSortValue>,
        val read: ListWindowRead<BridgeAlbumAppearance>,
    ) {
        val first = CompletableDeferred<List<BridgeAlbum>>()
        var latest: List<BridgeAlbum>? = null
        lateinit var job: Job
    }

    private val mutex = Mutex()
    private val windows = LinkedHashMap<Pair<Int, Int>, PageWindow>(16, 0.75f, true)
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
    ): List<BridgeAlbum> = page(offset, limit)?.first?.await() ?: emptyList()

    /** The window holding the page at [offset], or null when the list ends before it. */
    private suspend fun page(
        offset: Int,
        limit: Int,
    ): PageWindow? {
        ready.await()
        val total =
            mutex.withLock {
                checkOpen()
                count ?: throw checkNotNull(failure)
            }
        // The key of the album at that offset; a plain read, not a live one.
        val key = if (offset < total) reads.keyAt(offset.toULong()) else null
        return key?.let { held(offset to limit, it) }
    }

    /** The live window for [place] from [key]: the one held when it starts there and has not failed, else a new one. */
    private suspend fun held(
        place: Pair<Int, Int>,
        key: List<BridgeSortValue>,
    ): PageWindow =
        mutex.withLock {
            checkOpen()
            val held = windows[place]
            if (held != null && held.key == key && !held.first.isCancelled) return@withLock held
            held?.let { drop(place, it) }
            open(place, key).also {
                while (windows.size > MAXIMUM_ALBUM_WINDOWS) {
                    windows.entries.first().let { oldest -> drop(oldest.key, oldest.value) }
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
                windows.values.toList().also {
                    it.forEach { page -> page.first.completeExceptionally(error) }
                    windows.clear()
                }
            }
        toClose.forEach { it.read.cancel() }
        countRead.cancel()
        toClose.forEach { it.job.cancelAndJoin() }
        consumer.cancelAndJoin()
    }

    private fun open(
        place: Pair<Int, Int>,
        key: List<BridgeSortValue>,
    ): PageWindow {
        val request = BridgeListWindow.Forward(key, null, place.second.toULong())
        val page = PageWindow(key, reads.window(request))
        windows[place] = page
        page.job =
            scope.launch(start = CoroutineStart.UNDISPATCHED) {
                page.read.pullEach(
                    onValue = { delivery ->
                        if (delivery.window != request) return@pullEach
                        val albums = delivery.rows.map { it.album }
                        val changed =
                            mutex.withLock {
                                val before = page.latest
                                page.latest = albums
                                before != null && before != albums
                            }
                        page.first.complete(albums)
                        val total = mutex.withLock { if (closed || windows[place] !== page) null else count }
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
        page: PageWindow,
    ) {
        windows.remove(place)
        page.first.completeExceptionally(windowEvicted())
        page.read.cancel()
        page.job.cancel()
    }

    /** Each count value: the first makes the projection ready; a changed one drops the windows and tells the tree. */
    private suspend fun consume() {
        countRead.pullEach(
            onValue = { value: BridgeListCount ->
                val total = value.rows.toInt()
                val notify =
                    mutex.withLock {
                        if (closed) return@pullEach
                        val changed = count != null && count != total
                        count = total
                        if (changed) windows.entries.toList().forEach { drop(it.key, it.value) }
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
