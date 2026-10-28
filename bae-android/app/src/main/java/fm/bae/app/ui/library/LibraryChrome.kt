package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
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
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.data.AlbumPageStore
import fm.bae.app.data.SyncActivityStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.sync.SyncActivityButton
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSortField
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField
import uniffi.bae_bridge.BridgeUploadTransfer

/** A later page of the album grid that failed to load, above the rows that did, with Retry. */
@Composable
internal fun LibraryErrorBanner(page: AlbumPageStore) {
    val appendError = (if (page.rows.isNotEmpty()) page.error else null) ?: return
    ErrorBanner(error = appendError.error, onRetry = appendError.onRetry)
}

@Composable
internal fun LibraryTopBar(
    onOpenSearch: () -> Unit,
    onShuffleLibrary: () -> Unit,
    mode: LibraryBrowserMode,
    sortCriterion: BridgeSortCriterion,
    onSortChange: (BridgeSortCriterion) -> Unit,
    groupAlbumsByArtist: Boolean,
    onGroupAlbumsByArtistChange: (Boolean) -> Unit,
    composerSortCriterion: BridgeComposerSortCriterion,
    onComposerSortChange: (BridgeComposerSortCriterion) -> Unit,
    artistSortCriterion: BridgeArtistSortCriterion,
    onArtistSortChange: (BridgeArtistSortCriterion) -> Unit,
    syncActivity: SyncActivityStore,
    uploadTransfer: LatestSource<BridgeUploadTransfer>,
    onOpenSyncSettings: () -> Unit,
    onOpenDownloads: () -> Unit,
    onSettings: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = stringResource(R.string.library), style = ThemeText.title.style)
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
            when (mode) {
                LibraryBrowserMode.ALBUMS -> {
                    SortMenu(
                        criterion = sortCriterion,
                        onChange = onSortChange,
                        groupByArtist = groupAlbumsByArtist,
                        onGroupByArtistChange = onGroupAlbumsByArtistChange,
                    )
                }

                LibraryBrowserMode.COMPOSERS -> {
                    ComposerSortMenu(
                        criterion = composerSortCriterion,
                        onChange = onComposerSortChange,
                    )
                }

                LibraryBrowserMode.ARTISTS -> {
                    ArtistSortMenu(
                        criterion = artistSortCriterion,
                        onChange = onArtistSortChange,
                    )
                }
            }
            SyncActivityButton(
                store = syncActivity,
                uploadTransfer = uploadTransfer,
                onOpenSettings = onOpenSyncSettings,
                onOpenDownloads = onOpenDownloads,
            )
            IconButton(onClick = onSettings) {
                Icon(imageVector = Icons.Filled.Settings, contentDescription = stringResource(R.string.settings))
            }
        }
    }
}

/** A screen inside the library: its bar holds the one way back, labeled with where it goes. */
@Composable
internal fun LibraryDetailTopBar(onBack: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // One button: the arrow and the label read together as "Back, Library".
            TextButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
                Spacer(modifier = Modifier.width(ThemeSpace.inline))
                Text(text = stringResource(R.string.library))
            }
        }
    }
}

@Composable
internal fun LibraryModeBar(
    mode: LibraryBrowserMode,
    onModeChange: (LibraryBrowserMode) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        TextButton(onClick = { onModeChange(LibraryBrowserMode.ALBUMS) }) {
            Text(
                text = stringResource(R.string.library_mode_albums),
                style = if (mode == LibraryBrowserMode.ALBUMS) ThemeText.strong.style else ThemeText.body.style,
            )
        }
        TextButton(onClick = { onModeChange(LibraryBrowserMode.COMPOSERS) }) {
            Text(
                text = stringResource(R.string.library_mode_composers),
                style = if (mode == LibraryBrowserMode.COMPOSERS) ThemeText.strong.style else ThemeText.body.style,
            )
        }
        TextButton(onClick = { onModeChange(LibraryBrowserMode.ARTISTS) }) {
            Text(
                text = stringResource(R.string.library_mode_artists),
                style = if (mode == LibraryBrowserMode.ARTISTS) ThemeText.strong.style else ThemeText.body.style,
            )
        }
    }
}

@Composable
internal fun LibrarySearchBar(
    query: String,
    onQueryChange: (String) -> Unit,
    onClose: () -> Unit,
) {
    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(Unit) { focusRequester.requestFocus() }
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
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
                        focusedContainerColor = BaeTheme.surfaces.well,
                        unfocusedContainerColor = BaeTheme.surfaces.well,
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
    groupByArtist: Boolean,
    onGroupByArtistChange: (Boolean) -> Unit,
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
            DropdownMenuItem(
                text = { Text(stringResource(R.string.group_by_artist)) },
                onClick = {
                    onGroupByArtistChange(!groupByArtist)
                    expanded = false
                },
                leadingIcon = {
                    Icon(
                        imageVector = Icons.Filled.Check,
                        contentDescription = null,
                        modifier = Modifier.alpha(if (groupByArtist) 1f else 0f),
                    )
                },
            )
            HorizontalDivider()
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

private val previewAlbumSort = BridgeSortCriterion(BridgeSortField.TITLE, BridgeSortDirection.ASCENDING)
private val previewComposerSort =
    BridgeComposerSortCriterion(BridgeComposerSortField.NAME, BridgeSortDirection.ASCENDING)
private val previewArtistSort = BridgeArtistSortCriterion(BridgeArtistSortField.NAME, BridgeSortDirection.ASCENDING)

@Preview(showBackground = true)
@Composable
private fun LibraryTopBarPreview() {
    BaeTheme {
        LibraryTopBar(
            onOpenSearch = {},
            onShuffleLibrary = {},
            mode = LibraryBrowserMode.ALBUMS,
            sortCriterion = previewAlbumSort,
            onSortChange = {},
            groupAlbumsByArtist = false,
            onGroupAlbumsByArtistChange = {},
            composerSortCriterion = previewComposerSort,
            onComposerSortChange = {},
            artistSortCriterion = previewArtistSort,
            onArtistSortChange = {},
            syncActivity = SyncActivityStore({}, {}).apply { apply(PreviewData.syncActivity(needsAttention = true)) },
            uploadTransfer = PreviewData.uploadTransferSource(),
            onOpenSyncSettings = {},
            onOpenDownloads = {},
            onSettings = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun LibraryDetailTopBarPreview() {
    BaeTheme {
        LibraryDetailTopBar(onBack = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun LibraryModeBarPreview() {
    BaeTheme {
        LibraryModeBar(mode = LibraryBrowserMode.ALBUMS, onModeChange = {})
    }
}

@Preview(showBackground = true)
@Composable
private fun LibrarySearchBarPreview() {
    BaeTheme {
        LibrarySearchBar(query = "search text", onQueryChange = {}, onClose = {})
    }
}
