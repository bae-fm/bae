package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.WarningAmber
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ArtworkLoadingStore
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import uniffi.bae_bridge.BridgeEagerCacheFillProgress
import uniffi.bae_bridge.BridgeEagerCacheFillStatus

@Composable
internal fun ArtworkLoadingBanner(store: ArtworkLoadingStore) {
    val state by store.state.collectAsState()
    if (state.dismissed) return
    when (val current = state.status) {
        BridgeEagerCacheFillStatus.NotRunning,
        is BridgeEagerCacheFillStatus.Complete,
        -> {
            Unit
        }

        is BridgeEagerCacheFillStatus.Scanning -> {
            ArtworkScanningStatus(current.titleKey, store::cancel)
        }

        is BridgeEagerCacheFillStatus.Downloading -> {
            ArtworkDownloadingStatus(current, store::cancel)
        }

        is BridgeEagerCacheFillStatus.Cancelled -> {
            ArtworkStatusSurface {
                ArtworkStatusLine(
                    title = LocalContext.current.coreString(current.titleKey),
                    progress = current.progress,
                    trailing = { DismissArtworkButton(store::dismiss) },
                )
            }
        }

        is BridgeEagerCacheFillStatus.Failed -> {
            ArtworkFailureStatus(current, store::dismiss)
        }
    }
}

@Composable
private fun ArtworkScanningStatus(
    titleKey: String,
    onCancel: () -> Unit,
) {
    ArtworkStatusSurface {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            CircularProgressIndicator(modifier = Modifier.size(18.dp))
            Text(
                LocalContext.current.coreString(titleKey),
                modifier = Modifier.weight(1f),
            )
            CancelArtworkButton(onCancel)
        }
    }
}

@Composable
private fun ArtworkDownloadingStatus(
    status: BridgeEagerCacheFillStatus.Downloading,
    onCancel: () -> Unit,
) {
    val progress = status.progress
    require(progress.bytesTotal > 0uL) { "downloading artwork has no byte total" }
    ArtworkStatusSurface {
        ArtworkStatusLine(
            title = LocalContext.current.coreString(status.titleKey),
            progress = progress,
            trailing = { CancelArtworkButton(onCancel) },
        )
        LinearProgressIndicator(
            progress = {
                (progress.bytesDone.toDouble() / progress.bytesTotal.toDouble()).toFloat()
            },
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

@Composable
private fun ArtworkStatusSurface(content: @Composable ColumnScope.() -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surfaceVariant) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
            content = content,
        )
    }
}

@Composable
private fun ArtworkStatusLine(
    title: String,
    progress: BridgeEagerCacheFillProgress,
    trailing: @Composable () -> Unit = {},
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(title, modifier = Modifier.weight(1f))
        ArtworkByteProgress(progress)
        trailing()
    }
}

@Composable
private fun CancelArtworkButton(onCancel: () -> Unit) {
    TextButton(onClick = onCancel) {
        Text(stringResource(R.string.cancel))
    }
}

@Composable
private fun ArtworkFailureStatus(
    status: BridgeEagerCacheFillStatus.Failed,
    onDismiss: () -> Unit,
) {
    var detailsVisible by rememberSaveable(status) { mutableStateOf(false) }
    val title = LocalContext.current.coreString(status.titleKey)
    Surface(color = MaterialTheme.colorScheme.errorContainer) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(Icons.Filled.WarningAmber, contentDescription = null, modifier = Modifier.size(18.dp))
            Text(
                text = title,
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.bodySmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            TextButton(onClick = { detailsVisible = true }) { Text(stringResource(R.string.details)) }
            DismissArtworkButton(onDismiss)
        }
    }
    if (detailsVisible) {
        AlertDialog(
            onDismissRequest = { detailsVisible = false },
            title = { Text(title) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    ArtworkByteProgress(status.progress)
                    SelectionContainer {
                        Text(
                            text = status.error,
                            modifier = Modifier.verticalScroll(rememberScrollState()),
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { detailsVisible = false }) { Text(stringResource(R.string.close)) }
            },
        )
    }
}

@Composable
private fun DismissArtworkButton(onDismiss: () -> Unit) {
    IconButton(onClick = onDismiss) {
        Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.close))
    }
}

@Composable
private fun ArtworkByteProgress(progress: BridgeEagerCacheFillProgress) {
    val context = LocalContext.current
    Text(
        context.coreString(
            "core.download.bytes_progress",
            mapOf(
                "done" to context.formatFileSize(progress.bytesDone.requireDisplayableByteCount()),
                "total" to context.formatFileSize(progress.bytesTotal.requireDisplayableByteCount()),
            ),
        ),
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}
