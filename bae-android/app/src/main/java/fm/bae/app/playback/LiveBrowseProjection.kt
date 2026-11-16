package fm.bae.app.playback

import androidx.media3.common.MediaItem
import fm.bae.app.data.LiveQueryEvent
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.pullEach
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collect
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

/**
 * A browse parent's live count of its children, read while a controller is interested in the parent. Each
 * count after the first tells the tree, through [onChanged], that the parent's children changed; the
 * children themselves are read once per page.
 */
internal class ChildCount(
    scope: CoroutineScope,
    private val count: PulledSubscription<BridgeListCount>,
    private val onChanged: (Int) -> Unit,
    private val onError: (BridgeException) -> Unit,
) {
    /** Completes at the count's first answer, or fails with its first error or when the read closes first. */
    private val answered = CompletableDeferred<Unit>()
    private val reader =
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            count.pullEach(
                onValue = { value -> if (!answered.complete(Unit)) onChanged(value.rows.toInt()) },
                onError = { error ->
                    onError(error)
                    answered.completeExceptionally(error)
                },
            )
        }

    /** Wait for the count's first answer, throwing its error. */
    suspend fun awaitAnswer() = answered.await()

    suspend fun close() {
        answered.completeExceptionally(treeClosedError())
        count.cancel()
        reader.cancelAndJoin()
    }
}
