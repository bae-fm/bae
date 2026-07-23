package fm.bae.app.ui.library

import android.content.Context
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeSortCriterion

private const val TAG = "bae.LibraryPage"
private val logger = BaeLogger(TAG)
internal const val PAGE_SIZE = 60
private const val GRID_PREFETCH_AHEAD = 12

internal class PageError(
    val message: String,
    val onRetry: () -> Unit,
)

internal enum class LibraryBrowserMode {
    ALBUMS,
    COMPOSERS,
    ARTISTS,
}

/**
 * Loads and accumulates the album grid one page at a time. Owns the loaded
 * albums and their display order, plus the two loads: the first page, and each
 * next page as the grid scrolls near its end. Backed by snapshot state so reads
 * in composition recompose on change. (Each album carries its own cover
 * reference; the grid cards fetch the bytes by id.)
 */
internal class LibraryPage(
    private val session: OpenLibrary,
    private val sortCriterion: BridgeSortCriterion,
    private val appContext: Context,
    private val onRetry: () -> Unit,
) {
    val albums = mutableStateMapOf<String, BridgeAlbum>()
    val order = mutableStateListOf<String>()
    var totalCount by mutableStateOf(0)
        private set
    var loading by mutableStateOf(true)
        private set
    var error by mutableStateOf<PageError?>(null)
        private set
    private var loadedOffset = 0

    private fun ingest(page: List<BridgeAlbum>) {
        page.forEach { album ->
            if (!albums.containsKey(album.id)) order.add(album.id)
            albums[album.id] = album
        }
    }

    suspend fun loadFirst() {
        loading = true
        error = null
        try {
            val (count, page) =
                withContext(Dispatchers.IO) {
                    val c = session.library.albumCount().toInt()
                    val p = session.library.albumPage(listOf(sortCriterion), 0u, PAGE_SIZE.toULong())
                    c to p
                }
            totalCount = count
            ingest(page)
            loadedOffset = PAGE_SIZE
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to load first album page", e)
            error = PageError(e.message ?: appContext.getString(R.string.library_load_failed), onRetry)
        } finally {
            loading = false
        }
    }

    suspend fun loadMore() {
        if (order.size >= totalCount) return
        val offset = loadedOffset
        try {
            val more =
                withContext(Dispatchers.IO) {
                    session.library.albumPage(listOf(sortCriterion), offset.toULong(), PAGE_SIZE.toULong())
                }
            ingest(more)
            loadedOffset = offset + PAGE_SIZE
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to load album page at offset $offset", e)
            error = PageError(e.message ?: appContext.getString(R.string.library_load_more_failed), onRetry)
        }
    }
}

@Composable
internal fun rememberLibraryPage(
    session: OpenLibrary,
    generation: Long,
    sortCriterion: BridgeSortCriterion,
    appContext: Context,
    gridState: LazyGridState,
): LibraryPage {
    var retryToken by remember(generation, sortCriterion) { mutableStateOf(0) }
    val page =
        remember(generation, sortCriterion, retryToken) {
            LibraryPage(session, sortCriterion, appContext) { retryToken++ }
        }
    LaunchedEffect(page) { page.loadFirst() }
    val shouldLoadMore by remember {
        derivedStateOf {
            val lastVisible =
                gridState.layoutInfo.visibleItemsInfo
                    .lastOrNull()
                    ?.index ?: 0
            page.order.size < page.totalCount && lastVisible >= page.order.size - GRID_PREFETCH_AHEAD
        }
    }
    LaunchedEffect(shouldLoadMore, page.totalCount, page) {
        if (shouldLoadMore) page.loadMore()
    }
    return page
}
