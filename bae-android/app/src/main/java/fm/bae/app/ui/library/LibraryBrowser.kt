package fm.bae.app.ui.library

import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.DownloadStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.downloads.DownloadsSummaryStrip
import fm.bae.app.ui.playback.NowPlayingBar
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSortField
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
private val DEFAULT_ARTIST_SORT =
    BridgeArtistSortCriterion(BridgeArtistSortField.NAME, BridgeSortDirection.ASCENDING)

/**
 * Browser chrome state, owned above the stack in [LibraryScreen] so tab, sort,
 * search, and grid scroll survive while a pushed destination hides the browser.
 */
internal class LibraryBrowserState {
    var searchOpen by mutableStateOf(false)
    var searchQuery by mutableStateOf("")
    var mode by mutableStateOf(LibraryBrowserMode.ALBUMS)
    var sortCriterion by mutableStateOf(DEFAULT_ALBUM_SORT)
    var composerSortCriterion by mutableStateOf(DEFAULT_COMPOSER_SORT)
    var artistSortCriterion by mutableStateOf(DEFAULT_ARTIST_SORT)
    val gridState = LazyGridState()

    fun closeSearch() {
        searchOpen = false
        searchQuery = ""
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun LibraryBrowser(
    session: OpenLibrary,
    state: LibraryBrowserState,
    onSelectAlbum: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
    onSelectWork: (String) -> Unit,
    onSettings: () -> Unit,
    onDownloads: () -> Unit,
) {
    val generation by session.libraryStore.generation.collectAsState()
    val composerGeneration by session.libraryStore.composerGeneration.collectAsState()
    val artistGeneration by session.libraryStore.artistGeneration.collectAsState()
    val syncing by session.configStore.syncing.collectAsState()
    val syncError by session.configStore.syncError.collectAsState()
    val appError by session.configStore.error.collectAsState()
    val appContext = LocalContext.current
    val coroutineScope = rememberCoroutineScope()

    BackHandler(enabled = state.searchOpen) { state.closeSearch() }
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryBrowserChrome(
            searchOpen = state.searchOpen,
            searchQuery = state.searchQuery,
            onSearchQueryChange = { state.searchQuery = it },
            onOpenSearch = { state.searchOpen = true },
            onCloseSearch = state::closeSearch,
            mode = state.mode,
            onModeChange = { state.mode = it },
            sortCriterion = state.sortCriterion,
            onSortChange = { state.sortCriterion = it },
            composerSortCriterion = state.composerSortCriterion,
            onComposerSortChange = { state.composerSortCriterion = it },
            artistSortCriterion = state.artistSortCriterion,
            onArtistSortChange = { state.artistSortCriterion = it },
            syncing = syncing,
            onShuffleLibrary = { session.playLibraryShuffledOrReport(appContext, coroutineScope) },
            onSettings = onSettings,
        )
        DownloadsStrip(session.downloadStore, onDownloads)
        LibraryBrowserContent(
            modifier = Modifier.fillMaxWidth().weight(1f),
            session = session,
            searchQuery = state.searchQuery,
            mode = state.mode,
            generation = generation,
            composerGeneration = composerGeneration,
            artistGeneration = artistGeneration,
            sortCriterion = state.sortCriterion,
            composerSortCriterion = state.composerSortCriterion,
            artistSortCriterion = state.artistSortCriterion,
            appError = appError,
            syncError = syncError,
            gridState = state.gridState,
            onSelectAlbum = onSelectAlbum,
            onSelectComposer = onSelectComposer,
            onSelectArtist = onSelectArtist,
            onSelectWork = onSelectWork,
            appContext = appContext,
        )
        NowPlayingBar(session = session)
    }
}

@Composable
private fun DownloadsStrip(
    downloadStore: DownloadStore,
    onTap: () -> Unit,
) {
    val snapshot by downloadStore.snapshot.collectAsState()
    if (snapshot.downloads.isNotEmpty()) {
        DownloadsSummaryStrip(snapshot = snapshot, onTap = onTap)
    }
}

private fun OpenLibrary.playLibraryShuffledOrReport(
    appContext: Context,
    coroutineScope: CoroutineScope,
) {
    coroutineScope.launch {
        try {
            withContext(Dispatchers.IO) {
                appHandle.playLibraryShuffled()
            }
        } catch (e: Exception) {
            logger.error("playLibraryShuffled failed", e)
            configStore.showError(appContext.getString(R.string.library_load_failed))
        }
    }
}

@Composable
private fun LibraryBrowserContent(
    modifier: Modifier,
    session: OpenLibrary,
    searchQuery: String,
    mode: LibraryBrowserMode,
    generation: Long,
    composerGeneration: Long,
    artistGeneration: Long,
    sortCriterion: BridgeSortCriterion,
    composerSortCriterion: BridgeComposerSortCriterion,
    artistSortCriterion: BridgeArtistSortCriterion,
    appError: String?,
    syncError: String?,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
    onSelectWork: (String) -> Unit,
    appContext: Context,
) {
    Box(modifier = modifier) {
        if (searchQuery.isNotBlank()) {
            SearchResultsScreen(
                session = session,
                query = searchQuery,
                onSelectAlbum = onSelectAlbum,
                onSelectArtist = onSelectArtist,
                onSelectComposer = onSelectComposer,
                onSelectWork = onSelectWork,
            )
        } else {
            when (mode) {
                LibraryBrowserMode.ALBUMS -> {
                    AlbumBrowserContent(
                        session = session,
                        generation = generation,
                        sortCriterion = sortCriterion,
                        appError = appError,
                        syncError = syncError,
                        gridState = gridState,
                        onSelectAlbum = onSelectAlbum,
                        appContext = appContext,
                    )
                }

                LibraryBrowserMode.COMPOSERS -> {
                    ComposerBrowserContent(
                        session = session,
                        generation = composerGeneration,
                        sortCriterion = composerSortCriterion,
                        appError = appError,
                        syncError = syncError,
                        onSelectComposer = onSelectComposer,
                        appContext = appContext,
                    )
                }

                LibraryBrowserMode.ARTISTS -> {
                    ArtistBrowserContent(
                        session = session,
                        generation = artistGeneration,
                        sortCriterion = artistSortCriterion,
                        appError = appError,
                        syncError = syncError,
                        onSelectArtist = onSelectArtist,
                        appContext = appContext,
                    )
                }
            }
        }
    }
}

@Composable
private fun AlbumBrowserContent(
    session: OpenLibrary,
    generation: Long,
    sortCriterion: BridgeSortCriterion,
    appError: String?,
    syncError: String?,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
    appContext: Context,
) {
    val page =
        rememberLibraryPage(
            session = session,
            generation = generation,
            sortCriterion = sortCriterion,
            appContext = appContext,
            gridState = gridState,
        )
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryErrorBanner(
            page = page,
            appError = appError,
            syncError = syncError,
            session = session,
        )
        LibraryGridContent(
            session = session,
            page = page,
            gridState = gridState,
            onSelectAlbum = onSelectAlbum,
        )
    }
}

