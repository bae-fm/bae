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
import uniffi.bae_bridge.enteredNameIsBlank

private val logger = BaeLogger("bae.NewPlaylistDialog")

/**
 * Make a playlist holding [trackIds], none by default: its name and who sees it. Creating it closes
 * the dialog and hands the new playlist's id to [onCreated]; a failure shows in the dialog.
 * [nameIsBlank] is core's answer to whether a typed name is blank.
 */
@Composable
internal fun NewPlaylistDialog(
    session: OpenLibrary,
    onDismiss: () -> Unit,
    onCreated: (String) -> Unit,
    trackIds: List<String> = emptyList(),
    nameIsBlank: (String) -> Boolean = ::enteredNameIsBlank,
) {
    val scope = rememberScreenScope()
    var name by remember { mutableStateOf("") }
    var sharing by remember { mutableStateOf(SHARING_CHOICES.first()) }
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var saving by remember { mutableStateOf(false) }
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts =
            playlistDialogParts(
                title = stringResource(R.string.playlist_new),
                confirmLabel = stringResource(R.string.playlist_create),
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
                        var created: String? = null
                        performBridgeAction(
                            logger = logger,
                            operation = "make a playlist",
                            errors = session.errors,
                            showError = { error = it },
                        ) {
                            created = session.appHandle.createPlaylist(name, sharing, trackIds)
                        }
                        saving = false
                        created?.let {
                            onDismiss()
                            onCreated(it)
                        }
                    }
                },
                onDismiss = onDismiss,
            ),
    )
}
