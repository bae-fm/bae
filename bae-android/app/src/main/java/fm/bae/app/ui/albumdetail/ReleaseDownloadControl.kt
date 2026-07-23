package fm.bae.app.ui.albumdetail

import android.content.Context
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.DownloadDone
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.localizedLine
import fm.bae.app.ui.downloads.DownloadProgressBytes
import fm.bae.app.ui.downloads.WaitingToDownloadText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseDownloadStatus
import uniffi.bae_bridge.bridgeReleaseDownloadStatus

private val logger = BaeLogger("bae.ReleaseDownloadControl")

/**
 * Offline control for the shown release: Download / progress + Cancel /
 * Downloaded + Remove Download. Core joins the pin state, the storage actions it
 * offers, and the download queue into that state; the snapshot and the release
 * invalidations keep it live. Actions never mutate
 * optimistically — the next snapshot (or the release invalidation after a pin or
 * unpin) re-renders. Renders nothing when core offers no control for the release
 * (no cloud home, or a local release).
 */
@Composable
internal fun ReleaseDownloadControl(
    session: OpenLibrary,
    release: BridgeRelease,
) {
    val snapshot by session.downloadStore.snapshot.collectAsState()
    val status =
        bridgeReleaseDownloadStatus(
            pinned = release.pinned,
            storageActions = release.storageActions,
            downloads = snapshot,
            releaseId = release.id,
        ) ?: return
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var unpinning by remember(release.id) { mutableStateOf(false) }
    var unpinError by remember(release.id) { mutableStateOf<String?>(null) }

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        DownloadControlBody(
            status = status,
            unpinning = unpinning,
            // Fire-and-forget: progress and queue state arrive via the download
            // snapshot. Re-enqueuing is idempotent — core skips ids already
            // queued or pinned.
            onDownload = { scope.launch { session.appHandle.queuePinReleases(listOf(release.id)) } },
            onCancel = { session.appHandle.cancelDownload(release.id) },
            onRetry = { session.appHandle.retryDownloads() },
            onRemove = {
                unpinError = null
                unpinning = true
                scope.launch { unpinError = runUnpin(session, release.id, context) { unpinning = false } }
            },
        )
        unpinError?.let { message ->
            Text(text = message, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
        }
    }
}

/**
 * Unpin [releaseId], returning a user-facing error line to display (or null on
 * success). [onSettled] runs whether it succeeds or fails. A cancellation (the
 * screen left mid-unpin) propagates — core's drop guard emits the terminal
 * release invalidation, which flips the control back to Download.
 */
private suspend fun runUnpin(
    session: OpenLibrary,
    releaseId: String,
    context: Context,
    onSettled: () -> Unit,
): String? =
    try {
        session.appHandle.unpinRelease(releaseId)
        null
    } catch (e: CancellationException) {
        throw e
    } catch (e: BridgeException) {
        logger.error("unpinRelease failed for $releaseId", e)
        // Null already means "nothing to show" — no ifEmpty workaround needed.
        context.localizedLine(e)
    } catch (e: Exception) {
        logger.error("unpinRelease failed for $releaseId", e)
        e.message ?: e::class.java.simpleName
    } finally {
        onSettled()
    }

@Composable
private fun DownloadControlBody(
    status: BridgeReleaseDownloadStatus,
    unpinning: Boolean,
    onDownload: () -> Unit,
    onCancel: () -> Unit,
    onRetry: () -> Unit,
    onRemove: () -> Unit,
) {
    when (status) {
        BridgeReleaseDownloadStatus.Available -> {
            DownloadActionButton(stringResource(R.string.download), Icons.Filled.Download, onDownload)
        }

        BridgeReleaseDownloadStatus.Queued -> {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                WaitingToDownloadText()
                DownloadActionButton(stringResource(R.string.cancel), Icons.Filled.Close, onCancel)
            }
        }

        is BridgeReleaseDownloadStatus.Downloading -> {
            DownloadProgressBytes(status.progress)
            DownloadActionButton(stringResource(R.string.cancel), Icons.Filled.Close, onCancel)
        }

        is BridgeReleaseDownloadStatus.Failed -> {
            DownloadFailedControl(status.error, onRetry, onCancel)
        }

        BridgeReleaseDownloadStatus.Downloaded -> {
            DownloadedControl(unpinning, onRemove)
        }
    }
}

@Composable
private fun DownloadFailedControl(
    error: String,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
) {
    Text(text = error, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        // Core has no per-item retry: retryDownloads flips every failed entry
        // back to queued, like the desktop Downloads pane.
        DownloadActionButton(stringResource(R.string.retry), Icons.Filled.Refresh, onRetry)
        DownloadActionButton(stringResource(R.string.cancel), Icons.Filled.Close, onCancel)
    }
}

@Composable
private fun DownloadedControl(
    unpinning: Boolean,
    onRemove: () -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        Icon(
            imageVector = Icons.Filled.DownloadDone,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(18.dp),
        )
        Text(
            text = stringResource(R.string.download_downloaded),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (unpinning) {
            CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
        } else {
            DownloadActionButton(stringResource(R.string.download_remove), Icons.Filled.Delete, onRemove)
        }
    }
}

/** A bordered caption button — the shared shape for every download action. */
@Composable
private fun DownloadActionButton(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
) {
    OutlinedButton(onClick = onClick) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(modifier = Modifier.width(8.dp))
        Text(text)
    }
}
