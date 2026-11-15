package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidMenu
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface

private val logger = BaeLogger("bae.PlaylistDeletion")

/**
 * Deleting the playlist [playlistId] from its page: whether the confirmation is up, and why the last
 * delete failed. A delete that succeeds leaves the page through [onDeleted]; one that fails keeps the
 * playlist in view with its error.
 */
internal class PlaylistDeletion(
    private val playlistId: String,
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
    private val onDeleted: () -> Unit,
) {
    var confirming by mutableStateOf(false)
        private set

    var error by mutableStateOf<DisplayError?>(null)
        private set

    /** Show the confirmation. */
    fun ask() {
        confirming = true
    }

    /** Close the confirmation without deleting. */
    fun dismiss() {
        confirming = false
    }

    /** Close the confirmation and delete the playlist. */
    fun confirm() {
        confirming = false
        error = null
        scope.launch {
            performBridgeAction(logger, "delete playlist $playlistId", errors, { error = it }) {
                appHandle.deletePlaylist(playlistId)
                onDeleted()
            }
        }
    }
}

/** The deletion of the playlist [playlistId] the page shows, run in [scope], leaving the page through [onDeleted]. */
@Composable
internal fun rememberPlaylistDeletion(
    session: OpenLibrary,
    playlistId: String,
    scope: CoroutineScope,
    onDeleted: () -> Unit,
): PlaylistDeletion {
    val errors = session.errors
    val leave by rememberUpdatedState(onDeleted)
    return remember(playlistId, session, scope) {
        PlaylistDeletion(playlistId, session.appHandle, errors, scope) { leave() }
    }
}

/** The playlist's own actions, at the end of the page's bar: Edit playlist… and Delete playlist…. */
@Composable
internal fun PlaylistOptionsMenu(
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { expanded = true }) {
            Icon(imageVector = Icons.Filled.MoreVert, contentDescription = stringResource(R.string.playlist_options))
        }
        AndroidMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.playlist_edit_menu)) },
                onClick = {
                    expanded = false
                    onEdit()
                },
            )
            DropdownMenuItem(
                text = { Text(stringResource(R.string.playlist_delete_menu), color = MaterialTheme.colorScheme.error) },
                onClick = {
                    expanded = false
                    onDelete()
                },
            )
        }
    }
}

/** Why the last delete failed, under the page's bar, and the confirmation for [name] while it is up. */
@Composable
internal fun PlaylistDeletionShown(
    deletion: PlaylistDeletion,
    name: String,
) {
    deletion.error?.let {
        AndroidNotice(
            error = it,
            modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        )
    }
    if (deletion.confirming) {
        DeletePlaylistDialog(name = name, onConfirm = deletion::confirm, onDismiss = deletion::dismiss)
    }
}

/** Asks before the playlist named [name] is deleted: that it goes from every device, and its tracks stay. */
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
        title = stringResource(R.string.playlist_delete_title, name),
        message = stringResource(R.string.playlist_delete_message),
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.DESTRUCTIVE, onClick = onConfirm) {
                Text(stringResource(R.string.playlist_delete_confirm))
            }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
