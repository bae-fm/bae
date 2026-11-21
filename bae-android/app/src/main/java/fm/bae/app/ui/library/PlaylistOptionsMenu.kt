package fm.bae.app.ui.library

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
import fm.bae.app.ui.components.AndroidIconButton
import fm.bae.app.ui.components.AndroidMenu
import fm.bae.app.ui.components.DialogAction
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.WIDE_DIALOG_WIDTH

/** The playlist's own actions, at the end of the page's bar: Edit playlist… and Delete playlist…. */
@Composable
internal fun PlaylistOptionsMenu(
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        AndroidIconButton(
            SymbolIcons.MoreVert,
            stringResource(R.string.playlist_options),
            onClick = { expanded = true },
        )
        AndroidMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.playlist_edit_menu)) },
                onClick = {
                    expanded = false
                    onEdit()
                },
            )
            DropdownMenuItem(
                text = { Text(stringResource(R.string.playlist_delete_menu)) },
                onClick = {
                    expanded = false
                    onDelete()
                },
            )
        }
    }
}

/**
 * Asks before bae deletes the playlist named [name], and says that bae deletes it from every device and keeps its
 * tracks.
 */
@Composable
internal fun DeletePlaylistDialog(
    name: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(onDismissRequest = onDismiss, parts = deletePlaylistParts(name, onConfirm, onDismiss))
}

/** What the delete confirmation for the playlist named [name] says and offers. */
@Composable
internal fun deletePlaylistParts(
    name: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    DialogParts(
        width = WIDE_DIALOG_WIDTH,
        title = stringResource(R.string.playlist_delete_title, name),
        message = stringResource(R.string.playlist_delete_message),
        action = DialogAction(stringResource(R.string.playlist_delete_confirm), onConfirm, destructive = true),
        dismiss = DialogAction(stringResource(R.string.cancel), onDismiss),
    )
