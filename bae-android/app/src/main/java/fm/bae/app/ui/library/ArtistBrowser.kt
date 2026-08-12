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
import androidx.compose.runtime.DisposableEffect
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
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.bae_bridge.ArtistPageCallback
import uniffi.bae_bridge.BridgeArtistPage
import uniffi.bae_bridge.BridgeArtistSortCriterion
import uniffi.bae_bridge.BridgeArtistSortField
import uniffi.bae_bridge.BridgeArtistSummary
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.BridgeSortDirection

private const val TAG = "bae.ArtistBrowser"
private val logger = BaeLogger(TAG)

internal class ArtistPage(
    private val session: OpenLibrary,
    private val sortCriterion: BridgeArtistSortCriterion,
    private val appContext: Context,
    private val scope: CoroutineScope,
    private val onRetry: () -> Unit,
) {
    val artists = mutableStateMapOf<String, BridgeArtistSummary>()
    val order = mutableStateListOf<String>()
    var totalCount by mutableStateOf(0)
        private set
    var loading by mutableStateOf(true)
        private set
    var error by mutableStateOf<PageError?>(null)
        private set
    private val pages = mutableMapOf<Int, List<String>>()
    private val subscriptions = mutableMapOf<Int, LiveSubscription>()

    private fun ingest(offset: Int, page: BridgeArtistPage) {
        page.rows.forEach { artist ->
            artists[artist.artistId] = artist
        }
        pages[offset] = page.rows.map { it.artistId }
        order.clear()
        order.addAll(pages.toSortedMap().values.flatten().distinct())
        totalCount = page.totalCount.toInt()
        loading = false
        error = null
    }

    fun start() {
        loading = true
        error = null
        subscribe(0)
    }

    fun loadMore() {
        if (order.size >= totalCount) return
        subscribe((pages.keys.maxOrNull() ?: -PAGE_SIZE) + PAGE_SIZE)
    }

    private fun subscribe(offset: Int) {
        if (subscriptions.containsKey(offset)) return
        subscriptions[offset] =
            session.library.subscribeArtistPage(
                sortCriterion,
                offset.toULong(),
                PAGE_SIZE.toULong(),
                object : ArtistPageCallback {
                    override fun onValue(value: BridgeArtistPage) {
                        scope.launch(Dispatchers.Main.immediate) { ingest(offset, value) }
                    }

                    override fun onError(errorValue: BridgeException) {
                        logger.error("Artist page subscription failed at offset $offset", errorValue)
                        scope.launch(Dispatchers.Main.immediate) {
                            loading = false
                            error = PageError(errorValue.message ?: appContext.getString(R.string.library_load_failed), onRetry)
                        }
                    }
                },
            )
    }

    fun cancel() {
        subscriptions.values.forEach(LiveSubscription::cancel)
        subscriptions.clear()
    }
}

@Composable
internal fun rememberArtistPage(
    session: OpenLibrary,
    sortCriterion: BridgeArtistSortCriterion,
    appContext: Context,
): ArtistPage {
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    var retryToken by remember(sortCriterion) { mutableStateOf(0) }
    val page =
        remember(sortCriterion, retryToken) {
            ArtistPage(session, sortCriterion, appContext, scope) { retryToken++ }
        }
    DisposableEffect(page) {
        page.start()
        onDispose(page::cancel)
    }
    return page
}

@Composable
internal fun ArtistListContent(
    page: ArtistPage,
    onSelectArtist: (String) -> Unit,
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
                    text = stringResource(R.string.library_empty_artists),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
                )
            }
        }

        else -> {
            LazyColumn(modifier = Modifier.fillMaxSize()) {
                items(page.order, key = { it }) { artistId ->
                    val artist =
                        checkNotNull(page.artists[artistId]) {
                            "artist page order referenced missing artist: $artistId"
                        }
                    ArtistSummaryRow(
                        artist = artist,
                        onClick = { onSelectArtist(artistId) },
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
