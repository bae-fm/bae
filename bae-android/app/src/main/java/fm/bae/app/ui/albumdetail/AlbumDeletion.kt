package fm.bae.app.ui.albumdetail

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.ErrorDetailDisclosure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface

private val logger = BaeLogger("bae.AlbumDeletion")

/**
 * Deleting the album [albumId] from its page: whether the confirmation is up,
 * and why the last delete failed. A delete that succeeds leaves the page
 * through [onDeleted]; one that fails keeps the album in view with its error.
 */
internal class AlbumDeletion(
    private val albumId: String,
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

    /** Close the confirmation and delete the album. */
    fun confirm() {
        confirming = false
        error = null
        scope.launch {
            performBridgeAction(logger, "delete album $albumId", errors, { error = it }) {
                appHandle.deleteAlbum(albumId)
                onDeleted()
            }
        }
    }
}

/** The deletion of the album [albumId] the page shows, run in [scope], leaving the page through [onDeleted]. */
@Composable
internal fun rememberAlbumDeletion(
    session: OpenLibrary,
    albumId: String,
    scope: CoroutineScope,
    onDeleted: () -> Unit,
): AlbumDeletion {
    val errors = LocaleErrorDisplays(LocalContext.current.applicationContext)
    val leave by rememberUpdatedState(onDeleted)
    return remember(albumId, session, scope) {
        AlbumDeletion(albumId, session.appHandle, errors, scope) { leave() }
    }
}

/** The album's own actions, at the end of the page's bar. */
@Composable
internal fun AlbumOptionsMenu(onDelete: () -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { expanded = true }) {
            Icon(imageVector = Icons.Filled.MoreVert, contentDescription = stringResource(R.string.album_options))
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.delete_album), color = MaterialTheme.colorScheme.error) },
                onClick = {
                    expanded = false
                    onDelete()
                },
            )
        }
    }
}

/** Why the last delete failed, under the page's bar, and the confirmation while it is up. */
@Composable
internal fun AlbumDeletionShown(deletion: AlbumDeletion) {
    deletion.error?.let {
        ErrorDetailDisclosure(
            error = it,
            modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
        )
    }
    if (deletion.confirming) {
        DeleteAlbumDialog(onConfirm = deletion::confirm, onDismiss = deletion::dismiss)
    }
}

/** Asks before an album is deleted: what goes with it, what stays, and that it can't be undone. */
@Composable
internal fun DeleteAlbumDialog(
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.delete_album_confirm_title)) },
        text = { Text(stringResource(R.string.delete_album_confirm_body)) },
        confirmButton = {
            TextButton(
                onClick = onConfirm,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) {
                Text(stringResource(R.string.delete_album_confirm))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Preview(showBackground = true)
@Composable
private fun DeleteAlbumDialogPreview() {
    BaeTheme {
        DeleteAlbumDialog(onConfirm = {}, onDismiss = {})
    }
}
