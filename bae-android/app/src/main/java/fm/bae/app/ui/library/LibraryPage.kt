package fm.bae.app.ui.library

import android.content.Context
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
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
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AlbumPageCallback
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumPage
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.LiveSubscription

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
    private val scope: CoroutineScope,
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
    private val pages = mutableMapOf<Int, List<String>>()
    private val subscriptions = mutableMapOf<Int, LiveSubscription>()

    private fun ingest(offset: Int, page: BridgeAlbumPage) {
        page.rows.forEach { album ->
            albums[album.id] = album
        }
        pages[offset] = page.rows.map { it.id }
        order.clear()
        order.addAll(pages.toSortedMap().values.flatten().distinct())
        totalCount = page.totalCount.toInt()
        loading = false
        error = null
    }

    fun start() {
        loading = true
        error = null
        subscribe(0)
    }

    fun loadMore() {
        if (order.size >= totalCount) return
        val offset = ((pages.keys.maxOrNull() ?: -PAGE_SIZE) + PAGE_SIZE)
        subscribe(offset)
    }

    private fun subscribe(offset: Int) {
        if (subscriptions.containsKey(offset)) return
        subscriptions[offset] =
            session.library.subscribeAlbumPage(
                sortCriteria = listOf(sortCriterion),
                offset = offset.toULong(),
                limit = PAGE_SIZE.toULong(),
                callback =
                    object : AlbumPageCallback {
                        override fun onValue(value: BridgeAlbumPage) {
                            scope.launch(Dispatchers.Main.immediate) { ingest(offset, value) }
                        }

                        override fun onError(errorValue: BridgeException) {
                            logger.error("Album page subscription failed at offset $offset", errorValue)
                            scope.launch(Dispatchers.Main.immediate) {
                                loading = false
                                error =
                                    PageError(
                                        errorValue.message
                                            ?: appContext.getString(
                                                if (offset == 0) R.string.library_load_failed
                                                else R.string.library_load_more_failed,
                                            ),
                                        onRetry,
                                    )
                            }
                        }
                    },
            )
    }

    fun cancel() {
        subscriptions.values.forEach(LiveSubscription::cancel)
        subscriptions.clear()
    }
}

@Composable
internal fun rememberLibraryPage(
    session: OpenLibrary,
    sortCriterion: BridgeSortCriterion,
    appContext: Context,
    gridState: LazyGridState,
): LibraryPage {
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    var retryToken by remember(sortCriterion) { mutableStateOf(0) }
    val page =
        remember(sortCriterion, retryToken) {
            LibraryPage(session, sortCriterion, appContext, scope) { retryToken++ }
        }
    DisposableEffect(page) {
        page.start()
        onDispose(page::cancel)
    }
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
