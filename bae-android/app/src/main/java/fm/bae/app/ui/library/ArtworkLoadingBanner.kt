package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.CircularProgressIndicator
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
    val status by store.status.collectAsState()
    when (val current = status) {
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
                )
            }
        }

        is BridgeEagerCacheFillStatus.Failed -> {
            ArtworkStatusSurface {
                ArtworkStatusLine(
                    title = LocalContext.current.coreString(current.titleKey),
                    progress = current.progress,
                )
                Text(
                    text = current.error,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
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
    val context = LocalContext.current
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(title, modifier = Modifier.weight(1f))
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
        trailing()
    }
}

@Composable
private fun CancelArtworkButton(onCancel: () -> Unit) {
    TextButton(onClick = onCancel) {
        Text(stringResource(R.string.cancel))
    }
}
