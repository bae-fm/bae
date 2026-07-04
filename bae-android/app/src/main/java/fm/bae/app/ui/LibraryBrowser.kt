package fm.bae.app.ui

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.DownloadStore
import fm.bae.app.formatFileSize
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadState
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
    val appContext = LocalContext.current

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
            onShuffleLibrary = { session.playLibraryShuffledOrReport(appContext) },
            onSettings = onSettings,
        )
        DownloadProgressStrip(session.downloadStore)
        LibraryBrowserContent(
            modifier = Modifier.fillMaxWidth().weight(1f),
            session = session,
            searchQuery = searchQuery,
            mode = mode,
            generation = generation,
            composerGeneration = composerGeneration,
            sortCriterion = sortCriterion,
            composerSortCriterion = composerSortCriterion,
            appError = appError,
            syncError = syncError,
            gridState = gridState,
            onSelectAlbum = onSelectAlbum,
            onSelectComposer = onSelectComposer,
            onSelectWork = onSelectWork,
            appContext = appContext,
        )
        NowPlayingBar(session = session)
    }
}

@Composable
private fun DownloadProgressStrip(downloadStore: DownloadStore) {
    val snapshot by downloadStore.snapshot.collectAsState()
    Column(modifier = Modifier.fillMaxWidth()) {
        for (download in snapshot.downloads) {
            DownloadProgressRow(download)
        }
    }
}

@Composable
private fun DownloadProgressRow(download: BridgeDownloadOp) {
    val context = LocalContext.current
    val progress =
        when (val state = download.state) {
            is BridgeDownloadState.Active -> {
                state.progress
            }

            is BridgeDownloadState.Failed -> {
                Text(
                    text = state.error,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
                return
            }

            else -> {
                return
            }
        }
    Column {
        LinearProgressIndicator(
            progress = { progress.fraction.toFloat() },
            modifier = Modifier.fillMaxWidth(),
        )
        Text(
            text =
                context.coreString(
                    "core.download.bytes_progress",
                    mapOf(
                        "done" to progress.bytesDone.formatDownloadBytes(context),
                        "total" to progress.bytesTotal.formatDownloadBytes(context),
                    ),
                ),
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun ULong.formatDownloadBytes(context: Context): String {
    val byteCount = requireDownloadByteCountInDisplayRange()
    return context.formatFileSize(byteCount)
}

private fun ULong.requireDownloadByteCountInDisplayRange(): Long {
    require(this <= Long.MAX_VALUE.toULong()) {
        "download byte count exceeds display range"
    }
    return toLong()
}

private fun OpenLibrary.playLibraryShuffledOrReport(appContext: Context) {
    try {
        appHandle.playLibraryShuffled()
    } catch (e: Exception) {
        logger.error("playLibraryShuffled failed", e)
        configStore.showError(appContext.getString(R.string.library_load_failed))
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
    sortCriterion: BridgeSortCriterion,
    composerSortCriterion: BridgeComposerSortCriterion,
    appError: String?,
    syncError: String?,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
    onSelectComposer: (String) -> Unit,
    onSelectWork: (String) -> Unit,
    appContext: Context,
) {
    Box(modifier = modifier) {
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
            loadImage = session.library::imageBytes,
            onSelectComposer = onSelectComposer,
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
