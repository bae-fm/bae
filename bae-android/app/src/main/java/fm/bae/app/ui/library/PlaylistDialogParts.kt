package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSegmentedControl
import fm.bae.app.ui.components.AndroidTextField
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.focusOnShow
import uniffi.bae_bridge.BridgeSharing

/** The choices of who sees a playlist, in the order the dialog shows them; a new playlist starts on the first. */
internal val SHARING_CHOICES = listOf(BridgeSharing.PRIVATE, BridgeSharing.HOUSEHOLD)

/**
 * The dialog of a playlist's name and who sees it, under [title], with a [confirmLabel] button, enabled
 * while [canConfirm] is true, that calls [onConfirm]. The New playlist and Edit playlist dialogs share it.
 */
@Composable
internal fun playlistDialogParts(
    title: String,
    confirmLabel: String,
    name: String,
    sharing: BridgeSharing,
    error: DisplayError?,
    canConfirm: Boolean,
    onNameChange: (String) -> Unit,
    onSharingChange: (BridgeSharing) -> Unit,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    DialogParts(
        title = title,
        content = {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.group)) {
                AndroidTextField(
                    value = name,
                    onValueChange = onNameChange,
                    label = stringResource(R.string.playlist_name),
                    modifier = Modifier.focusOnShow(),
                )
                Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                    AndroidSegmentedControl(
                        options = SHARING_CHOICES.map { stringResource(it.labelRes()) },
                        selected = SHARING_CHOICES.indexOf(sharing),
                        onSelect = { onSharingChange(SHARING_CHOICES[it]) },
                    )
                    Text(
                        text = stringResource(sharing.noteRes()),
                        style = ThemeText.detail.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                error?.let { AndroidNotice(it, look = NoticeLook.QUIET) }
            }
        },
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = onConfirm, enabled = canConfirm) {
                Text(confirmLabel)
            }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
