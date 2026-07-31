package fm.bae.app.ui.library

import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.ImageStore
import fm.bae.app.data.LocalImageStore
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.components.CoverImage
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.bae_bridge.BridgeComposerSortCriterion
import uniffi.bae_bridge.BridgeComposerSortField
import uniffi.bae_bridge.BridgeComposerSummary
import uniffi.bae_bridge.BridgeSortDirection

private const val TAG = "bae.ComposerBrowser"
private val logger = BaeLogger(TAG)

internal class ComposerPage(
    private val session: OpenLibrary,
    private val sortCriterion: BridgeComposerSortCriterion,
    private val appContext: Context,
    private val onRetry: () -> Unit,
) {
    val composers = mutableStateMapOf<String, BridgeComposerSummary>()
    val order = mutableStateListOf<String>()
    var totalCount by mutableStateOf(0)
        private set
    var loading by mutableStateOf(true)
        private set
    var error by mutableStateOf<PageError?>(null)
        private set
    private var loadedOffset = 0

    private fun ingest(page: List<BridgeComposerSummary>) {
        page.forEach { composer ->
            if (!composers.containsKey(composer.artistId)) order.add(composer.artistId)
            composers[composer.artistId] = composer
        }
    }

    suspend fun loadFirst() {
        loading = true
        error = null
        try {
            val (count, page) =
                withContext(Dispatchers.IO) {
                    val c = session.library.composerCount().toInt()
                    val p = session.library.composerPage(sortCriterion, 0u, PAGE_SIZE.toULong())
                    c to p
                }
            totalCount = count
            ingest(page)
            loadedOffset = PAGE_SIZE
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to load first composer page", e)
            error = PageError(appContext.getString(R.string.library_load_failed), onRetry)
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
                    session.library.composerPage(sortCriterion, offset.toULong(), PAGE_SIZE.toULong())
                }
            ingest(more)
            loadedOffset = offset + PAGE_SIZE
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            logger.error("Failed to load composer page at offset $offset", e)
            error = PageError(appContext.getString(R.string.library_load_more_failed), onRetry)
        }
    }
}

@Composable
internal fun rememberComposerPage(
    session: OpenLibrary,
    generation: Long,
    sortCriterion: BridgeComposerSortCriterion,
    appContext: Context,
): ComposerPage {
    var retryToken by remember(generation, sortCriterion) { mutableStateOf(0) }
    val page =
        remember(generation, sortCriterion, retryToken) {
            ComposerPage(session, sortCriterion, appContext) { retryToken++ }
        }
    LaunchedEffect(page) { page.loadFirst() }
    return page
}

@Composable
internal fun ComposerListContent(
    page: ComposerPage,
    onSelectComposer: (String) -> Unit,
) {
    val pageError = page.error
    when {
        pageError != null && page.order.isEmpty() -> {
            Column(
                modifier = Modifier.fillMaxSize().padding(32.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Text(text = pageError.message, color = MaterialTheme.colorScheme.error)
                TextButton(onClick = pageError.onRetry) { Text(stringResource(R.string.retry)) }
            }
        }

        page.loading && page.order.isEmpty() -> {
            Box(modifier = Modifier.fillMaxSize()) {
                CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
            }
        }

        page.totalCount == 0 -> {
            Box(modifier = Modifier.fillMaxSize()) {
                Text(
                    text = stringResource(R.string.library_empty_composers),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                )
            }
        }

        else -> {
            LazyColumn(modifier = Modifier.fillMaxSize()) {
                items(page.order, key = { it }) { artistId ->
                    val composer =
                        checkNotNull(page.composers[artistId]) {
                            "composer page order referenced missing composer: $artistId"
                        }
                    ComposerSummaryRow(
                        composer = composer,
                        onClick = { onSelectComposer(artistId) },
                    )
                }
                item {
                    LaunchedEffect(page.order.size, page.totalCount) {
                        page.loadMore()
                    }
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
