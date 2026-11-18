package fm.bae.app.playback

import androidx.media3.common.MediaItem
import uniffi.bae_bridge.BridgeException

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
