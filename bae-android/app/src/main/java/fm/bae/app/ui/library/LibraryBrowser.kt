package fm.bae.app.ui.library

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.OpenLibrary
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.playback.NowPlayingBar
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSortField
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField
import uniffi.bae_bridge.BridgeUploadTransfer

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
    var groupAlbumsByArtist by mutableStateOf(false)
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
    onSyncStatus: () -> Unit,
) {
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
            groupAlbumsByArtist = state.groupAlbumsByArtist,
            onGroupAlbumsByArtistChange = { state.groupAlbumsByArtist = it },
            composerSortCriterion = state.composerSortCriterion,
            onComposerSortChange = { state.composerSortCriterion = it },
            artistSortCriterion = state.artistSortCriterion,
            onArtistSortChange = { state.artistSortCriterion = it },
            onShuffleLibrary = {
                session.messages.perform("play the library shuffled") { session.appHandle.playLibraryShuffled() }
            },
            syncActivity = session.syncActivityStore,
            uploadTransfer = session.outboxStore.totalTransfer,
            onOpenSyncSettings = onSyncStatus,
            onOpenDownloads = onDownloads,
            onSettings = onSettings,
        )
        LibraryBrowserContent(
            modifier = Modifier.fillMaxWidth().weight(1f),
            session = session,
            searchQuery = state.searchQuery,
            mode = state.mode,
            sortCriterion = state.sortCriterion,
            groupAlbumsByArtist = state.groupAlbumsByArtist,
            composerSortCriterion = state.composerSortCriterion,
            artistSortCriterion = state.artistSortCriterion,
            gridState = state.gridState,
            onSelectAlbum = onSelectAlbum,
            onSelectComposer = onSelectComposer,
            onSelectArtist = onSelectArtist,
            onSelectWork = onSelectWork,
        )
        NowPlayingBar(session = session)
    }
}

@Composable
private fun LibraryBrowserContent(
    modifier: Modifier,
    session: OpenLibrary,
    searchQuery: String,
    mode: LibraryBrowserMode,
    sortCriterion: BridgeSortCriterion,
    groupAlbumsByArtist: Boolean,
    composerSortCriterion: BridgeComposerSortCriterion,
    artistSortCriterion: BridgeArtistSortCriterion,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
    onSelectWork: (String) -> Unit,
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
                        sortCriterion = sortCriterion,
                        groupByArtist = groupAlbumsByArtist,
                        gridState = gridState,
                        onSelectAlbum = onSelectAlbum,
                    )
                }

                LibraryBrowserMode.COMPOSERS -> {
                    ComposerBrowserContent(
                        session = session,
                        sortCriterion = composerSortCriterion,
                        onSelectComposer = onSelectComposer,
                    )
                }

                LibraryBrowserMode.ARTISTS -> {
                    ArtistBrowserContent(
                        session = session,
                        sortCriterion = artistSortCriterion,
                        onSelectArtist = onSelectArtist,
                    )
                }
            }
        }
    }
}

@Composable
private fun AlbumBrowserContent(
    session: OpenLibrary,
    sortCriterion: BridgeSortCriterion,
    groupByArtist: Boolean,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
) {
    val page =
        rememberLibraryPage(
            session = session,
            sortCriterion = sortCriterion,
            groupByArtist = groupByArtist,
            gridState = gridState,
        )
    Column(modifier = Modifier.fillMaxSize()) {
        LibraryErrorBanner(page = page)
        LibraryGridContent(
            session = session,
            page = page,
            gridState = gridState,
            groupByArtist = groupByArtist,
            onSelectAlbum = onSelectAlbum,
        )
    }
}

@Composable
private fun ComposerBrowserContent(
    session: OpenLibrary,
    sortCriterion: BridgeComposerSortCriterion,
    onSelectComposer: (String) -> Unit,
) {
    val page =
        rememberComposerPage(
            session = session,
            sortCriterion = sortCriterion,
        )
    ComposerListContent(
        session = session,
        page = page,
        onSelectComposer = onSelectComposer,
    )
}

@Composable
private fun ArtistBrowserContent(
    session: OpenLibrary,
    sortCriterion: BridgeArtistSortCriterion,
    onSelectArtist: (String) -> Unit,
) {
    val page =
        rememberArtistPage(
            session = session,
            sortCriterion = sortCriterion,
        )
    ArtistListContent(
        session = session,
        page = page,
        onSelectArtist = onSelectArtist,
    )
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
    groupAlbumsByArtist: Boolean,
    onGroupAlbumsByArtistChange: (Boolean) -> Unit,
    composerSortCriterion: BridgeComposerSortCriterion,
    onComposerSortChange: (BridgeComposerSortCriterion) -> Unit,
    artistSortCriterion: BridgeArtistSortCriterion,
    onArtistSortChange: (BridgeArtistSortCriterion) -> Unit,
    onShuffleLibrary: () -> Unit,
    syncActivity: SyncActivityStore,
    uploadTransfer: StateFlow<BridgeUploadTransfer?>,
    onOpenSyncSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
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
            groupAlbumsByArtist = groupAlbumsByArtist,
            onGroupAlbumsByArtistChange = onGroupAlbumsByArtistChange,
            composerSortCriterion = composerSortCriterion,
            onComposerSortChange = onComposerSortChange,
            artistSortCriterion = artistSortCriterion,
            onArtistSortChange = onArtistSortChange,
            syncActivity = syncActivity,
            uploadTransfer = uploadTransfer,
            onOpenSyncSettings = onOpenSyncSettings,
            onOpenDownloads = onOpenDownloads,
            onSettings = onSettings,
        )
    }
    LibraryModeBar(mode = mode, onModeChange = onModeChange)
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
            groupAlbumsByArtist = false,
            onGroupAlbumsByArtistChange = {},
            composerSortCriterion = DEFAULT_COMPOSER_SORT,
            onComposerSortChange = {},
            artistSortCriterion = DEFAULT_ARTIST_SORT,
            onArtistSortChange = {},
            onShuffleLibrary = {},
            syncActivity = SyncActivityStore({}, {}).apply { apply(PreviewData.syncActivity(needsAttention = false)) },
            uploadTransfer = MutableStateFlow(PreviewData.uploadTransfer()),
            onOpenSyncSettings = {},
            onOpenDownloads = {},
            onSettings = {},
        )
    }
}
