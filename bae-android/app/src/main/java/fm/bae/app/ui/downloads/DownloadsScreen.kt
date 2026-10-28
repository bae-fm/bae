package fm.bae.app.ui.downloads

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.countLabelsText
import fm.bae.app.data.DownloadPageStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LiveValue
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeCountLabel
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadState

/** The download queue: each pin's progress, pause and retry for the queue, and cancel per item. */
@Composable
internal fun DownloadsScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val state by session.downloadStore.state.collectAsState()
    val config by session.configStore.config.collectAsState()
    // A failed read leaves no queue known to be current, so nothing acts on one.
    val counts = (state as? LiveValue.Current)?.value
    val page = session.browserPages.downloads
    DisposableEffect(page) {
        page.activate(Unit)
        onDispose(page::deactivate)
    }
    Column(modifier = Modifier.fillMaxSize()) {
        DownloadsTopBar(
            paused = counts?.paused == true,
            hasDownloads = counts != null && counts.total.queued + counts.total.active + counts.total.failed > 0u,
            hasFailures = counts != null && counts.total.failed > 0u,
            onBack = onBack,
            onPauseToggle = {
                counts?.let {
                    session.messages.perform("pause or resume downloads") {
                        session.appHandle.setDownloadsPaused(!it.paused)
                    }
                }
            },
            onRetry = { session.messages.perform("retry downloads") { session.appHandle.retryDownloads() } },
        )
        DownloadConcurrencyRow(
            session = session,
            value = config.maxConcurrentDownloads,
        )
        HorizontalDivider()
        DownloadsBody(
            session = session,
            state = state,
            summaryParts = counts?.summaryParts.orEmpty(),
            page = page,
            modifier = Modifier.weight(1f).fillMaxWidth(),
        )
    }
}

/** The queue's rows, or why there are none: the counts failing, the first window failing or loading, an empty queue. */
@Composable
private fun DownloadsBody(
    session: OpenLibrary,
    state: LiveValue<BridgeDownloadCounts>,
    summaryParts: List<BridgeCountLabel>,
    page: DownloadPageStore,
    modifier: Modifier,
) {
    Box(modifier = modifier) {
        val pageError = page.error
        when {
            state is LiveValue.Failed -> {
                ErrorDetailDisclosure(
                    error = state.error,
                    modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                )
            }

            pageError != null && page.rows.isEmpty() -> {
                Column(
                    modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    ErrorDetailDisclosure(pageError.error)
                    TextButton(onClick = pageError.onRetry) { Text(stringResource(R.string.retry)) }
                }
            }

            page.loading && page.rows.isEmpty() -> {
                CircularProgressIndicator(modifier = Modifier.align(Alignment.Center))
            }

            page.totalCount == 0 -> {
                Text(
                    text = stringResource(R.string.downloads_empty),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                )
            }

            else -> {
                val listState = rememberLazyListState()
                ReportVisibleDownloads(page, listState)
                DownloadsList(
                    summaryParts = summaryParts,
                    count = page.totalCount,
                    row = { index -> page.rows[index] },
                    downloads = session.downloadStore,
                    listState = listState,
                    onCancel = { releaseId ->
                        session.messages.perform("cancel download") {
                            session.appHandle.cancelDownload(releaseId)
                        }
                    },
                )
            }
        }
    }
}

/** Tells [page] which rows show, so it reads the windows around them. The summary line is row 0. */
@Composable
private fun ReportVisibleDownloads(
    page: DownloadPageStore,
    listState: LazyListState,
) {
    LaunchedEffect(page, listState) {
        snapshotFlow {
            val visible = listState.layoutInfo.visibleItemsInfo.mapNotNull { it.key as? Int }
            (visible.firstOrNull() ?: 0) to (visible.lastOrNull() ?: 0)
        }.distinctUntilChanged()
            .collect { (first, last) -> page.reportVisibleRange(first, last) }
    }
}

@Composable
private fun DownloadsTopBar(
    paused: Boolean,
    hasDownloads: Boolean,
    hasFailures: Boolean,
    onBack: () -> Unit,
    onPauseToggle: () -> Unit,
    onRetry: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(
                text = stringResource(R.string.downloads),
                style = ThemeText.title.style,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onRetry, enabled = hasFailures) {
                Text(stringResource(R.string.retry))
            }
            TextButton(onClick = onPauseToggle, enabled = hasDownloads) {
                Text(stringResource(if (paused) R.string.resume else R.string.pause))
            }
        }
    }
}

/** The queue's summary, then its [count] entries in the order queued, each drawn once [row] has read it. */
@Composable
private fun DownloadsList(
    summaryParts: List<BridgeCountLabel>,
    count: Int,
    row: (Int) -> BridgeDownloadOp?,
    downloads: DownloadStore,
    listState: LazyListState,
    onCancel: (String) -> Unit,
) {
    val context = LocalContext.current
    LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
        item(key = "summary") {
            val summary = context.countLabelsText(summaryParts)
            if (summary.isNotEmpty()) {
                Text(
                    text = summary,
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
                )
            }
        }
        // Keyed by place in the queue, which is what the windows read by.
        items(count, key = { index -> index }) { index ->
            val op = row(index) ?: return@items
            // The row's progress View reads the release's latest bytes each frame; nothing recomposes for them.
            DownloadQueueRow(
                op = op,
                progress = downloads.latestProgress(op.releaseId),
                onCancel = { onCancel(op.releaseId) },
            )
            HorizontalDivider()
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadsListPreview() {
    BaeTheme {
        val rows =
            listOf(
                PreviewData.downloadOp(releaseId = "rel-1", state = BridgeDownloadState.Active),
                PreviewData.downloadOp(releaseId = "rel-2", state = BridgeDownloadState.Queued),
                PreviewData.downloadOp(
                    releaseId = "rel-3",
                    state = BridgeDownloadState.Failed("Network unreachable"),
                ),
            )
        DownloadsList(
            summaryParts = emptyList(),
            count = rows.size,
            row = rows::getOrNull,
            downloads = PreviewData.downloadStore(downloadingReleaseId = "rel-1"),
            listState = rememberLazyListState(),
            onCancel = {},
        )
    }
}
