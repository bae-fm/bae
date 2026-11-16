package fm.bae.app.playback

import androidx.media3.common.MediaItem
import fm.bae.screensubscription.PulledSubscription
import fm.bae.screensubscription.pullEach
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
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

    /** The page asked for of the root's [categories], a list the app fixes, not rows a read returned. */
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
