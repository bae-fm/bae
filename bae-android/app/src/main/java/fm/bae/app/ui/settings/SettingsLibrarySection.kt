package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.LibraryListing
import fm.bae.app.LocalBugReports
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.data.LibraryWithoutStorage
import fm.bae.app.localizedLine
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.ButtonTone
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.focusOnShow
import fm.bae.app.ui.components.retryAction
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.storageLocationLabel
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState

private val logger = BaeLogger("bae.SettingsLibrarySection")

/** The switch to another library on this device, when there is another. */
@Composable
internal fun SettingsLibrarySwitcher(
    session: OpenLibrary,
    listing: LibraryListing,
    onReloadLibraries: () -> Unit,
    onSwitchLibrary: (BridgeLibrary) -> Unit,
) {
    when (listing) {
        LibraryListing.Unlisted -> {
            Unit
        }

        is LibraryListing.Listed -> {
            if (listing.libraries.size > 1) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
                    verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
                ) {
                    LibrarySwitcher(
                        libraries = listing.libraries,
                        activeLibraryId = session.libraryId,
                        onSwitchLibrary = onSwitchLibrary,
                    )
                }
            }
        }

        // The switcher's list isn't known, so it isn't shown as if it were.
        is LibraryListing.Failed -> {
            Column(modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge)) {
                ReadFailure(
                    error =
                        DisplayError(
                            stringResource(R.string.settings_libraries_load_failed),
                            listing.error.detail,
                            listing.error.offersRetry,
                        ),
                    onRetry = onReloadLibraries,
                )
            }
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
    AndroidCard {
        libraries.forEach { library ->
            // The active library comes from the open session; BridgeLibrary.isActive goes stale after an in-app switch.
            val isActive = library.id == activeLibraryId
            val detail = libraryDetail(library.state)
            AndroidRow(
                title = library.name,
                subtitle = detail.text,
                tone = detail.tone,
                selected = isActive,
                // A library whose settings can't be read offers nothing; one
                // whose reset or restore stopped partway offers to finish it.
                onClick =
                    if (!isActive && library.state !is BridgeLibraryState.Unreadable) {
                        { onSwitchLibrary(library) }
                    } else {
                        null
                    },
            )
        }
    }
}

/** A read Settings needs that failed: the failure where its rows would be, and Retry when it can help. */
@Composable
internal fun ReadFailure(
    error: DisplayError,
    onRetry: () -> Unit,
) {
    AndroidNotice(error, action = error.retryAction(onClick = onRetry))
}

@Composable
internal fun RenameLibraryDialog(
    session: OpenLibrary,
    current: String,
    onDismiss: () -> Unit,
) {
    val scope = rememberScreenScope()
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
                    errors = session.errors,
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
internal fun RenameLibraryDialogContent(
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
            renameLibraryParts(
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
internal fun renameLibraryParts(
    draft: String,
    error: DisplayError?,
    saving: Boolean,
    onDraftChange: (String) -> Unit,
    onSave: () -> Unit,
    onDismiss: () -> Unit,
): DialogParts =
    DialogParts(
        title = stringResource(R.string.settings_rename_library_title),
        content = {
            Column {
                OutlinedTextField(
                    value = draft,
                    onValueChange = onDraftChange,
                    label = { Text(stringResource(R.string.settings_library_new_name)) },
                    supportingText = { Text(stringResource(R.string.name_shown_to_everyone)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth().focusOnShow(),
                )
                error?.let { AndroidNotice(it, look = NoticeLook.QUIET) }
            }
        },
        confirmButton = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = onSave, enabled = draft.isNotBlank() && !saving) {
                Text(stringResource(R.string.settings_rename_library))
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
 * The line under a library's name: its provider and account, or why it doesn't
 * open: its settings can't be read, a reset or restore of this device's copy
 * stopped partway, or removing it from this device stopped partway. A library
 * with no storage, which a phone never has, is a reported bug, said in place.
 */
@Composable
internal fun libraryDetail(state: BridgeLibraryState): LibraryDetail {
    val context = LocalContext.current
    return when (state) {
        is BridgeLibraryState.Openable -> {
            val storage = state.storage
            if (storage == null) {
                val bugs = LocalBugReports.current
                LaunchedEffect(state) { bugs.reportShownInPlace("show a library's storage", LibraryWithoutStorage()) }
                LibraryDetail(context.coreString("core.error.category.internal"), StatusTone.DANGER)
            } else {
                LibraryDetail(storageLocationLabel(storage), tone = null)
            }
        }

        is BridgeLibraryState.Unreadable -> {
            val line =
                checkNotNull(context.localizedLine(state.error)) {
                    "unreadable settings are a diagnostic, never a cancellation"
                }
            LibraryDetail(line, StatusTone.DANGER)
        }

        is BridgeLibraryState.ReplacementUnfinished -> {
            LibraryDetail(
                context.coreString(state.lineKey),
                StatusTone.DANGER,
            )
        }

        is BridgeLibraryState.DeletionUnfinished -> {
            LibraryDetail(
                context.coreString(state.lineKey),
                StatusTone.DANGER,
            )
        }
    }
}

/** The line a library row shows under its name, and the tone it is in. */
internal data class LibraryDetail(
    val text: String,
    val tone: StatusTone?,
)
