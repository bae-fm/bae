package fm.bae.app.ui

import android.util.Log
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
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

private const val PAGE_SIZE = 60

/** The display-label resource for a sort field, shown in the library sort menu. */
private fun BridgeSortField.labelRes(): Int =
    when (this) {
        BridgeSortField.TITLE -> R.string.sort_title
        BridgeSortField.ARTIST -> R.string.sort_artist
        BridgeSortField.YEAR -> R.string.sort_year
        BridgeSortField.DATE_ADDED -> R.string.sort_date_added
    }

/**
 * Library browse. A top bar (wordmark + search/sort/settings) over an album
 * grid paginated from the database, navigation into an album's detail, and a
 * persistent now-playing bar once playback starts. Search is tap-to-open: the
 * search icon swaps the top bar for a focused query field. A thin indeterminate
 * bar shows under the top bar only while a sync cycle is mid-flight. The grid
 * re-queries whenever the library shape changes (sync streams albums in over
 * time).
 */
private const val TAG = "bae.LibraryScreen"

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LibraryScreen(
    session: OpenLibrary,
    onLeaveLibrary: () -> Unit,
) {
    var selectedAlbumId by remember { mutableStateOf<String?>(null) }
    var showSettings by remember { mutableStateOf(false) }
    // Declared before the detail early-return so the active query (and whether
    // the search bar is open) survive a round-trip into an album's detail and
    // back.
    var searchQuery by remember { mutableStateOf("") }
    var searchOpen by remember { mutableStateOf(false) }
    // Survives the detail round-trip (declared before the early return). Changing
    // it resets the paged accumulator below, which keys on it.
    var sortCriterion by remember {
        mutableStateOf(
            BridgeSortCriterion(BridgeSortField.DATE_ADDED, BridgeSortDirection.DESCENDING),
        )
    }

    val selected = selectedAlbumId
    if (selected != null) {
        AlbumDetailScreen(
            session = session,
            albumId = selected,
            onBack = { selectedAlbumId = null },
        )
        return
    }

    if (showSettings) {
        SettingsScreen(
            session = session,
            onBack = { showSettings = false },
            onLeaveLibrary = onLeaveLibrary,
        )
        return
    }

    val generation by session.libraryStore.generation.collectAsState()
    val syncing by session.configStore.syncing.collectAsState()
    val syncError by session.configStore.syncError.collectAsState()
    val appError by session.configStore.error.collectAsState()
    // For error fallbacks set inside LaunchedEffects (stringResource needs a
    // composition; the load runs off it).
    val appContext = LocalContext.current

    // Bumped by a Retry after a failed load; included in the accumulator keys
    // below so a retry resets everything and reloads from the first page. Reset
    // to 0 whenever the generation or sort changes (those reload on their own).
    var retryToken by remember(generation, sortCriterion) { mutableStateOf(0) }
    // Single source of truth for the grid's rows: an id-keyed, insertion-ordered
    // accumulator. The DATE_ADDED-descending sort head-inserts as sync streams
    // albums in, so offset paging re-fetches albums already seen; keying by id
    // dedupes them so the grid never renders two cards with the same id. The
    // accumulator (and the count/offset) reset together on a generation bump or
    // a retry, keyed on all three so the reset and append paths share one owner
    // and can't race.
    val albums = remember(generation, sortCriterion, retryToken) { mutableStateMapOf<String, BridgeAlbum>() }
    val order = remember(generation, sortCriterion, retryToken) { mutableStateListOf<String>() }
    // Album id -> absolute cover path, resolved from the primary release's cover
    // file on disk. Resolved off-main as each page loads (see `resolveCovers`),
    // so every paged card carries its cover — not just albums whose detail a
    // live sync event happened to intern this session. Absent when the cover
    // file isn't on disk yet; a later generation bump re-resolves it.
    val coverPaths = remember(generation, sortCriterion, retryToken) { mutableStateMapOf<String, String>() }
    var totalCount by remember(generation, sortCriterion, retryToken) { mutableStateOf(0) }
    var loadedOffset by remember(generation, sortCriterion, retryToken) { mutableStateOf(0) }
    var loading by remember(generation, sortCriterion, retryToken) { mutableStateOf(true) }
    // Set when a page read throws. Drives the in-grid error+retry when the first
    // page failed (nothing loaded) and the top banner when an append failed over
    // already-loaded albums. Cleared by a generation/sort change or a retry.
    var loadError by remember(generation, sortCriterion, retryToken) { mutableStateOf<String?>(null) }

    val gridState = rememberLazyGridState()
    // Pull-to-refresh shows the indicator briefly to acknowledge the manual
    // sync kick; results stream back in via album events (sync runs on its own).
    var refreshing by remember { mutableStateOf(false) }
    val refreshScope = rememberCoroutineScope()

    fun ingest(
        page: List<BridgeAlbum>,
        covers: Map<String, String>,
    ) {
        page.forEach { album ->
            if (!albums.containsKey(album.id)) order.add(album.id)
            albums[album.id] = album
        }
        coverPaths.putAll(covers)
    }

    // Load the first page when the library shape changes (the accumulator was
    // just reset by the generation-keyed `remember`s above).
    LaunchedEffect(generation, sortCriterion, retryToken) {
        loading = true
        loadError = null
        try {
            val (count, page, covers) =
                withContext(Dispatchers.IO) {
                    val c = session.library.albumCount().toInt()
                    val p = session.library.albumPage(listOf(sortCriterion), 0u, PAGE_SIZE.toULong())
                    Triple(c, p, resolveCovers(session, p))
                }
            totalCount = count
            ingest(page, covers)
            loadedOffset = PAGE_SIZE
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.e(TAG, "Failed to load first album page", e)
            loadError = e.message ?: appContext.getString(R.string.library_load_failed)
        } finally {
            loading = false
        }
    }

    // Append the next page as the user nears the end.
    val shouldLoadMore by remember {
        derivedStateOf {
            val lastVisible =
                gridState.layoutInfo.visibleItemsInfo
                    .lastOrNull()
                    ?.index ?: 0
            order.size < totalCount && lastVisible >= order.size - 12
        }
    }
    LaunchedEffect(shouldLoadMore, totalCount, generation, sortCriterion) {
        if (shouldLoadMore && order.size < totalCount) {
            val offset = loadedOffset
            try {
                val (more, moreCovers) =
                    withContext(Dispatchers.IO) {
                        val p = session.library.albumPage(listOf(sortCriterion), offset.toULong(), PAGE_SIZE.toULong())
                        p to resolveCovers(session, p)
                    }
                ingest(more, moreCovers)
                loadedOffset = offset + PAGE_SIZE
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.e(TAG, "Failed to load album page at offset $offset", e)
                loadError = e.message ?: appContext.getString(R.string.library_load_more_failed)
            }
        }
    }

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
                sortCriterion = sortCriterion,
                onSortChange = { sortCriterion = it },
                onSettings = { showSettings = true },
            )
        }

        SyncIndicatorBar(syncing = syncing)

        // A failed append (over already-loaded albums) surfaces in this banner
        // with a Retry; a first-page failure shows in-grid below instead.
        val appendError = if (order.isNotEmpty()) loadError else null
        val banner = appendError ?: appError ?: syncError
        if (banner != null) {
            ErrorBanner(
                message = banner,
                // Retry the recoverable paths: a failed append reloads from the
                // first page; a sync error re-kicks sync. An app error isn't.
                onRetry =
                    when {
                        appendError != null -> {
                            { retryToken++ }
                        }

                        appError == null && syncError != null -> {
                            { session.appHandle.triggerSync() }
                        }

                        else -> {
                            null
                        }
                    },
            )
        }

        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            if (searchQuery.isNotBlank()) {
                SearchResultsScreen(
                    session = session,
                    query = searchQuery,
                    onSelectAlbum = { selectedAlbumId = it },
                )
            } else {
                PullToRefreshBox(
                    isRefreshing = refreshing,
                    onRefresh = {
                        session.appHandle.triggerSync()
                        refreshScope.launch {
                            refreshing = true
                            delay(900)
                            refreshing = false
                        }
                    },
                    modifier = Modifier.fillMaxSize(),
                ) {
                    when {
                        loadError != null && order.isEmpty() -> {
                            Column(
                                modifier = Modifier.align(Alignment.Center).padding(32.dp),
                                horizontalAlignment = Alignment.CenterHorizontally,
                            ) {
                                Text(
                                    text = loadError ?: "",
                                    color = MaterialTheme.colorScheme.error,
                                )
                                TextButton(onClick = { retryToken++ }) { Text(stringResource(R.string.retry)) }
                            }
                        }

                        loading && order.isEmpty() -> {
                            CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
                        }

                        totalCount == 0 -> {
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
                                contentPadding =
                                    androidx.compose.foundation.layout
                                        .PaddingValues(12.dp),
                                horizontalArrangement = Arrangement.spacedBy(12.dp),
                                verticalArrangement = Arrangement.spacedBy(12.dp),
                                modifier = Modifier.fillMaxSize(),
                            ) {
                                items(order, key = { it }) { albumId ->
                                    val album = albums[albumId] ?: return@items
                                    AlbumGridCard(
                                        album = album,
                                        coverPath = coverPaths[albumId],
                                        onClick = { selectedAlbumId = albumId },
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }

        NowPlayingBar(session = session)
    }
}

@Composable
private fun LibraryTopBar(
    onOpenSearch: () -> Unit,
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
            SortMenu(criterion = sortCriterion, onChange = onSortChange)
            IconButton(onClick = onSettings) {
                Icon(imageVector = Icons.Filled.Settings, contentDescription = stringResource(R.string.settings))
            }
        }
    }
}

/**
 * Top bar in search mode: a back affordance that closes search, a focused query
 * field (autofocused on open), and a clear button once there's a query. Swapped
 * in for [LibraryTopBar] while the search field is open.
 */
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

/**
 * Sync shows only when it's happening: a thin indeterminate bar while a sync
 * cycle is mid-flight, gone otherwise. Driven by the real `SyncingChanged`
 * signal (see [fm.bae.app.data.ConfigStore.syncing]); pull-to-refresh kicks a
 * cycle, which surfaces here.
 */
@Composable
private fun SyncIndicatorBar(syncing: Boolean) {
    if (syncing) {
        LinearProgressIndicator(
            modifier = Modifier.fillMaxWidth(),
            color = MaterialTheme.colorScheme.primary,
            trackColor = MaterialTheme.colorScheme.surface,
        )
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
                        // Keep the slot in the tree always; toggle the checkmark
                        // via alpha so rows don't shift between fields.
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
                text = { Text(stringResource(if (ascending) R.string.sort_ascending else R.string.sort_descending)) },
                onClick = {
                    val toggled =
                        if (ascending) {
                            BridgeSortDirection.DESCENDING
                        } else {
                            BridgeSortDirection.ASCENDING
                        }
                    onChange(BridgeSortCriterion(criterion.field, toggled))
                    expanded = false
                },
                leadingIcon = {
                    Icon(
                        imageVector =
                            if (ascending) {
                                Icons.Filled.ArrowUpward
                            } else {
                                Icons.Filled.ArrowDownward
                            },
                        contentDescription = null,
                    )
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

/**
 * Resolve each album's grid cover to the primary release's cover file on disk.
 * Mirrors the desktop grid, which resolves covers by release id through this
 * same bridge call rather than loading each album's full detail. Albums whose
 * cover file isn't on disk yet are absent from the map (card shows a
 * placeholder until a later page load / generation bump resolves it).
 */
private fun resolveCovers(
    session: OpenLibrary,
    page: List<BridgeAlbum>,
): Map<String, String> =
    page
        .mapNotNull { album ->
            session.library.imagePathIfExists(album.primaryReleaseId)?.let { album.id to it }
        }.toMap()

@Composable
private fun AlbumGridCard(
    album: BridgeAlbum,
    coverPath: String?,
    onClick: () -> Unit,
) {
    Column(modifier = Modifier.clickable(onClick = onClick)) {
        CoverImage(
            path = coverPath,
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
