package fm.bae.app.ui.library

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ComposerPageStore
import fm.bae.app.ui.components.AndroidLibraryRow
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeComposerSortCriterion

@Composable
internal fun rememberComposerPage(
    session: OpenLibrary,
    sortCriterion: BridgeComposerSortCriterion,
): ComposerPageStore {
    val page = session.browserPages.composers
    DisposableEffect(page, sortCriterion) {
        page.activate(sortCriterion)
        onDispose(page::deactivate)
    }
    return page
}

@Composable
internal fun ComposerListContent(
    session: OpenLibrary,
    page: ComposerPageStore,
    listState: LazyListState,
    onSelectComposer: (String) -> Unit,
) {
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage = stringResource(R.string.library_empty_composers),
    ) {
        LaunchedEffect(page, listState) {
            snapshotFlow {
                val visible = listState.layoutInfo.visibleItemsInfo
                (visible.firstOrNull()?.index ?: 0) to (visible.lastOrNull()?.index ?: 0)
            }.distinctUntilChanged().collect { (first, last) ->
                page.reportVisibleRange(first, last)
            }
        }
        val context = LocalContext.current
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            items(
                count = page.totalCount,
                key = { index -> page.rowAt(index)?.artistId ?: "composer-slot-$index" },
            ) { index ->
                page.rowAt(index)?.let { composer ->
                    AndroidLibraryRow(
                        title = composer.name,
                        subtitle = context.coreString(composer.countLine),
                        art = composer.image,
                        missing = composer.image == null,
                        onClick = { onSelectComposer(composer.artistId) },
                    )
                }
            }
        }
    }
}
