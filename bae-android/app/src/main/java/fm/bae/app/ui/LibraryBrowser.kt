package fm.bae.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField

private const val TAG = "bae.LibraryBrowser"
private val logger = BaeLogger(TAG)
private val DEFAULT_ALBUM_SORT =
    BridgeSortCriterion(BridgeSortField.DATE_ADDED, BridgeSortDirection.DESCENDING)
private val DEFAULT_COMPOSER_SORT =
    BridgeComposerSortCriterion(BridgeComposerSortField.NAME, BridgeSortDirection.ASCENDING)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LibraryBrowser(
    session: OpenLibrary,
    onSelectAlbum: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectWork: (String) -> Unit,
    onSettings: () -> Unit,
) {
    var searchQuery by remember { mutableStateOf("") }
    var searchOpen by remember { mutableStateOf(false) }
    var mode by remember { mutableStateOf(LibraryBrowserMode.ALBUMS) }
    var sortCriterion by remember { mutableStateOf(DEFAULT_ALBUM_SORT) }
    var composerSortCriterion by remember { mutableStateOf(DEFAULT_COMPOSER_SORT) }
    val generation by session.libraryStore.generation.collectAsState()
    val composerGeneration by session.libraryStore.composerGeneration.collectAsState()
    val syncing by session.configStore.syncing.collectAsState()
    val syncError by session.configStore.syncError.collectAsState()
    val appError by session.configStore.error.collectAsState()
    val gridState = rememberLazyGridState()
    val libraryActionFailed = stringResource(R.string.library_load_failed)
    val page = rememberLibraryPage(session, generation, sortCriterion, LocalContext.current, gridState)
    val composerPage = rememberComposerPage(session, composerGeneration, composerSortCriterion, LocalContext.current)

    Column(modifier = Modifier.fillMaxSize()) {
        LibraryBrowserChrome(
            searchOpen = searchOpen,
            searchQuery = searchQuery,
            onSearchQueryChange = { searchQuery = it },
            onOpenSearch = { searchOpen = true },
            onCloseSearch = {
                searchOpen = false
                searchQuery = ""
            },
            mode = mode,
            onModeChange = { mode = it },
            sortCriterion = sortCriterion,
            onSortChange = { sortCriterion = it },
            composerSortCriterion = composerSortCriterion,
            onComposerSortChange = { composerSortCriterion = it },
            syncing = syncing,
            onShuffleLibrary = {
                try {
                    session.appHandle.playLibraryShuffled()
                } catch (e: Exception) {
                    logger.error("playLibraryShuffled failed", e)
                    session.configStore.showError(libraryActionFailed)
                }
            },
            onSettings = onSettings,
        )
        LibraryErrorBanner(page = page, appError = appError, syncError = syncError, session = session)
        LibraryBrowserContent(
            session = session,
            searchQuery = searchQuery,
            mode = mode,
            page = page,
            composerPage = composerPage,
            gridState = gridState,
            onSelectAlbum = onSelectAlbum,
            onSelectComposer = onSelectComposer,
            onSelectWork = onSelectWork,
        )
        NowPlayingBar(session = session)
    }
}

@Composable
private fun LibraryBrowserChrome(
    searchOpen: Boolean,
    searchQuery: String,
    onSearchQueryChange: (String) -> Unit,
    onOpenSearch: () -> Unit,
    onCloseSearch: () -> Unit,
    mode: LibraryBrowserMode,
    onModeChange: (LibraryBrowserMode) -> Unit,
    sortCriterion: BridgeSortCriterion,
    onSortChange: (BridgeSortCriterion) -> Unit,
    composerSortCriterion: BridgeComposerSortCriterion,
    onComposerSortChange: (BridgeComposerSortCriterion) -> Unit,
    syncing: Boolean,
    onShuffleLibrary: () -> Unit,
    onSettings: () -> Unit,
) {
    if (searchOpen) {
        LibrarySearchBar(
            query = searchQuery,
            onQueryChange = onSearchQueryChange,
            onClose = onCloseSearch,
        )
    } else {
        LibraryTopBar(
            onOpenSearch = onOpenSearch,
            onShuffleLibrary = onShuffleLibrary,
            mode = mode,
            sortCriterion = sortCriterion,
            onSortChange = onSortChange,
            composerSortCriterion = composerSortCriterion,
            onComposerSortChange = onComposerSortChange,
            onSettings = onSettings,
        )
    }
    LibraryModeBar(mode = mode, onModeChange = onModeChange)
    if (syncing) {
        LinearProgressIndicator(
            modifier = Modifier.fillMaxWidth(),
            color = MaterialTheme.colorScheme.primary,
            trackColor = MaterialTheme.colorScheme.surface,
        )
    }
}

@Composable
private fun ColumnScope.LibraryBrowserContent(
    session: OpenLibrary,
    searchQuery: String,
    mode: LibraryBrowserMode,
    page: LibraryPage,
    composerPage: ComposerPage,
    gridState: androidx.compose.foundation.lazy.grid.LazyGridState,
    onSelectAlbum: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectWork: (String) -> Unit,
) {
    Box(modifier = Modifier.fillMaxWidth().weight(1f)) {
        if (searchQuery.isNotBlank()) {
            SearchResultsScreen(
                session = session,
                query = searchQuery,
                onSelectAlbum = onSelectAlbum,
                onSelectComposer = onSelectComposer,
                onSelectWork = onSelectWork,
            )
        } else {
            when (mode) {
                LibraryBrowserMode.ALBUMS -> {
                    LibraryGridContent(
                        session = session,
                        page = page,
                        gridState = gridState,
                        onSelectAlbum = onSelectAlbum,
                    )
                }

                LibraryBrowserMode.COMPOSERS -> {
                    ComposerListContent(
                        page = composerPage,
                        loadImage = session.library::imageBytes,
                        onSelectComposer = onSelectComposer,
                    )
                }
            }
        }
    }
}
