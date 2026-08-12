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
    gridState: LazyGridState,
): AlbumPageStore {
    val page = session.browserPages.albums
    DisposableEffect(page, sortCriterion) {
        page.activate(sortCriterion)
        onDispose(page::deactivate)
    }
    LaunchedEffect(page, gridState) {
        snapshotFlow {
            val visible = gridState.layoutInfo.visibleItemsInfo
            (visible.firstOrNull()?.index ?: 0) to (visible.lastOrNull()?.index ?: 0)
        }.distinctUntilChanged()
            .collect { (first, last) -> page.reportVisibleRange(first, last) }
    }
    return page
}
