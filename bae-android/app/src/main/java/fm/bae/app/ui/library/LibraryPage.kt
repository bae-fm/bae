package fm.bae.app.ui.library

import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import fm.bae.app.OpenLibrary
import fm.bae.app.data.AlbumPageStore
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeSortCriterion

internal enum class LibraryBrowserMode {
    ALBUMS,
    COMPOSERS,
    ARTISTS,
}

/** Reports the album grid's parameters and visible rows to the session-owned
 * page store. The store owns subscriptions, page data, merge, and errors. */
@Composable
internal fun rememberLibraryPage(
    session: OpenLibrary,
    sortCriterion: BridgeSortCriterion,
    groupByArtist: Boolean,
    gridState: LazyGridState,
): AlbumPageStore {
    val page = session.browserPages.albums
    DisposableEffect(page, sortCriterion, groupByArtist) {
        page.activate(sortCriterion, groupByArtist)
        onDispose(page::deactivate)
    }
    LaunchedEffect(page, gridState, sortCriterion, groupByArtist) {
        snapshotFlow {
            // Album keys are their list positions; artist headings have string keys.
            val visible = gridState.layoutInfo.visibleItemsInfo.mapNotNull { it.key as? Int }
            Triple(visible.firstOrNull() ?: 0, visible.lastOrNull() ?: 0, page.totalCount)
        }.distinctUntilChanged()
            .collect { (first, last, total) ->
                if (total > 0) page.reportVisibleRange(first, last)
            }
    }
    return page
}
