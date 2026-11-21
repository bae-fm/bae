package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Box
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import fm.bae.app.R
import fm.bae.app.ui.appearance.SymbolIcons
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidDivider
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidMenu
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.WIDE_DIALOG_WIDTH

/**
 * The album's own actions, at the end of the page's bar: [onSetPrimaryRelease] makes the release in view
 * the album's primary one, and is `null` when it already is or the album holds one release.
 */
@Composable
internal fun AlbumOptionsMenu(
    onSetPrimaryRelease: (() -> Unit)?,
    onDelete: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        AndroidIconButton(SymbolIcons.MoreVert, stringResource(R.string.album_options), onClick = { expanded = true })
        AndroidMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            if (onSetPrimaryRelease != null) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.set_primary_release)) },
                    onClick = {
                        expanded = false
                        onSetPrimaryRelease()
                    },
                )
                AndroidDivider()
            }
            DropdownMenuItem(
                text = { Text(stringResource(R.string.delete_album)) },
                onClick = {
                    expanded = false
                    onDelete()
                },
            )
        }
    }
}

/** Asks before an album is deleted: what goes with it, what stays, and that it can't be undone. */
@Composable
internal fun DeleteAlbumDialog(
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        width = WIDE_DIALOG_WIDTH,
        onDismissRequest = onDismiss,
        title = stringResource(R.string.delete_album_confirm_title),
        message = stringResource(R.string.delete_album_confirm_body),
        action = DialogAction(stringResource(R.string.delete_album_confirm), onConfirm, destructive = true),
        dismiss = DialogAction(stringResource(R.string.cancel), onDismiss),
    )
}
