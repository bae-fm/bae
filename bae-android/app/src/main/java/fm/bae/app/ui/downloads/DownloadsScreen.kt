package fm.bae.app.ui.downloads

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
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
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.countLabelsText
import fm.bae.app.data.DownloadPageStore
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LiveValue
import fm.bae.app.factSeparator
import fm.bae.app.queueWaitText
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.PreviewTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidTopBar
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.retryAction
import kotlinx.coroutines.flow.distinctUntilChanged
import uniffi.bae_bridge.BridgeDownloadCounts
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeQueueWait
import uniffi.bae_bridge.bridgePinSummaryParts

/** The Pinning screen: each pin's progress, pause and retry for the queue, and cancel per item. */
@Composable
internal fun DownloadsScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val state by session.downloadStore.state.collectAsState()
    val config by session.configStore.config.collectAsState()
    // A failed read leaves no queue known to be current, so nothing acts on one.
    val counts = (state as? LiveValue.Current)?.value
    val context = LocalContext.current
    val paused = counts?.wait == BridgeQueueWait.PAUSED
    val page = session.browserPages.downloads
    DisposableEffect(page) {
        page.activate(Unit)
        onDispose(page::deactivate)
    }
    Column(modifier = Modifier.fillMaxSize()) {
        AndroidDownloadsBar(
            paused = paused,
            hasDownloads = counts != null && counts.total.queued + counts.total.active + counts.total.failed > 0u,
            hasFailures = counts != null && counts.total.failed > 0u,
            onBack = onBack,
            onPauseToggle = {
                if (counts != null) {
                    session.messages.perform("pause or resume downloads") {
                        session.appHandle.setDownloadsPaused(!paused)
                    }
                }
            },
            onRetry = { session.messages.perform("retry downloads") { session.appHandle.retryDownloads() } },
        )
        DownloadConcurrencyRow(
            session = session,
            value = config.maxConcurrentDownloads,
            limit = config.maxConcurrentTransfers,
        )
        HorizontalDivider()
        DownloadsBody(
            session = session,
            state = state,
            summary = counts?.let { queueSummaryText(context, it) }.orEmpty(),
            page = page,
            modifier = Modifier.weight(1f).fillMaxWidth(),
        )
    }
}

/**
 * The queue's rows, or why there are none: the counts failing, the first
 * window failing, an empty queue; nothing while the first window loads.
 */
@Composable
private fun DownloadsBody(
    session: OpenLibrary,
    state: LiveValue<BridgeDownloadCounts>,
    summary: String,
    page: DownloadPageStore,
    modifier: Modifier,
) {
    Box(modifier = modifier) {
        val pageError = page.error
        when {
            state is LiveValue.Failed -> {
                AndroidNotice(
                    error = state.error,
                    modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                )
            }

            pageError != null && page.rows.isEmpty() -> {
                AndroidNotice(
                    error = pageError.error,
                    look = NoticeLook.CENTERED,
                    action = pageError.error.retryAction(onClick = pageError.onRetry),
                )
            }

            // The first window is a local read; nothing shows until it lands.
            page.loading && page.rows.isEmpty() -> {}

            page.totalCount == 0 -> {
                Text(
                    text = stringResource(R.string.pinning_empty),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                )
            }

            else -> {
                val listState = rememberLazyListState()
                ReportVisibleDownloads(page, listState)
                DownloadsList(
                    summary = summary,
                    count = page.totalCount,
                    row = { index -> page.rows[index] },
                    downloads = session.downloadStore,
                    listState = listState,
                    onCancel = { releaseId ->
                        session.messages.perform("cancel download") {
                            session.appHandle.cancelDownload(releaseId)
                        }
                    },
                    onRetry = { releaseId ->
                        session.messages.perform("retry download") { session.appHandle.retryDownload(releaseId) }
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
internal fun AndroidDownloadsBar(
    paused: Boolean,
    hasDownloads: Boolean,
    hasFailures: Boolean,
    onBack: () -> Unit,
    onPauseToggle: () -> Unit,
    onRetry: () -> Unit,
) {
    AndroidTopBar(onBack = onBack, title = stringResource(R.string.pinning)) {
        AndroidButton(kind = ButtonKind.TEXT, onClick = onRetry, enabled = hasFailures) {
            Text(stringResource(R.string.retry))
        }
        AndroidButton(kind = ButtonKind.TEXT, onClick = onPauseToggle, enabled = hasDownloads) {
            Text(stringResource(if (paused) R.string.resume else R.string.pause))
        }
    }
}

/** The queue's counts, then what holds it back, joined with the catalog's fact separator. */
private fun queueSummaryText(
    context: Context,
    counts: BridgeDownloadCounts,
): String =
    listOfNotNull(
        context.countLabelsText(bridgePinSummaryParts(counts.total)).ifEmpty { null },
        counts.wait?.let(context::queueWaitText),
    ).joinToString(context.factSeparator())

/** The queue's summary, then its [count] entries in the order queued, each drawn once [row] has read it. */
@Composable
internal fun DownloadsList(
    summary: String,
    count: Int,
    row: (Int) -> BridgeDownloadOp?,
    downloads: DownloadStore,
    listState: LazyListState,
    onCancel: (String) -> Unit,
    onRetry: (String) -> Unit,
) {
    LazyColumn(state = listState, modifier = Modifier.fillMaxSize()) {
        item(key = "summary") {
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
            AndroidDownloadRow(
                op = op,
                progress = downloads.latestProgress(op.releaseId),
                onCancel = { onCancel(op.releaseId) },
                onRetry = { onRetry(op.releaseId) },
            )
            HorizontalDivider()
        }
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadsListPreview() {
    PreviewTheme {
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
            summary = "",
            count = rows.size,
            row = rows::getOrNull,
            downloads = PreviewData.downloadStore(downloadingReleaseId = "rel-1"),
            listState = rememberLazyListState(),
            onCancel = {},
            onRetry = {},
        )
    }
}
