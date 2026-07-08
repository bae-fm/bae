package fm.bae.app.ui

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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.formatFileSize
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadSnapshot
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress

/**
 * The download-queue management surface: every queued/active/failed pin with its
 * progress, pause/resume for the whole queue, retry for failed entries, and
 * per-item cancel. A stack destination like Settings and Devices, so it inherits
 * the browser's system-back and saved-state handling. Renders only from the
 * download snapshot — actions never mutate optimistically; the next snapshot
 * re-renders. It does not dismiss when the queue drains: the queue is transient
 * and can empty while open, so an empty state shows rather than yanking the
 * screen away.
 */
@Composable
internal fun DownloadsScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val snapshot by session.downloadStore.snapshot.collectAsState()
    Column(modifier = Modifier.fillMaxSize()) {
        DownloadsTopBar(
            paused = snapshot.paused,
            hasDownloads = snapshot.downloads.isNotEmpty(),
            hasFailures = snapshot.total.failed > 0u,
            onBack = onBack,
            onPauseToggle = { session.appHandle.setDownloadsPaused(!snapshot.paused) },
            onRetry = { session.appHandle.retryDownloads() },
        )
        Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
            if (snapshot.downloads.isEmpty()) {
                Text(
                    text = stringResource(R.string.downloads_empty),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.align(Alignment.Center).padding(32.dp),
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
            modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(
                text = stringResource(R.string.downloads),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
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
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
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
        modifier = Modifier.fillMaxWidth().padding(start = 16.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                text = op.title,
                style = MaterialTheme.typography.bodyLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = op.detailText(context),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
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
            Text(
                text = state.error,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
            )
        }
    }
}

/** "Waiting to download" — shown on a queued row and the album-detail control. */
@Composable
internal fun WaitingToDownloadText() {
    Text(
        text = stringResource(R.string.download_waiting),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** The active-download progress bar plus its "{done} of {total}" byte line. */
@Composable
internal fun DownloadProgressBytes(progress: BridgeDownloadTransferProgress) {
    val context = LocalContext.current
    Column(modifier = Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        LinearProgressIndicator(
            progress = { progress.fraction.toFloat() },
            modifier = Modifier.fillMaxWidth(),
        )
        Text(
            text = progress.bytesProgressText(context),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun BridgeDownloadOp.detailText(context: Context): String {
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

private fun ULong.requireDisplayableByteCount(): Long {
    require(this <= Long.MAX_VALUE.toULong()) { "download byte count exceeds display range" }
    return toLong()
}
