package fm.bae.app.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidSegmentedButtons
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.focusOnShow
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeSharing
import uniffi.bae_bridge.enteredNameIsBlank

private val logger = BaeLogger("bae.NewPlaylistDialog")

/** The choices of who sees a playlist, in the order they are offered; a new playlist starts on the first. */
private val SHARING_CHOICES = listOf(BridgeSharing.PRIVATE, BridgeSharing.HOUSEHOLD)

/**
 * Make a playlist: its name and who sees it. Creating it closes the dialog; a failure shows in the
 * dialog. [nameIsBlank] is core's answer to whether a typed name is blank.
 */
@Composable
internal fun NewPlaylistDialog(
    session: OpenLibrary,
    onDismiss: () -> Unit,
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
            newPlaylistParts(
                name = name,
                sharing = sharing,
                error = error,
                canCreate = !nameIsBlank(name) && !saving,
                onNameChange = { name = it },
                onSharingChange = { sharing = it },
                onCreate = {
                    saving = true
                    error = null
                    scope.launch {
                        var created = false
                        performBridgeAction(
                            logger = logger,
                            operation = "make a playlist",
                            errors = session.errors,
                            showError = { error = it },
                        ) {
                            session.appHandle.createPlaylist(name, sharing, emptyList())
                            created = true
                        }
                        saving = false
                        if (created) onDismiss()
                    }
                },
                onDismiss = onDismiss,
            ),
    )
}

@Composable
private fun newPlaylistParts(
    name: String,
    sharing: BridgeSharing,
    error: DisplayError?,
    canCreate: Boolean,
    onNameChange: (String) -> Unit,
    onSharingChange: (BridgeSharing) -> Unit,
    onCreate: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    DialogParts(
        title = stringResource(R.string.playlist_new),
        content = {
            Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.group)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = onNameChange,
                    label = { Text(stringResource(R.string.playlist_name)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth().focusOnShow(),
                )
                Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                    AndroidSegmentedButtons(
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
            AndroidButton(kind = ButtonKind.TEXT, onClick = onCreate, enabled = canCreate) {
                Text(stringResource(R.string.playlist_create))
            }
        },
        dismissButton = {
            AndroidButton(kind = ButtonKind.TEXT, tone = ButtonTone.NEUTRAL, onClick = onDismiss) {
                Text(stringResource(R.string.cancel))
            }
        },
    )
