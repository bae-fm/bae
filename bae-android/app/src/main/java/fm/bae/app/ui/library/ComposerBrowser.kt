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
import fm.bae.app.data.ComposerPageStore
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverImage
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeSortDirection

@Composable
internal fun rememberComposerPage(
    session: OpenLibrary,
    sortCriterion: BridgeComposerSortCriterion,
): ComposerPageStore {
    val page = session.browserPages.composers
    DisposableEffect(page, sortCriterion) {
        page.activate(sortCriterion)
        onDispose(page::deactivate)
    }
    return page
}

@Composable
internal fun ComposerListContent(
    session: OpenLibrary,
    page: ComposerPageStore,
    onSelectComposer: (String) -> Unit,
) {
    LibraryPageContent(
        session = session,
        page = page,
        emptyMessage = stringResource(R.string.library_empty_composers),
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
            items(
                count = page.totalCount,
                key = { index -> page.rows[index]?.artistId ?: "composer-slot-$index" },
            ) { index ->
                page.rows[index]?.let { composer ->
                    ComposerSummaryRow(
                        composer = composer,
                        onClick = { onSelectComposer(composer.artistId) },
                    )
                }
            }
        }
    }
}

private val COMPOSER_SORT_FIELDS =
    listOf(
        BridgeComposerSortField.NAME,
        BridgeComposerSortField.WORK_COUNT,
        BridgeComposerSortField.LINKED_RELEASE_COUNT,
    )

@Composable
internal fun ComposerSortMenu(
    criterion: BridgeComposerSortCriterion,
    onChange: (BridgeComposerSortCriterion) -> Unit,
) {
    fun BridgeComposerSortField.labelRes(): Int =
        when (this) {
            BridgeComposerSortField.NAME -> R.string.sort_name
            BridgeComposerSortField.WORK_COUNT -> R.string.search_section_works
            BridgeComposerSortField.LINKED_RELEASE_COUNT -> R.string.search_section_releases
        }
    var expanded by remember { mutableStateOf(false) }
    val ascending = criterion.direction == BridgeSortDirection.ASCENDING
    Box {
        IconButton(onClick = { expanded = true }) {
            Icon(Icons.AutoMirrored.Filled.Sort, contentDescription = stringResource(R.string.sort))
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            COMPOSER_SORT_FIELDS.forEach { field ->
                DropdownMenuItem(
                    text = { Text(stringResource(field.labelRes())) },
                    onClick = {
                        onChange(BridgeComposerSortCriterion(field, criterion.direction))
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
                    onChange(BridgeComposerSortCriterion(criterion.field, direction))
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
internal fun ComposerSummaryRow(
    composer: BridgeComposerSummary,
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
            cover = composer.image,
            cornerRadius = 6.dp,
            iconPadding = 12.dp,
            modifier = Modifier.size(48.dp),
            contentDescription = composer.name,
        )
        Spacer(modifier = Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = composer.name,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
            )
            Text(
                text = stringResource(R.string.work_count, composer.workCount.toLong()),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
            )
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ComposerSummaryRowPreview() {
    BaeTheme {
        CompositionLocalProvider(LocalImageStore provides ImageStore.unresolved()) {
            ComposerSummaryRow(composer = PreviewData.composerSummary(), onClick = {})
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun ComposerSortMenuPreview() {
    BaeTheme {
        ComposerSortMenu(
            criterion = BridgeComposerSortCriterion(BridgeComposerSortField.NAME, BridgeSortDirection.ASCENDING),
            onChange = {},
        )
    }
}
