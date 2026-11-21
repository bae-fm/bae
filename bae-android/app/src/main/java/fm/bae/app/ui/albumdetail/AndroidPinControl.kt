package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCheckbox
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.WIDE_DIALOG_WIDTH
import fm.bae.app.ui.downloads.DownloadProgress
import fm.bae.app.ui.downloads.PinQueuedText
import fm.bae.fastvalues.LatestSource
import uniffi.bae_bridge.BridgeDownloadTransferProgress
import uniffi.bae_bridge.BridgeRelease
import uniffi.bae_bridge.BridgeReleaseDownloadStatus

/**
 * The shown release's download control; nothing when core offers none for the
 * release, as it offers none for one this device can't play.
 */
@Composable
internal fun AndroidDownloadControlContainer(
    session: OpenLibrary,
    release: BridgeRelease,
) {
    // Core reads the release's place in the download queue into the release itself.
    val status = release.downloadStatus ?: return
    AndroidPinControl(
        status = status,
        // Progress arrives through the download snapshot.
        progress = session.downloadStore.latestProgress(release.id),
        onCancel = {
            session.messages.perform("cancel download") { session.appHandle.cancelDownload(release.id) }
        },
        onRetry = { session.messages.perform("retry download") { session.appHandle.retryDownload(release.id) } },
    )
}

/**
 * A pin under way or failed: Queued to pin with Cancel, Pinning with its bar and Cancel pinning, or
 * Could not pin with Retry and Cancel. A release that is not pinned, or is pinned, shows nothing here:
 * the album's menu offers Pin or Unpin for it.
 */
@Composable
internal fun AndroidPinControl(
    status: BridgeReleaseDownloadStatus,
    progress: LatestSource<BridgeDownloadTransferProgress>,
    onCancel: () -> Unit,
    onRetry: () -> Unit,
) {
    when (status) {
        BridgeReleaseDownloadStatus.Available, BridgeReleaseDownloadStatus.Downloaded -> {}

        BridgeReleaseDownloadStatus.Queued -> {
            Row(
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                PinQueuedText()
                AndroidButton(
                    kind = ButtonKind.OUTLINED,
                    onClick = onCancel,
                    icon = SymbolIcons.Close,
                ) { Text(stringResource(R.string.cancel)) }
            }
        }

        BridgeReleaseDownloadStatus.Downloading -> {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
                DownloadProgress(
                    progress,
                    label = stringResource(R.string.pinning),
                    modifier = Modifier.fillMaxWidth(),
                    stopDot = true,
                )
                AndroidButton(
                    kind = ButtonKind.OUTLINED,
                    onClick = onCancel,
                    icon = SymbolIcons.Close,
                ) { Text(stringResource(R.string.pin_cancel)) }
            }
        }

        is BridgeReleaseDownloadStatus.Failed -> {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.compact)) {
                AndroidNotice(
                    title = stringResource(R.string.pin_failed),
                    advice = null,
                    error = DisplayError(status.error),
                    action = ErrorAction(stringResource(R.string.retry), onClick = onRetry),
                    look = NoticeLook.QUIET,
                )
                AndroidButton(kind = ButtonKind.OUTLINED, onClick = onCancel, icon = SymbolIcons.Close) {
                    Text(stringResource(R.string.cancel))
                }
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
        width = WIDE_DIALOG_WIDTH,
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
        action = DialogAction(stringResource(R.string.unpin), { onConfirm(dontShowAgain) }),
        dismiss = DialogAction(stringResource(R.string.cancel), onDismiss),
    )
}
