package fm.bae.app.ui

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyGridState
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Shuffle
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField

private const val TAG = "bae.LibraryScreen"
private val logger = BaeLogger(TAG)
private const val PAGE_SIZE = 60
private const val GRID_PREFETCH_AHEAD = 12
private const val PULL_REFRESH_SETTLE_MS = 900L

private class PageError(
    val message: String,
    val onRetry: () -> Unit,
)

/**
 * Loads and accumulates the album grid one page at a time. Owns the loaded
 * albums and their display order, plus the two loads: the first page, and each
 * next page as the grid scrolls near its end. Backed by snapshot state so reads
 * in composition recompose on change. (Each album carries its own cover
 * reference; the grid cards fetch the bytes by id.)
 */
private class LibraryPage(
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
private fun rememberLibraryPage(
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

@Composable
fun LibraryScreen(
    session: OpenLibrary,
    onLeaveLibrary: () -> Unit,
) {
    var selectedAlbumId by remember { mutableStateOf<String?>(null) }
    var showSettings by remember { mutableStateOf(false) }
    var showMembers by remember { mutableStateOf(false) }
    val selected = selectedAlbumId
    when {
        selected != null -> {
            AlbumDetailScreen(session = session, albumId = selected, onBack = { selectedAlbumId = null })
        }

        showMembers -> {
            MembersScreen(session = session, onBack = { showMembers = false })
        }

        showSettings -> {
            SettingsScreen(
                session = session,
                onBack = { showSettings = false },
                onManageDevices = { showMembers = true },
                onLeaveLibrary = onLeaveLibrary,
            )
        }

        else -> {
            LibraryBrowser(
                session = session,
                onSelectAlbum = { selectedAlbumId = it },
                onSettings = { showSettings = true },
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LibraryBrowser(
    session: OpenLibrary,
    onSelectAlbum: (String) -> Unit,
    onSettings: () -> Unit,
) {
    var searchQuery by remember { mutableStateOf("") }
    var searchOpen by remember { mutableStateOf(false) }
    var sortCriterion by
        remember { mutableStateOf(BridgeSortCriterion(BridgeSortField.DATE_ADDED, BridgeSortDirection.DESCENDING)) }
    val generation by session.libraryStore.generation.collectAsState()
    val syncing by session.configStore.syncing.collectAsState()
    val syncError by session.configStore.syncError.collectAsState()
    val appError by session.configStore.error.collectAsState()
    val gridState = rememberLazyGridState()
    val page = rememberLibraryPage(session, generation, sortCriterion, LocalContext.current, gridState)

    Column(modifier = Modifier.fillMaxSize()) {
        if (searchOpen) {
            LibrarySearchBar(
                query = searchQuery,
                onQueryChange = { searchQuery = it },
                onClose = {
                    searchOpen = false
                    searchQuery = ""
                },
            )
        } else {
            LibraryTopBar(
                onOpenSearch = { searchOpen = true },
                onShuffleLibrary = {
                    try {
                        session.appHandle.playLibraryShuffled()
                    } catch (e: Exception) {
                        logger.error("playLibraryShuffled failed", e)
                    }
                },
                sortCriterion = sortCriterion,
                onSortChange = { sortCriterion = it },
                onSettings = onSettings,
            )
        }
        if (syncing) {
            LinearProgressIndicator(
                modifier = Modifier.fillMaxWidth(),
                color = MaterialTheme.colorScheme.primary,
                trackColor = MaterialTheme.colorScheme.surface,
            )
        }
        LibraryErrorBanner(page = page, appError = appError, syncError = syncError, session = session)
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            if (searchQuery.isNotBlank()) {
                SearchResultsScreen(session = session, query = searchQuery, onSelectAlbum = onSelectAlbum)
            } else {
                LibraryGridContent(
                    session = session,
                    page = page,
                    gridState = gridState,
                    onSelectAlbum = onSelectAlbum,
                )
            }
        }
        NowPlayingBar(session = session)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LibraryGridContent(
    session: OpenLibrary,
    page: LibraryPage,
    gridState: LazyGridState,
    onSelectAlbum: (String) -> Unit,
) {
    var refreshing by remember { mutableStateOf(false) }
    val refreshScope = rememberCoroutineScope()
    val onRefresh: () -> Unit = {
        session.appHandle.triggerSync()
        refreshScope.launch {
            refreshing = true
            delay(PULL_REFRESH_SETTLE_MS)
            refreshing = false
        }
    }
    val pageError = page.error
    PullToRefreshBox(isRefreshing = refreshing, onRefresh = onRefresh, modifier = Modifier.fillMaxSize()) {
        when {
            pageError != null && page.order.isEmpty() -> {
                Column(
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(text = pageError.message, color = MaterialTheme.colorScheme.error)
                    TextButton(onClick = pageError.onRetry) { Text(stringResource(R.string.retry)) }
                }
            }

            page.loading && page.order.isEmpty() -> {
                CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
            }

            page.totalCount == 0 -> {
                Text(
                    text = stringResource(R.string.library_empty_syncing),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                )
            }

            else -> {
                LazyVerticalGrid(
                    columns = GridCells.Adaptive(minSize = 150.dp),
                    state = gridState,
                    contentPadding = PaddingValues(12.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier.fillMaxSize(),
                ) {
                    items(page.order, key = { it }) { albumId ->
                        val album = page.albums[albumId] ?: return@items
                        AlbumGridCard(
                            album = album,
                            loadImage = session.library::imageBytes,
                            onClick = { onSelectAlbum(albumId) },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun LibraryErrorBanner(
    page: LibraryPage,
    appError: String?,
    syncError: String?,
    session: OpenLibrary,
) {
    val appendError = if (page.order.isNotEmpty()) page.error else null
    val banner = appendError?.message ?: appError ?: syncError ?: return
    ErrorBanner(
        message = banner,
        onRetry =
            when {
                appendError != null -> appendError.onRetry
                appError == null && syncError != null -> ({ session.appHandle.triggerSync() })
                else -> null
            },
    )
}

@Composable
private fun LibraryTopBar(
    onOpenSearch: () -> Unit,
    onShuffleLibrary: () -> Unit,
    sortCriterion: BridgeSortCriterion,
    onSortChange: (BridgeSortCriterion) -> Unit,
    onSettings: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = "bae", fontSize = 24.sp, fontWeight = FontWeight.Bold)
            Spacer(modifier = Modifier.weight(1f))
            IconButton(onClick = onOpenSearch) {
                Icon(imageVector = Icons.Filled.Search, contentDescription = stringResource(R.string.search))
            }
            IconButton(onClick = onShuffleLibrary) {
                Icon(
                    imageVector = Icons.Filled.Shuffle,
                    contentDescription = stringResource(R.string.shuffle_library),
                )
            }
            SortMenu(criterion = sortCriterion, onChange = onSortChange)
            IconButton(onClick = onSettings) {
                Icon(imageVector = Icons.Filled.Settings, contentDescription = stringResource(R.string.settings))
            }
        }
    }
}

@Composable
private fun LibrarySearchBar(
    query: String,
    onQueryChange: (String) -> Unit,
    onClose: () -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) { focusRequester.requestFocus() }
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onClose) {
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                    contentDescription = stringResource(R.string.close_search),
                )
            }
            TextField(
                value = query,
                onValueChange = onQueryChange,
                modifier = Modifier.weight(1f).focusRequester(focusRequester),
                placeholder = { Text(stringResource(R.string.search_placeholder)) },
                singleLine = true,
                colors =
                    TextFieldDefaults.colors(
                        focusedContainerColor = Color.Transparent,
                        unfocusedContainerColor = Color.Transparent,
                        focusedIndicatorColor = Color.Transparent,
                        unfocusedIndicatorColor = Color.Transparent,
                    ),
                trailingIcon = {
                    if (query.isNotEmpty()) {
                        IconButton(onClick = { onQueryChange("") }) {
                            Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.clear_search))
                        }
                    }
                },
            )
        }
    }
}

private val SORT_FIELDS =
    listOf(
        BridgeSortField.TITLE,
        BridgeSortField.ARTIST,
        BridgeSortField.YEAR,
        BridgeSortField.DATE_ADDED,
    )

@Composable
private fun SortMenu(
    criterion: BridgeSortCriterion,
    onChange: (BridgeSortCriterion) -> Unit,
) {
    fun BridgeSortField.labelRes(): Int =
        when (this) {
            BridgeSortField.TITLE -> R.string.sort_title
            BridgeSortField.ARTIST -> R.string.sort_artist
            BridgeSortField.YEAR -> R.string.sort_year
            BridgeSortField.DATE_ADDED -> R.string.sort_date_added
        }
    var expanded by remember { mutableStateOf(false) }
    val ascending = criterion.direction == BridgeSortDirection.ASCENDING
    Box {
        IconButton(onClick = { expanded = true }) {
            Icon(Icons.AutoMirrored.Filled.Sort, contentDescription = stringResource(R.string.sort))
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            SORT_FIELDS.forEach { field ->
                DropdownMenuItem(
                    text = { Text(stringResource(field.labelRes())) },
                    onClick = {
                        onChange(BridgeSortCriterion(field, criterion.direction))
                        expanded = false
                    },
                    leadingIcon = {
                        Icon(
                            imageVector = Icons.Filled.Check,
                            contentDescription = null,
                            modifier = Modifier.alpha(if (field == criterion.field) 1f else 0f),
                        )
                    },
                )
            }
            HorizontalDivider()
            DropdownMenuItem(
                text = {
                    Text(stringResource(if (ascending) R.string.sort_ascending else R.string.sort_descending))
                },
                onClick = {
                    val toggled = if (ascending) BridgeSortDirection.DESCENDING else BridgeSortDirection.ASCENDING
                    onChange(BridgeSortCriterion(criterion.field, toggled))
                    expanded = false
                },
                leadingIcon = {
                    val icon = if (ascending) Icons.Filled.ArrowUpward else Icons.Filled.ArrowDownward
                    Icon(imageVector = icon, contentDescription = null)
                },
            )
        }
    }
}

@Composable
private fun ErrorBanner(
    message: String,
    onRetry: (() -> Unit)? = null,
) {
    Surface(color = MaterialTheme.colorScheme.errorContainer, modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = message,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onErrorContainer,
                modifier = Modifier.weight(1f),
            )
            if (onRetry != null) {
                TextButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
            }
        }
    }
}

@Composable
private fun AlbumGridCard(
    album: BridgeAlbum,
    loadImage: suspend (imageId: String) -> ByteArray?,
    onClick: () -> Unit,
) {
    Column(modifier = Modifier.clickable(onClick = onClick)) {
        CoverImage(
            coverId = album.cover?.id,
            coverVersion = album.cover?.version,
            loadImage = loadImage,
            cornerRadius = 6.dp,
            iconPadding = 40.dp,
            modifier = Modifier.fillMaxWidth().aspectRatio(1f),
            contentDescription = album.title,
        )
        Spacer(modifier = Modifier.height(6.dp))
        Text(
            text = album.title,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
        )
        Text(
            text = album.artistNames,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
    }
}
