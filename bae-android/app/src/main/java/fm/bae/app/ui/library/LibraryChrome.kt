package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField

@Composable
internal fun LibraryErrorBanner(
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
internal fun LibraryGlobalErrorBanner(
    appError: String?,
    syncError: String?,
    session: OpenLibrary,
) {
    val banner = appError ?: syncError ?: return
    ErrorBanner(
        message = banner,
        onRetry = if (appError == null && syncError != null) ({ session.appHandle.triggerSync() }) else null,
    )
}

@Composable
internal fun LibraryTopBar(
    onOpenSearch: () -> Unit,
    onShuffleLibrary: () -> Unit,
    mode: LibraryBrowserMode,
    sortCriterion: BridgeSortCriterion,
    onSortChange: (BridgeSortCriterion) -> Unit,
    composerSortCriterion: BridgeComposerSortCriterion,
    onComposerSortChange: (BridgeComposerSortCriterion) -> Unit,
    artistSortCriterion: BridgeArtistSortCriterion,
    onArtistSortChange: (BridgeArtistSortCriterion) -> Unit,
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
            when (mode) {
                LibraryBrowserMode.ALBUMS -> {
                    SortMenu(criterion = sortCriterion, onChange = onSortChange)
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
            IconButton(onClick = onSettings) {
                Icon(imageVector = Icons.Filled.Settings, contentDescription = stringResource(R.string.settings))
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
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        TextButton(onClick = { onModeChange(LibraryBrowserMode.ALBUMS) }) {
            Text(
                text = stringResource(R.string.library_mode_albums),
                fontWeight = if (mode == LibraryBrowserMode.ALBUMS) FontWeight.Bold else FontWeight.Normal,
            )
        }
        TextButton(onClick = { onModeChange(LibraryBrowserMode.COMPOSERS) }) {
            Text(
                text = stringResource(R.string.library_mode_composers),
                fontWeight = if (mode == LibraryBrowserMode.COMPOSERS) FontWeight.Bold else FontWeight.Normal,
            )
        }
        TextButton(onClick = { onModeChange(LibraryBrowserMode.ARTISTS) }) {
            Text(
                text = stringResource(R.string.library_mode_artists),
                fontWeight = if (mode == LibraryBrowserMode.ARTISTS) FontWeight.Bold else FontWeight.Normal,
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
