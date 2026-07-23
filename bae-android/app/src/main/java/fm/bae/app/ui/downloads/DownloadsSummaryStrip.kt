package fm.bae.app.ui.downloads

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import uniffi.bae_bridge.BridgeDownloadSnapshot
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress

/**
 * Compact one-line summary of the download queue for the library strip: the
 * paused chip or the count summary, plus the active download's progress bar
 * (the queue is serial, so at most one). Tapping opens the downloads screen.
 * The caller hides it when the queue is empty.
 */
@Composable
internal fun DownloadsSummaryStrip(
    snapshot: BridgeDownloadSnapshot,
    onTap: () -> Unit,
) {
    val context = LocalContext.current
    Surface(
        color = MaterialTheme.colorScheme.surface,
        tonalElevation = 1.dp,
        modifier = Modifier.fillMaxWidth(),
        onClick = onTap,
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text =
                        if (snapshot.paused) {
                            stringResource(R.string.paused)
                        } else {
                            downloadQueueSummaryText(context, snapshot)
                        },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Icon(
                    imageVector = Icons.AutoMirrored.Filled.KeyboardArrowRight,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            snapshot.activeProgress()?.let { progress ->
                LinearProgressIndicator(
                    progress = { progress.fraction.toFloat() },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

/**
 * The queue summary line: the counts core rolls up (downloading / failed /
 * queued), each a localized "{count} <state>" label, joined with " · ". Empty
 * when the queue is idle. bae-core owns the counts; the UI composes and
 * localizes them (the same shape the desktop Downloads pane renders).
 */
internal fun downloadQueueSummaryText(
    context: Context,
    snapshot: BridgeDownloadSnapshot,
): String =
    // Core decides which parts appear, their order, and the drop-if-zero rule;
    // this localizes each and joins.
    snapshot.summaryParts.joinToString(" · ") { part ->
        context.coreString(part.key, mapOf("count" to part.count.toInt()))
    }

private fun BridgeDownloadSnapshot.activeProgress(): BridgeDownloadTransferProgress? =
    downloads.firstNotNullOfOrNull { (it.state as? BridgeDownloadState.Active)?.progress }

@Preview(showBackground = true)
@Composable
private fun DownloadsSummaryStripPreview() {
    BaeTheme {
        DownloadsSummaryStrip(
            snapshot =
                PreviewData.downloadSnapshot(
                    downloads =
                        listOf(
                            PreviewData.downloadOp(
                                state = BridgeDownloadState.Active(PreviewData.downloadTransferProgress()),
                            ),
                        ),
                ),
            onTap = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadsSummaryStripPausedPreview() {
    BaeTheme {
        DownloadsSummaryStrip(snapshot = PreviewData.downloadSnapshot(paused = true), onTap = {})
    }
}
