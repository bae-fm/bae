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
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.LocaleErrorLines
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.localizedLine
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.downloads.DownloadProgressBytes
import fm.bae.app.ui.downloads.WaitingToDownloadText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseDownloadStatus
import uniffi.bae_bridge.bridgeReleaseDownloadStatus

private val logger = BaeLogger("bae.ReleaseDownloadControl")

/** The shown release's download control; nothing when core offers none for the release. */
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

    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
        DownloadControlBody(
            status = status,
            unpinning = unpinning,
            // Progress arrives through the download snapshot; core skips ids already queued or pinned.
            onDownload = {
                scope.launch {
                    performBridgeAction(
                        logger = logger,
                        operation = "queue release download",
                        errors = LocaleErrorLines(context),
                        showError = session.configStore::showError,
                    ) {
                        session.appHandle.queuePinReleases(listOf(release.id))
                    }
                }
            },
            onCancel = { session.appHandle.cancelDownload(release.id) },
            onRetry = { session.appHandle.retryDownloads() },
            onRemove = {
                unpinError = null
                unpinning = true
                scope.launch { unpinError = runUnpin(session, release.id, context) { unpinning = false } }
            },
        )
        unpinError?.let { message ->
            Text(text = message, style = ThemeText.body.style, color = MaterialTheme.colorScheme.error)
        }
    }
}

/** Unpins [releaseId] and returns the error line to show, or null on success; [onSettled] runs either way. */
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
            Row(
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
                verticalAlignment = Alignment.CenterVertically,
            ) {
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
    Text(text = error, style = ThemeText.body.style, color = MaterialTheme.colorScheme.error)
    Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        // Core has no per-item retry: retryDownloads re-queues every failed entry.
        DownloadActionButton(stringResource(R.string.retry), Icons.Filled.Refresh, onRetry)
        DownloadActionButton(stringResource(R.string.cancel), Icons.Filled.Close, onCancel)
    }
}

@Composable
private fun DownloadedControl(
    unpinning: Boolean,
    onRemove: () -> Unit,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = Icons.Filled.DownloadDone,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(ThemeIcon.small),
        )
        Text(
            text = stringResource(R.string.download_downloaded),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (unpinning) {
            CircularProgressIndicator(modifier = Modifier.size(ThemeIcon.medium), strokeWidth = 2.dp)
        } else {
            DownloadActionButton(stringResource(R.string.download_remove), Icons.Filled.Delete, onRemove)
        }
    }
}

/** The bordered button every download action uses. */
@Composable
private fun DownloadActionButton(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
) {
    OutlinedButton(onClick = onClick) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(ThemeIcon.medium))
        Spacer(modifier = Modifier.width(ThemeSpace.related))
        Text(text)
    }
}

@Composable
private fun DownloadControlBodyStub(status: BridgeReleaseDownloadStatus) {
    DownloadControlBody(
        status = status,
        unpinning = false,
        onDownload = {},
        onCancel = {},
        onRetry = {},
        onRemove = {},
    )
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlAvailablePreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Available)
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlDownloadingPreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Downloading(PreviewData.downloadTransferProgress()))
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlDownloadedPreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Downloaded)
    }
}

@Preview(showBackground = true)
@Composable
private fun DownloadControlFailedPreview() {
    BaeTheme {
        DownloadControlBodyStub(BridgeReleaseDownloadStatus.Failed("Network unreachable"))
    }
}
