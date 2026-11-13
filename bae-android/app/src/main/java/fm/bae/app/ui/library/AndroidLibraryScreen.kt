package fm.bae.app.ui.library

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import fm.bae.app.OpenLibrary
import fm.bae.app.ui.ActivityProblemId
import fm.bae.app.ui.AppProblem
import fm.bae.app.ui.playback.AndroidNowPlayingBarContainer
import fm.bae.app.ui.sync.ActivityButton
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSortField
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField

private val DEFAULT_ALBUM_SORT =
    BridgeSortCriterion(BridgeSortField.DATE_ADDED, BridgeSortDirection.DESCENDING)
private val DEFAULT_COMPOSER_SORT =
    BridgeComposerSortCriterion(BridgeComposerSortField.NAME, BridgeSortDirection.ASCENDING)
private val DEFAULT_ARTIST_SORT =
    BridgeArtistSortCriterion(BridgeArtistSortField.NAME, BridgeSortDirection.ASCENDING)

/**
 * Browser chrome state: the view, its sort, search, which header menu is open,
 * and each view's scroll. Owned above the stack in [LibraryScreen] so they
 * survive while a pushed destination hides the browser.
 */
internal class LibraryBrowserState {
    var searchOpen by mutableStateOf(false)
    var searchQuery by mutableStateOf("")
    var mode by mutableStateOf(LibraryBrowserMode.ALBUMS)
    var openMenu by mutableStateOf<LibraryHeaderMenu?>(null)
    var sortCriterion by mutableStateOf(DEFAULT_ALBUM_SORT)
    var groupAlbumsByArtist by mutableStateOf(false)
    var composerSortCriterion by mutableStateOf(DEFAULT_COMPOSER_SORT)
    var artistSortCriterion by mutableStateOf(DEFAULT_ARTIST_SORT)

    /** The problems the person took out of the Activity sheet; they show again if they leave the list and return. */
    var activityDismissed by mutableStateOf(emptySet<ActivityProblemId>())
    val gridState = LazyGridState()
    val artistListState = LazyListState()
    val composerListState = LazyListState()
    val playlistListState = LazyListState()

    fun closeSearch() {
        searchOpen = false
        searchQuery = ""
    }
}

/** The library's screen reading [session]: the browser [state] over what the session holds. */
@Composable
internal fun AndroidLibraryScreenContainer(
    session: OpenLibrary,
    state: LibraryBrowserState,
    launchProblems: List<AppProblem>,
    onSelectAlbum: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
    onSelectWork: (String) -> Unit,
    onSettings: () -> Unit,
    onDownloads: () -> Unit,
    onSyncStatus: () -> Unit,
) {
    AndroidLibraryScreen(
        state = state,
        onShuffleLibrary = {
            session.messages.play("play the library shuffled") { session.appHandle.playLibraryShuffled() }
        },
        onSettings = onSettings,
        syncActivity = {
            ActivityButton(
                store = session.syncActivityStore,
                app = launchProblems,
                dismissed = state.activityDismissed,
                onDismissProblem = { state.activityDismissed += it },
                onOpenSettings = onSyncStatus,
                onOpenDownloads = onDownloads,
                onSetDownloadsPaused = { paused ->
                    session.messages.perform("pause or resume downloads") {
                        session.appHandle.setDownloadsPaused(paused)
                    }
                },
            )
        },
        nowPlayingBar = { AndroidNowPlayingBarContainer(session = session) },
    ) { modifier ->
        LibraryBrowserContent(
            modifier = modifier,
            session = session,
            state = state,
            onSelectAlbum = onSelectAlbum,
            onSelectComposer = onSelectComposer,
            onSelectArtist = onSelectArtist,
            onSelectWork = onSelectWork,
        )
    }
}

/**
 * The library's screen: its header, or the search bar when search is open, over the [content] (given
 * the room that is left), then the [nowPlayingBar]. [syncActivity] fills the header's place for the
 * Activity button.
 */
