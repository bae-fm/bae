package fm.bae.app.ui.downloads

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.DownloadStore
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.InlineIconButton
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.TitledErrorNotice
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeDownloadOp
import uniffi.bae_bridge.BridgeDownloadState
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeQueuedRelease

@Composable
internal fun DownloadQueueRow(
    op: BridgeDownloadOp,
    progress: LatestSource<BridgeDownloadTransferProgress>,
    onCancel: () -> Unit,
    onRetry: () -> Unit,
) {
    val context = LocalContext.current
    val release = op.release
    Column(
        modifier = Modifier.fillMaxWidth().padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline),
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            if (release != null) {
                Text(
                    text = release.title,
                    style = ThemeText.rowTitle.style,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            } else {
                Text(
                    text = context.coreString("core.queue.release_missing"),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
            }
            InlineIconButton(Icons.Filled.Close, stringResource(R.string.cancel), onCancel)
        }
        if (release != null) {
            Text(
                text = release.detailText(context),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        DownloadRowState(op.state, progress, onRetry)
    }
}

@Composable
private fun DownloadRowState(
    state: BridgeDownloadState,
    progress: LatestSource<BridgeDownloadTransferProgress>,
    onRetry: () -> Unit,
) {
    when (state) {
        BridgeDownloadState.Queued -> {
            PinQueuedText()
        }

        BridgeDownloadState.Active -> {
            DownloadProgress(progress, label = null)
        }

        is BridgeDownloadState.Failed -> {
            TitledErrorNotice(
                title = stringResource(R.string.pin_failed),
                advice = null,
                error = DisplayError(state.error),
                action = ErrorAction(stringResource(R.string.retry), onClick = onRetry),
                look = NoticeLook.QUIET,
            )
        }
    }
}

/** "Queued to pin", shown on a queued row and the album-detail control. */
@Composable
internal fun PinQueuedText() {
    Text(
        text = stringResource(R.string.pin_queued),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** [releaseId]'s download progress, which a View reads from its latest value each display frame. */
@Composable
internal fun ActiveDownloadProgress(
    releaseId: String,
    downloads: DownloadStore,
    label: String?,
) {
    DownloadProgress(downloads.latestProgress(releaseId), label, Modifier.fillMaxWidth())
}

private fun BridgeQueuedRelease.detailText(context: Context): String {
    val files =
        context.resources.getQuantityString(
            R.plurals.pin_file_count,
            fileCount.toInt(),
            fileCount.toInt(),
        )
    return "$files · ${context.formatFileSize(totalSize.requireDisplayableByteCount())}"
}
