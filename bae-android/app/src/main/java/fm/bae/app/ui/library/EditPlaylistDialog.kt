package fm.bae.app.ui.library

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgePlaylistSummary
import uniffi.bae_bridge.enteredNameIsBlank

private val logger = BaeLogger("bae.EditPlaylistDialog")

/**
 * Change [playlist]'s name and who sees it. Saving writes both in one write and closes the dialog; a
 * failure shows in the dialog, which stays open. [nameIsBlank] is core's answer to whether a typed name is blank.
 */
@Composable
internal fun EditPlaylistDialog(
    session: OpenLibrary,
    playlist: BridgePlaylistSummary,
    onDismiss: () -> Unit,
    nameIsBlank: (String) -> Boolean = ::enteredNameIsBlank,
) {
    val scope = rememberScreenScope()
    var name by remember { mutableStateOf(playlist.name) }
    var sharing by remember { mutableStateOf(playlist.sharing) }
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var saving by remember { mutableStateOf(false) }
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts =
            playlistDialogParts(
                title = stringResource(R.string.playlist_edit_title),
                confirmLabel = stringResource(R.string.save),
                name = name,
                sharing = sharing,
                error = error,
                canConfirm = !nameIsBlank(name) && !saving,
                onNameChange = { name = it },
                onSharingChange = { sharing = it },
                onConfirm = {
                    saving = true
                    error = null
                    scope.launch {
                        var saved = false
                        performBridgeAction(
                            logger = logger,
                            operation = "edit playlist ${playlist.id}",
                            errors = session.errors,
                            showError = { error = it },
                        ) {
                            session.appHandle.editPlaylist(playlist.id, name, sharing)
                            saved = true
                        }
                        saving = false
                        if (saved) onDismiss()
                    }
                },
                onDismiss = onDismiss,
            ),
    )
}
