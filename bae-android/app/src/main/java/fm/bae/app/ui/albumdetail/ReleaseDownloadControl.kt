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
import androidx.compose.material.icons.filled.PushPin
import androidx.compose.material.icons.outlined.PushPin
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.DownloadStore
import fm.bae.app.displayError
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.TitledErrorNotice
import fm.bae.app.ui.components.WaitingIndicator
import fm.bae.app.ui.downloads.ActiveDownloadProgress
import fm.bae.app.ui.downloads.PinQueuedText
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseDownloadStatus
import uniffi.bae_bridge.BridgeReleaseStorageAction
import uniffi.bae_bridge.bridgeReleasePlayabilityLine
import uniffi.bae_bridge.bridgeTransferActionKey

private val logger = BaeLogger("bae.ReleaseDownloadControl")

/**
 * The shown release's download control; nothing when core offers none for the
 * release. A release this device can't play says why instead, as core words
 * it: the device its audio is on ("On Ana's Mac"), or its originals gone.
 */
@Composable
internal fun ReleaseDownloadControl(
    session: OpenLibrary,
    release: BridgeRelease,
) {
    val unplayable = bridgeReleasePlayabilityLine(release.playability)
    if (unplayable != null) {
        UnplayableLine(unplayable)
        return
    }
    // Core reads the release's place in the download queue into the release itself.
    val status = release.downloadStatus ?: return
    PlayableReleaseDownloadControl(session = session, release = release, status = status)
}

/** Why this device can't play the release, as core's [message] words it. */
@Composable
internal fun UnplayableLine(message: BridgeMessage) {
    Text(
        text = LocalContext.current.coreString(message),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** The control for a release this device can play, in the download [status] core read for it. */
@Composable
private fun PlayableReleaseDownloadControl(
    session: OpenLibrary,
    release: BridgeRelease,
    status: BridgeReleaseDownloadStatus,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var unpinning by remember(release.id) { mutableStateOf(false) }
    var unpinError by remember(release.id) { mutableStateOf<DisplayError?>(null) }

    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
        DownloadControlBody(
            status = status,
            releaseId = release.id,
            downloads = session.downloadStore,
            unpinning = unpinning,
            // Progress arrives through the download snapshot; core skips ids already queued or pinned.
            onDownload = {
                session.messages.perform("queue release download") {
                    session.appHandle.queuePinReleases(listOf(release.id))
                }
            },
            onCancel = {
                session.messages.perform("cancel download") { session.appHandle.cancelDownload(release.id) }
            },
            onRetry = { session.messages.perform("retry download") { session.appHandle.retryDownload(release.id) } },
            onRemove = {
                unpinError = null
                unpinning = true
                scope.launch { unpinError = runUnpin(session, release.id, context) { unpinning = false } }
            },
        )
        unpinError?.let { shown ->
            ErrorNotice(shown)
        }
    }
}

/**
 * Unpins [releaseId] and returns the error to show, or null on success; [onSettled] runs either way.
 * Any failure but a typed bridge error is a bug, left to the screen scope's bug handler.
 */
private suspend fun runUnpin(
    session: OpenLibrary,
    releaseId: String,
    context: Context,
    onSettled: () -> Unit,
): DisplayError? =
    try {
        session.appHandle.unpinReleases(listOf(releaseId))
        null
    } catch (e: CancellationException) {
        throw e
    } catch (e: BridgeException) {
        logger.error("unpinReleases failed for $releaseId", e)
        context.displayError(e)
    } finally {
        onSettled()
    }

@Composable
internal fun DownloadControlBody(
    status: BridgeReleaseDownloadStatus,
    releaseId: String,
    downloads: DownloadStore,
    unpinning: Boolean,
    onDownload: () -> Unit,
    onCancel: () -> Unit,
    onRetry: () -> Unit,
    onRemove: () -> Unit,
) {
    when (status) {
        BridgeReleaseDownloadStatus.Available -> {
            DownloadActionButton(stringResource(R.string.pin), Icons.Filled.PushPin, onDownload)
        }

        BridgeReleaseDownloadStatus.Queued -> {
            Row(
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                PinQueuedText()
                DownloadActionButton(stringResource(R.string.cancel), Icons.Filled.Close, onCancel)
            }
        }

        BridgeReleaseDownloadStatus.Downloading -> {
            ActiveDownloadProgress(releaseId, downloads, label = stringResource(R.string.pinning))
            DownloadActionButton(stringResource(R.string.pin_cancel), Icons.Filled.Close, onCancel)
        }

        is BridgeReleaseDownloadStatus.Failed -> {
            DownloadFailedControl(status.error, onRetry, onCancel)
        }

        BridgeReleaseDownloadStatus.Downloaded -> {
            DownloadedControl(unpinning, onRemove)
        }

        BridgeReleaseDownloadStatus.CopyingHere -> {
            CopyingHereControl(unpinning, onRemove)
        }
    }
}

@Composable
private fun DownloadFailedControl(
    error: String,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
) {
    TitledErrorNotice(
        title = stringResource(R.string.pin_failed),
        advice = null,
        error = DisplayError(error),
        action = ErrorAction(stringResource(R.string.retry), onClick = onRetry),
        look = NoticeLook.QUIET,
    )
    DownloadActionButton(stringResource(R.string.cancel), Icons.Filled.Close, onCancel)
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
            imageVector = Icons.Filled.PushPin,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(ThemeIcon.small),
        )
        Text(
            text = stringResource(R.string.pinned),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (unpinning) {
            WaitingIndicator(size = ThemeIcon.medium)
            Text(
                text = LocalContext.current.coreString(bridgeTransferActionKey(BridgeReleaseStorageAction.UNPIN)),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            DownloadActionButton(stringResource(R.string.unpin), Icons.Outlined.PushPin, onRemove)
        }
    }
}

/**
 * A release moved to the cloud to be kept here, whose copies coven is still
 * downloading: the download queue holds no entry for it, so it has no progress
 * to show. Cancelling unpins it, which ends coven's request.
 */
@Composable
private fun CopyingHereControl(
    unpinning: Boolean,
    onStop: () -> Unit,
) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        WaitingIndicator(size = ThemeIcon.medium)
        Text(
            text = stringResource(R.string.pinning),
            style = ThemeText.detail.style,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (!unpinning) {
            DownloadActionButton(stringResource(R.string.pin_cancel), Icons.Filled.Close, onStop)
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
