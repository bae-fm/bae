package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRadio
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeHistorySharing

private val logger = BaeLogger("bae.PeoplePages")

/** Who sees what this person plays, as a choice of radio rows. */
@Composable
internal fun ListeningHistoryDialog(
    current: BridgeHistorySharing,
    onChoose: (BridgeHistorySharing) -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts = listeningHistoryParts(current = current, onChoose = onChoose, onDismiss = onDismiss),
    )
}

@Composable
internal fun listeningHistoryParts(
    current: BridgeHistorySharing,
    onChoose: (BridgeHistorySharing) -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    DialogParts(
        title = stringResource(R.string.settings_history),
        content = {
            Column {
                BridgeHistorySharing.entries.forEach { choice ->
                    AndroidRadio(
                        label = stringResource(choice.labelRes()),
                        checked = choice == current,
                        onClick = { onChoose(choice) },
                    )
                }
                Text(
                    stringResource(R.string.settings_history_note),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }
        },
        confirmButton = {},
        dismissButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                onClick = onDismiss,
            ) { Text(stringResource(R.string.cancel)) }
        },
    )

/** Renames this person's profile, which everyone in the library sees; a failure stays in the dialog. */
@Composable
internal fun RenameProfileDialog(
    you: YouActions,
    current: String,
    onDismiss: () -> Unit,
) {
    val scope = rememberScreenScope()
    var draft by remember { mutableStateOf(current) }
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var saving by remember { mutableStateOf(false) }
    RenameProfileDialogContent(
        draft = draft,
        error = error,
        saving = saving,
        onDraftChange = { draft = it },
        onSave = {
            saving = true
            error = null
            scope.launch {
                var renamed = false
                performBridgeAction(
                    logger = logger,
                    operation = "rename the profile",
                    errors = you.errors,
                    showError = { error = it },
                ) {
                    you.rename(draft.trim())
                    renamed = true
                }
                saving = false
                if (renamed) onDismiss()
            }
        },
        onDismiss = onDismiss,
    )
}

@Composable
internal fun RenameProfileDialogContent(
    draft: String,
    error: DisplayError?,
    saving: Boolean,
    onDraftChange: (String) -> Unit,
    onSave: () -> Unit,
    onDismiss: () -> Unit,
) {
    AndroidDialog(
        onDismissRequest = onDismiss,
        parts =
            renameProfileParts(
                draft = draft,
                error = error,
                saving = saving,
                onDraftChange = onDraftChange,
                onSave = onSave,
                onDismiss = onDismiss,
            ),
    )
}

@Composable
internal fun renameProfileParts(
    draft: String,
    error: DisplayError?,
    saving: Boolean,
    onDraftChange: (String) -> Unit,
    onSave: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    DialogParts(
        title = stringResource(R.string.onboarding_person_name),
        content = {
            Column {
                OutlinedTextField(
                    value = draft,
                    onValueChange = onDraftChange,
                    label = { Text(stringResource(R.string.invite_name)) },
                    supportingText = { Text(stringResource(R.string.name_shown_to_everyone)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                error?.let { AndroidNotice(it, look = NoticeLook.QUIET) }
            }
        },
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = onSave, enabled = draft.isNotBlank() && !saving) {
                Text(stringResource(R.string.save))
            }
        },
        dismissButton = {
            AndroidButton(
                kind = ButtonKind.TEXT,
                tone = ButtonTone.NEUTRAL,
                onClick = onDismiss,
            ) { Text(stringResource(R.string.cancel)) }
        },
    )
