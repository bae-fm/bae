package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.ArtworkLoadingStore
import fm.bae.app.formatFileSize
import fm.bae.app.requireDisplayableByteCount
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.Notice
import fm.bae.app.ui.components.StatusTone
import uniffi.bae_bridge.BridgeEagerCacheFillProgress
import uniffi.bae_bridge.BridgeEagerCacheFillStatus

@Composable
internal fun ArtworkLoadingBanner(store: ArtworkLoadingStore) {
    val state by store.state.collectAsState()
    if (state.dismissed) return
    when (val current = state.status) {
        BridgeEagerCacheFillStatus.Idle -> {
            Unit
        }

        is BridgeEagerCacheFillStatus.Downloading -> {
            ArtworkDownloadingStatus(current.progress, store::cancel)
        }

        is BridgeEagerCacheFillStatus.Cancelled -> {
            ArtworkNotice {
                ArtworkStatusLine(
                    title = LocalContext.current.coreString("core.artwork_cache.cancelled"),
                    progress = current.progress,
                    trailing = { DismissArtworkButton(store::dismiss) },
                )
            }
        }
    }
}

@Composable
private fun ArtworkDownloadingStatus(
    progress: BridgeEagerCacheFillProgress,
    onCancel: () -> Unit,
) {
    require(progress.bytesTotal > 0uL) { "downloading artwork has no byte total" }
    ArtworkNotice {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
            ArtworkStatusLine(
                title = LocalContext.current.coreString("core.artwork_cache.downloading"),
                progress = progress,
                trailing = { CancelArtworkButton(onCancel) },
            )
            LinearProgressIndicator(
                progress = {
                    (progress.bytesCached.toDouble() / progress.bytesTotal.toDouble()).toFloat()
                },
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** The artwork status as a full-width notice. */
@Composable
private fun ArtworkNotice(content: @Composable RowScope.() -> Unit) {
    Notice(
        tone = StatusTone.NEUTRAL,
        modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        content = content,
    )
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
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
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
                "done" to context.formatFileSize(progress.bytesCached.requireDisplayableByteCount()),
                "total" to context.formatFileSize(progress.bytesTotal.requireDisplayableByteCount()),
            ),
        ),
        style = ThemeText.fine.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}
