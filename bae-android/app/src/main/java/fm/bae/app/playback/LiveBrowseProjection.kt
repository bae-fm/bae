package fm.bae.app.playback

import androidx.media3.common.MediaItem
import fm.bae.app.data.LiveQueryEvent
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException

private const val MAXIMUM_EXACT_SUBSCRIPTIONS = 24

internal fun <Key, Value> exactProjectionCache(
    scope: CoroutineScope,
    flow: (Key) -> Flow<LiveQueryEvent<Value>>,
    onError: (BridgeException) -> Unit,
): LiveProjectionCache<Key, Value> =
    LiveProjectionCache(scope, MAXIMUM_EXACT_SUBSCRIPTIONS, flow) { _, event, _ ->
        (event as? LiveQueryEvent.Error)?.let { onError(it.error) }
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

    fun paginate(
        items: List<MediaItem>,
        page: Int,
        pageSize: Int,
    ): List<MediaItem> {
        val start = offsetOf(page, pageSize)
        if (start >= items.size.toULong()) return emptyList()
        return items.drop(start.toInt()).take(pageSize)
    }
}

internal class FixedProjection<Value : Any>(
    scope: CoroutineScope,
    flow: Flow<LiveQueryEvent<Value>>,
    private val onChanged: (Value) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    private val lock = Any()
    private val ready = CompletableDeferred<Unit>()
    private var latest: LiveQueryEvent<Value>? = null
    private var initial = true
    private var closed = false
    private val job =
        scope.launch {
            flow.collect { event ->
                val notify =
                    synchronized(lock) {
                        if (closed) return@collect
                        latest = event
                        ready.complete(Unit)
                        (!initial).also { initial = false }
                    }
                when (event) {
                    is LiveQueryEvent.Value -> if (notify) onChanged(event.value)
                    is LiveQueryEvent.Error -> onError(event.error)
                }
            }
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
