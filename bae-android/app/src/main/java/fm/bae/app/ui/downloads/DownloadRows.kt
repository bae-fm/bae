package fm.bae.app.ui.downloads

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.DownloadStore
import fm.bae.app.data.LatestSource
import fm.bae.app.formatFileSize
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorText
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeQueuedRelease

@Composable
internal fun DownloadQueueRow(
    op: BridgeDownloadOp,
    progress: LatestSource<BridgeDownloadTransferProgress>,
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
            DownloadRowState(op.state, progress)
        }
        IconButton(onClick = onCancel) {
            Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.cancel))
        }
    }
}

@Composable
private fun DownloadRowState(
    state: BridgeDownloadState,
    progress: LatestSource<BridgeDownloadTransferProgress>,
) {
    when (state) {
        BridgeDownloadState.Queued -> {
            WaitingToDownloadText()
        }

        BridgeDownloadState.Active -> {
            DownloadProgress(progress)
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

/** [releaseId]'s download progress, which a View reads from its latest value each display frame. */
@Composable
internal fun ActiveDownloadProgress(
    releaseId: String,
    downloads: DownloadStore,
) {
    DownloadProgress(downloads.latestProgress(releaseId), Modifier.fillMaxWidth())
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
