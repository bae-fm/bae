package fm.bae.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.localizedLine
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.ErrorText
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.storageLocationLabel
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.bridgeErrorCategoryKey

private val logger = BaeLogger("bae.SettingsLibrarySection")

/** The open library's name, renamed by admins, and the switch to another library on this device. */
@Composable
internal fun SettingsLibrarySection(
    session: OpenLibrary,
    libraries: List<BridgeLibrary>,
    name: LibraryNameRead,
    self: SelfRead,
    onRetryName: () -> Unit,
    onRetrySelf: () -> Unit,
    onSwitchLibrary: (BridgeLibrary) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        Eyebrow(text = stringResource(R.string.settings_library))
        LibraryNameRow(
            session = session,
            name = name,
            self = self,
            onRetryName = onRetryName,
            onRetrySelf = onRetrySelf,
        )
        if (libraries.size > 1) {
            LibrarySwitcher(
                libraries = libraries,
                activeLibraryId = session.libraryId,
                onSwitchLibrary = onSwitchLibrary,
            )
        }
    }
}

@Composable
private fun LibrarySwitcher(
    libraries: List<BridgeLibrary>,
    activeLibraryId: String,
    onSwitchLibrary: (BridgeLibrary) -> Unit,
) {
    Text(
        text = stringResource(R.string.settings_library_switch),
        style = ThemeText.detail.style,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    libraries.forEach { library ->
        // The active library comes from the open session; BridgeLibrary.isActive goes stale after an in-app switch.
        val isActive = library.id == activeLibraryId
        Row(
            modifier =
                Modifier
                    .fillMaxWidth()
                    // A library whose settings can't be read offers nothing; one
                    // whose reset or restore stopped partway offers to finish it.
                    .clickable(enabled = !isActive && library.state !is BridgeLibraryState.Unreadable) {
                        onSwitchLibrary(library)
                    },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(text = library.name)
                // Every row keeps the detail line so rows share one height.
                LibraryDetailLine(library.state)
            }
            // Always present but alpha-toggled so switching the active row
            // never re-measures the row heights.
            Icon(
                imageVector = Icons.Filled.Check,
                contentDescription = null,
                modifier = Modifier.alpha(if (isActive) 1f else 0f),
            )
        }
    }
}

/**
 * The library's one name, as its store holds it now, with Rename for an admin:
 * the name is the household's, and renaming is an admin entry. A name that
 * couldn't be read, or a person whose role couldn't be, shows that failure here
 * with Retry rather than leaving the row or its Rename out.
 */
@Composable
private fun LibraryNameRow(
    session: OpenLibrary,
    name: LibraryNameRead,
    self: SelfRead,
    onRetryName: () -> Unit,
    onRetrySelf: () -> Unit,
) {
    var renaming by remember { mutableStateOf(false) }
    val current =
        when (name) {
            LibraryNameRead.Loading -> {
                return
            }

            is LibraryNameRead.Failed -> {
                ReadFailure(error = name.error, onRetry = onRetryName)
                return
            }

            is LibraryNameRead.Named -> {
                name.name
            }
        }
    val canRename = (self as? SelfRead.Loaded)?.person?.membership?.role == BridgeMemberRole.ADMIN
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(text = current, style = ThemeText.heading.style, modifier = Modifier.weight(1f))
        if (canRename) {
            TextButton(onClick = { renaming = true }) { Text(stringResource(R.string.settings_rename_library)) }
        }
    }
    if (self is SelfRead.Failed) {
        ReadFailure(error = self.error, onRetry = onRetrySelf)
    }
    if (renaming) {
        RenameLibraryDialog(session = session, current = current, onDismiss = { renaming = false })
    }
}

/** A read Settings needs that failed: the failure where its rows would be, and Retry. */
@Composable
internal fun ReadFailure(
    error: DisplayError,
    onRetry: () -> Unit,
) {
    ErrorDetailDisclosure(error)
    TextButton(onClick = onRetry) { Text(stringResource(R.string.retry)) }
}

@Composable
private fun RenameLibraryDialog(
    session: OpenLibrary,
    current: String,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var draft by remember { mutableStateOf(current) }
    var error by remember { mutableStateOf<DisplayError?>(null) }
    var saving by remember { mutableStateOf(false) }
    RenameLibraryDialogContent(
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
                    operation = "rename the library",
                    errors = LocaleErrorDisplays(context),
                    showError = { error = it },
                ) {
                    session.appHandle.renameLibrary(draft.trim())
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
private fun RenameLibraryDialogContent(
    draft: String,
    error: DisplayError?,
    saving: Boolean,
    onDraftChange: (String) -> Unit,
    onSave: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.settings_rename_library)) },
        text = {
            Column {
                OutlinedTextField(
                    value = draft,
                    onValueChange = onDraftChange,
                    label = { Text(stringResource(R.string.settings_library_name)) },
                    supportingText = { Text(stringResource(R.string.name_shown_to_everyone)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                error?.let { ErrorDetailDisclosure(it) }
            }
        },
        confirmButton = {
            TextButton(onClick = onSave, enabled = draft.isNotBlank() && !saving) {
                Text(stringResource(R.string.save))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

@Preview
@Composable
private fun RenameLibraryDialogContentPreview() {
    BaeTheme {
        RenameLibraryDialogContent(
            draft = "Household",
            error = null,
            saving = false,
            onDraftChange = {},
            onSave = {},
            onDismiss = {},
        )
    }
}

@Preview(showBackground = true)
@Composable
private fun LibrarySwitcherPreview() {
    BaeTheme {
        Column {
            LibrarySwitcher(
                libraries =
                    listOf(
                        PreviewData.library(),
                        PreviewData.library(id = "lib-2", name = "Other Library", isActive = false),
                    ),
                activeLibraryId = "lib-1",
                onSwitchLibrary = {},
            )
        }
    }
}

/**
 * The line under a library's name: its provider and account when it has a
 * cloud home, nothing when it doesn't, or why it doesn't open: its settings
 * can't be read, or a reset or restore of this device's copy stopped partway.
 */
@Composable
private fun LibraryDetailLine(state: BridgeLibraryState) {
    when (state) {
        is BridgeLibraryState.Openable -> {
            Text(
                text = state.storage?.let { location -> storageLocationLabel(location) } ?: "",
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        is BridgeLibraryState.Unreadable -> {
            val line =
                checkNotNull(LocalContext.current.localizedLine(state.error)) {
                    "unreadable settings are a diagnostic, never a cancellation"
                }
            Text(text = line, style = ThemeText.detail.style, color = MaterialTheme.colorScheme.error)
        }

        is BridgeLibraryState.ReplacementUnfinished -> {
            Text(
                text =
                    LocalContext.current.coreString(
                        bridgeErrorCategoryKey(BridgeErrorCategory.ReplacementUnfinished(state.by)),
                    ),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.error,
            )
        }
    }
}
