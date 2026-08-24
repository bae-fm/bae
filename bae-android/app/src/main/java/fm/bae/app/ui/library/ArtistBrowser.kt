package fm.bae.app.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.ArtistPageStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverImage
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSortField
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeSortDirection

@Composable
internal fun rememberArtistPage(
    session: OpenLibrary,
    sortCriterion: BridgeArtistSortCriterion,
): ArtistPageStore {
    val page = session.browserPages.artists
    DisposableEffect(page, sortCriterion) {
        page.activate(sortCriterion)
        onDispose(page::deactivate)
    }
    return page
}

@Composable
internal fun ArtistListContent(
    session: OpenLibrary,
    page: ArtistPageStore,
    onSelectArtist: (String) -> Unit,
) {
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage = stringResource(R.string.library_empty_artists),
    ) {
        val listState = rememberLazyListState()
        LaunchedEffect(page, listState) {
            snapshotFlow {
                val visible = listState.layoutInfo.visibleItemsInfo
                (visible.firstOrNull()?.index ?: 0) to (visible.lastOrNull()?.index ?: 0)
            }.distinctUntilChanged().collect { (first, last) ->
                page.reportVisibleRange(first, last)
            }
        }
        LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
            items(page.totalCount, key = { index -> page.rows[index]?.artistId ?: "artist-slot-$index" }) { index ->
                page.rows[index]?.let { artist ->
                    ArtistSummaryRow(artist = artist, onClick = { onSelectArtist(artist.artistId) })
                }
            }
        }
    }
}

private val ARTIST_SORT_FIELDS =
    listOf(
        BridgeArtistSortField.NAME,
        BridgeArtistSortField.ALBUM_COUNT,
    )

@Composable
internal fun ArtistSortMenu(
    criterion: BridgeArtistSortCriterion,
    onChange: (BridgeArtistSortCriterion) -> Unit,
) {
    fun BridgeArtistSortField.labelRes(): Int =
        when (this) {
            BridgeArtistSortField.NAME -> R.string.sort_name
            BridgeArtistSortField.ALBUM_COUNT -> R.string.search_section_albums
        }
    var expanded by remember { mutableStateOf(false) }
    val ascending = criterion.direction == BridgeSortDirection.ASCENDING
    Box {
        IconButton(onClick = { expanded = true }) {
            Icon(Icons.AutoMirrored.Filled.Sort, contentDescription = stringResource(R.string.sort))
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            ARTIST_SORT_FIELDS.forEach { field ->
                DropdownMenuItem(
                    text = { Text(stringResource(field.labelRes())) },
                    onClick = {
                        onChange(BridgeArtistSortCriterion(field, criterion.direction))
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
                    val direction = if (ascending) BridgeSortDirection.DESCENDING else BridgeSortDirection.ASCENDING
                    onChange(BridgeArtistSortCriterion(criterion.field, direction))
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
internal fun ArtistSummaryRow(
    artist: BridgeArtistSummary,
    onClick: (() -> Unit)?,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
                .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoverImage(
            cover = artist.image,
            cornerRadius = 6.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
            contentDescription = artist.name,
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = artist.name,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
            )
            Text(
                text = stringResource(R.string.album_count, artist.albumCount),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ArtistSummaryRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            ArtistSummaryRow(artist = PreviewData.artistSummary(), onClick = {})
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ArtistSortMenuPreview() {
    BaeTheme {
        ArtistSortMenu(
            criterion = BridgeArtistSortCriterion(BridgeArtistSortField.NAME, BridgeSortDirection.ASCENDING),
            onChange = {},
        )
    }
}
