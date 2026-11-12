package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.PushPin
import androidx.compose.material.icons.outlined.PushPin
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.appearance.ThemeIcon
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCheckbox
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.downloads.DownloadProgress
import fm.bae.app.ui.downloads.PinQueuedText
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseDownloadStatus
import uniffi.bae_bridge.BridgeReleaseStorageAction
import uniffi.bae_bridge.bridgeReleasePlayabilityLine
import uniffi.bae_bridge.bridgeTransferActionKey

/**
 * The shown release's download control; nothing when core offers none for the
 * release. A release this device can't play says why instead, as core words
 * it: the device its audio is on ("On Ana’s Mac"), or its originals gone.
 */
@Composable
internal fun AndroidDownloadControlContainer(
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
    val unpin = rememberReleaseUnpin(session, release.id)
    val config by session.configStore.config.collectAsState()

    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
        AndroidDownloadControl(
            status = status,
            progress = session.downloadStore.latestProgress(release.id),
            unpinning = unpin.unpinning,
            asksBeforeUnpinning = config.askBeforeUnpinning,
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
            onUnpin = unpin::unpin,
        )
        unpin.error?.let { shown ->
            AndroidNotice(shown)
        }
    }
}

@Composable
internal fun AndroidDownloadControl(
    status: BridgeReleaseDownloadStatus,
    progress: LatestSource<BridgeDownloadTransferProgress>,
    unpinning: Boolean,
    asksBeforeUnpinning: Boolean,
    onDownload: () -> Unit,
    onCancel: () -> Unit,
    onRetry: () -> Unit,
    onUnpin: (stopAsking: Boolean) -> Unit,
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
            DownloadProgress(progress, label = stringResource(R.string.pinning), modifier = Modifier.fillMaxWidth())
            DownloadActionButton(stringResource(R.string.pin_cancel), Icons.Filled.Close, onCancel)
        }

        is BridgeReleaseDownloadStatus.Failed -> {
            DownloadFailedControl(status.error, onRetry, onCancel)
        }

        BridgeReleaseDownloadStatus.Downloaded -> {
            DownloadedControl(unpinning, asksBeforeUnpinning, onUnpin)
        }
    }
}

@Composable
private fun DownloadFailedControl(
    error: String,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
) {
    AndroidNotice(
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
    asksBeforeUnpinning: Boolean,
    onUnpin: (stopAsking: Boolean) -> Unit,
) {
    var asking by remember { mutableStateOf(false) }
    if (asking) {
        UnpinDialog(
            onConfirm = { stopAsking ->
                asking = false
                onUnpin(stopAsking)
            },
            onDismiss = { asking = false },
        )
    }
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
            AndroidSpinner(size = ThemeIcon.medium)
            Text(
                text = LocalContext.current.coreString(bridgeTransferActionKey(BridgeReleaseStorageAction.UNPIN)),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            DownloadActionButton(stringResource(R.string.unpin), Icons.Outlined.PushPin) {
                if (asksBeforeUnpinning) asking = true else onUnpin(false)
            }
        }
    }
}

/**
 * Asks before Unpin removes the release's files from this phone. [onConfirm] hears whether "Don't
 * show this again" was ticked; Cancel and dismissing change nothing, ticked or not.
 */
@Composable
internal fun UnpinDialog(
    onConfirm: (dontShowAgain: Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    var dontShowAgain by remember { mutableStateOf(false) }
    AndroidDialog(
        onDismissRequest = onDismiss,
        title = stringResource(R.string.unpin_confirm_title),
        message = stringResource(R.string.unpin_confirm_body),
        content = {
            AndroidCheckbox(
                label = stringResource(R.string.dont_show_again),
                checked = dontShowAgain,
                onCheckedChange = { dontShowAgain = it },
            )
        },
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = { onConfirm(dontShowAgain) }) {
                Text(stringResource(R.string.unpin))
            }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
}

/** The bordered button every download action uses. */
@Composable
private fun DownloadActionButton(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
) {
    AndroidButton(kind = ButtonKind.OUTLINED, onClick = onClick) {
        Icon(icon, contentDescription = null, modifier = Modifier.size(ThemeIcon.medium))
        Spacer(modifier = Modifier.width(ThemeSpace.related))
        Text(text)
    }
}