@Composable
internal fun AndroidLibraryScreen(
    state: LibraryBrowserState,
    onShuffleLibrary: () -> Unit,
    onSettings: () -> Unit,
    syncActivity: @Composable () -> Unit,
    nowPlayingBar: @Composable () -> Unit,
    content: @Composable (Modifier) -> Unit,
) {
    BackHandler(enabled = state.searchOpen) { state.closeSearch() }
    Column(modifier = Modifier.fillMaxSize()) {
        if (state.searchOpen) {
            LibrarySearchBar(
                query = state.searchQuery,
                onQueryChange = { state.searchQuery = it },
                onClose = state::closeSearch,
            )
        } else {
            val collapse = rememberHeadingCollapse(state)
            LibraryHeader(
                state = state,
                collapse = { collapse.value },
                onShuffleLibrary = onShuffleLibrary,
                onSettings = onSettings,
                syncActivity = syncActivity,
            )
        }
        content(Modifier.fillMaxWidth().weight(1f))
        nowPlayingBar()
    }
}

/** The heading's collapse, following the scroll of the view on screen. */
@Composable
private fun rememberHeadingCollapse(state: LibraryBrowserState): State<Float> =
    when (state.mode) {
        LibraryBrowserMode.ALBUMS -> rememberHeadingCollapse(state.gridState)
        LibraryBrowserMode.ARTISTS -> rememberHeadingCollapse(state.artistListState)
        LibraryBrowserMode.COMPOSERS -> rememberHeadingCollapse(state.composerListState)
        LibraryBrowserMode.PLAYLISTS -> rememberHeadingCollapse(state.playlistListState)
    }

@Composable
private fun LibraryBrowserContent(
    modifier: Modifier,
    session: OpenLibrary,
    state: LibraryBrowserState,
    onSelectAlbum: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectArtist: (String) -> Unit,
    onSelectWork: (String) -> Unit,
) {
    Box(modifier = modifier) {
        if (state.searchQuery.isNotBlank()) {
            SearchResultsScreen(
                session = session,
                query = state.searchQuery,
                onSelectAlbum = onSelectAlbum,
                onSelectArtist = onSelectArtist,
                onSelectComposer = onSelectComposer,
                onSelectWork = onSelectWork,
            )
        } else {
            when (state.mode) {
                LibraryBrowserMode.ALBUMS -> {
                    AlbumBrowserContent(
                        session = session,
                        sortCriterion = state.sortCriterion,
                        groupByArtist = state.groupAlbumsByArtist,
                        gridState = state.gridState,
                        onSelectAlbum = onSelectAlbum,
                        onSelectArtist = onSelectArtist,
                    )
                }

                LibraryBrowserMode.ARTISTS -> {
                    ArtistBrowserContent(
                        session = session,
                        sortCriterion = state.artistSortCriterion,
                        listState = state.artistListState,
                        onSelectArtist = onSelectArtist,
                    )
                }

                LibraryBrowserMode.COMPOSERS -> {
                    ComposerBrowserContent(
                        session = session,
                        sortCriterion = state.composerSortCriterion,
                        listState = state.composerListState,
                        onSelectComposer = onSelectComposer,
                    )
                }

                LibraryBrowserMode.PLAYLISTS -> {
                    PlaylistListContent(
                        session = session,
                        page = rememberPlaylistListing(session),
                        listState = state.playlistListState,
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
    onSelectArtist: (String) -> Unit,
) {
    val page =
        rememberLibraryPage(
            session = session,
            sortCriterion = sortCriterion,
            groupByArtist = groupByArtist,
            gridState = gridState,
        )
    Column(modifier = Modifier.fillMaxSize()) {
        AndroidErrorBannerContainer(page = page)
        AndroidAlbumGridContainer(
            session = session,
            page = page,
            gridState = gridState,
            groupByArtist = groupByArtist,
            onSelectAlbum = onSelectAlbum,
            onSelectArtist = onSelectArtist,
        )
    }
}

@Composable
private fun ComposerBrowserContent(
    session: OpenLibrary,
    sortCriterion: BridgeComposerSortCriterion,
    listState: LazyListState,
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
        listState = listState,
        onSelectComposer = onSelectComposer,
    )
}

@Composable
private fun ArtistBrowserContent(
    session: OpenLibrary,
    sortCriterion: BridgeArtistSortCriterion,
    listState: LazyListState,
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
        listState = listState,
        onSelectArtist = onSelectArtist,
    )
}
