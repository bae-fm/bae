package fm.bae.app.ui.downloads

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorText
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadSnapshot
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeQueuedRelease

/** The download queue: each pin's progress, pause and retry for the queue, and cancel per item. */
@Composable
internal fun DownloadsScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val snapshot by session.downloadStore.snapshot.collectAsState()
    val config by session.configStore.config.collectAsState()
    Column(modifier = Modifier.fillMaxSize()) {
        DownloadsTopBar(
            paused = snapshot.paused,
            hasDownloads = snapshot.downloads.isNotEmpty(),
            hasFailures = snapshot.total.failed > 0u,
            onBack = onBack,
            onPauseToggle = { session.appHandle.setDownloadsPaused(!snapshot.paused) },
            onRetry = { session.appHandle.retryDownloads() },
        )
        DownloadConcurrencyRow(
            session = session,
            value = config.maxConcurrentDownloads,
        )
        HorizontalDivider()
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            if (snapshot.downloads.isEmpty()) {
                Text(
                    text = stringResource(R.string.downloads_empty),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(ThemeSpace.page),
                )
            } else {
                DownloadsList(
                    snapshot = snapshot,
                    onCancel = { releaseId -> session.appHandle.cancelDownload(releaseId) },
                )
            }
        }
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

@Composable
private fun DownloadsList(
    snapshot: BridgeDownloadSnapshot,
    onCancel: (String) -> Unit,
) {
    val context = LocalContext.current
    LazyColumn(modifier = Modifier.fillMaxSize()) {
        item {
            val summary = downloadQueueSummaryText(context, snapshot)
            if (summary.isNotEmpty()) {
                Text(
                    text = summary,
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
                )
            }
        }
        items(snapshot.downloads, key = { it.releaseId }) { op ->
            DownloadQueueRow(op = op, onCancel = { onCancel(op.releaseId) })
            HorizontalDivider()
        }
    }
}

@Composable
private fun DownloadQueueRow(
    op: BridgeDownloadOp,
    onCancel: () -> Unit,
) {
    val context = LocalContext.current
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(start = ThemeSpace.edge, top = ThemeSpace.related, bottom = ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
            val release = op.release
            Text(
                text = release?.title ?: context.coreString("core.queue.release_missing"),
                style = ThemeText.rowTitle.style,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (release != null) {
                Text(
                    text = release.detailText(context),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            DownloadRowState(op.state)
        }
        IconButton(onClick = onCancel) {
            Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.cancel))
        }
    }
}

@Composable
private fun DownloadRowState(state: BridgeDownloadState) {
    when (state) {
        BridgeDownloadState.Queued -> {
            WaitingToDownloadText()
        }

        is BridgeDownloadState.Active -> {
            DownloadProgressBytes(state.progress)
        }

        is BridgeDownloadState.Failed -> {
            ErrorText(state.error)
        }
    }
}

/** "Waiting to download" — shown on a queued row and the album-detail control. */
@Composable
internal fun WaitingToDownloadText() {
    Text(
        text = stringResource(R.string.download_waiting),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** The active-download progress bar plus its "{done} of {total}" byte line. */
@Composable
internal fun DownloadProgressBytes(progress: BridgeDownloadTransferProgress) {
    val context = LocalContext.current
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        LinearProgressIndicator(
            progress = { progress.fraction.toFloat() },
            modifier = Modifier.fillMaxWidth(),
        )
        Text(
            text = progress.bytesProgressText(context),
            style = ThemeText.fine.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun BridgeQueuedRelease.detailText(context: Context): String {
    val files =
        context.resources.getQuantityString(
            R.plurals.download_file_count,
            fileCount.toInt(),
            fileCount.toInt(),
        )
    return "$files · ${context.formatFileSize(totalSize)}"
}

private fun BridgeDownloadTransferProgress.bytesProgressText(context: Context): String =
    context.coreString(
        "core.download.bytes_progress",
        mapOf(
            "done" to context.formatFileSize(bytesDone.requireDisplayableByteCount()),
            "total" to context.formatFileSize(bytesTotal.requireDisplayableByteCount()),
        ),
    )

@Preview(showBackground = true)
@Composable
private fun DownloadsListPreview() {
    BaeTheme {
        DownloadsList(
            snapshot =
                PreviewData.downloadSnapshot(
                    downloads =
                        listOf(
                            PreviewData.downloadOp(
                                releaseId = "rel-1",
                                state = BridgeDownloadState.Active(PreviewData.downloadTransferProgress()),
                            ),
                            PreviewData.downloadOp(releaseId = "rel-2", state = BridgeDownloadState.Queued),
                            PreviewData.downloadOp(
                                releaseId = "rel-3",
                                state = BridgeDownloadState.Failed("Network unreachable"),
                            ),
                        ),
                ),
            onCancel = {},
        )
    }
}