@Composable
private fun ComposerBrowserContent(
    session: OpenLibrary,
    generation: Long,
    sortCriterion: BridgeComposerSortCriterion,
    appError: String?,
    syncError: String?,
    onSelectComposer: (String) -> Unit,
    appContext: Context,
) {
    val page =
        rememberComposerPage(
            session = session,
            generation = generation,
            sortCriterion = sortCriterion,
            appContext = appContext,
        )
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryGlobalErrorBanner(
            appError = appError,
            syncError = syncError,
            session = session,
        )
        ComposerListContent(
            page = page,
            onSelectComposer = onSelectComposer,
        )
    }
}

@Composable
private fun ArtistBrowserContent(
    session: OpenLibrary,
    generation: Long,
    sortCriterion: BridgeArtistSortCriterion,
    appError: String?,
    syncError: String?,
    onSelectArtist: (String) -> Unit,
    appContext: Context,
) {
    val page =
        rememberArtistPage(
            session = session,
            generation = generation,
            sortCriterion = sortCriterion,
            appContext = appContext,
        )
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryGlobalErrorBanner(
            appError = appError,
            syncError = syncError,
            session = session,
        )
        ArtistListContent(
            page = page,
            onSelectArtist = onSelectArtist,
        )
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
    artistSortCriterion: BridgeArtistSortCriterion,
    onArtistSortChange: (BridgeArtistSortCriterion) -> Unit,
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
            artistSortCriterion = artistSortCriterion,
            onArtistSortChange = onArtistSortChange,
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

@Preview(showBackground = true)
@Composable
private fun LibraryBrowserChromePreview() {
    BaeTheme {
        LibraryBrowserChrome(
            searchOpen = false,
            searchQuery = "",
            onSearchQueryChange = {},
            onOpenSearch = {},
            onCloseSearch = {},
            mode = LibraryBrowserMode.ALBUMS,
            onModeChange = {},
            sortCriterion = DEFAULT_ALBUM_SORT,
            onSortChange = {},
            composerSortCriterion = DEFAULT_COMPOSER_SORT,
            onComposerSortChange = {},
            artistSortCriterion = DEFAULT_ARTIST_SORT,
            onArtistSortChange = {},
            syncing = false,
            onShuffleLibrary = {},
            onSettings = {},
        )
    }
}
