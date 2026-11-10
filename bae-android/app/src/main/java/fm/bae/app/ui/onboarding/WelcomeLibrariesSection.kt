package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.DisplayError
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.CardRow
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.Eyebrow
import fm.bae.app.ui.components.LibraryEnding
import fm.bae.app.ui.components.OpenProgressLine
import fm.bae.app.ui.components.RemoveLibraryDialog
import fm.bae.app.ui.components.RowCard
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.StepProgressLine
import fm.bae.app.ui.components.cardRowDivider
import fm.bae.app.ui.settings.libraryDetail
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryState
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.bridgeClosingStepKey

/** The widest the library list grows, on a tablet or a phone held sideways. */
private val librariesWidth = 400.dp

/** What the library list asks of the screen showing it. */
internal class WelcomeLibrariesCallbacks(
    val onOpen: (BridgeLibrary) -> Unit,
    val onDetails: (BridgeLibrary) -> Unit,
    val onRemove: (BridgeLibrary) -> Unit,
    val onRetry: (FailedRemoval, BridgeLibrary) -> Unit,
)

/**
 * The welcome's list of this device's libraries, read each time the welcome
 * shows. Tapping a library opens it; its ⋮ menu shows its details in a sheet
 * or removes it, Remove, or Delete for one that was never synced, asking first.
 * Nothing shows while the list loads or when the device holds no library.
 */
@Composable
internal fun WelcomeLibrariesSection(
    libraries: WelcomeLibraries,
    onOpen: (BridgeLibrary) -> Unit,
) {
    LaunchedEffect(libraries) { libraries.load() }
    WelcomeLibrariesContent(
        list = libraries.list,
        running = libraries.running,
        failed = libraries.failed,
        callbacks =
            WelcomeLibrariesCallbacks(
                onOpen = onOpen,
                onDetails = libraries::showDetails,
                onRemove = libraries::askToRemove,
                onRetry = libraries::retry,
            ),
    )
    libraries.details?.let { LibraryDetailsSheet(it, onDismiss = libraries::closeDetails) }
    libraries.pending?.let { pending ->
        RemoveLibraryDialog(
            ending = LibraryEnding.of(pending.library.state),
            loss = pending.loss,
            onConfirm = { libraries.remove(pending) },
            onDismiss = libraries::dismiss,
        )
    }
}

/**
 * The libraries under their heading, or why they couldn't be listed. While a
 * library is counted or removed, its row shows the step, and no row can be
 * tapped.
 */
@Composable
private fun WelcomeLibrariesContent(
    list: WelcomeLibrariesList,
    running: RunningRemoval?,
    failed: FailedRemoval?,
    callbacks: WelcomeLibrariesCallbacks,
) {
    val column = Modifier.widthIn(max = librariesWidth).fillMaxWidth().padding(bottom = ThemeSpace.section)
    when (list) {
        WelcomeLibrariesList.Loading -> {}

        is WelcomeLibrariesList.Failed -> {
            Column(modifier = column, verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
                Text(text = stringResource(R.string.settings_libraries_load_failed), style = ThemeText.heading.style)
                ErrorNotice(list.error)
            }
        }

        is WelcomeLibrariesList.Loaded -> {
            if (list.libraries.isEmpty()) return
            Column(modifier = column, verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                Eyebrow(stringResource(R.string.onboarding_libraries_title))
                RowCard {
                    list.libraries.forEach { library ->
                        WelcomeLibraryRow(
                            library = library,
                            busy = running != null,
                            running = running?.takeIf { it.libraryId == library.id },
                            failed = failed?.takeIf { it.libraryId == library.id },
                            callbacks = callbacks,
                        )
                    }
                }
            }
        }
    }
}

/**
 * One library: its name over where it is stored, or "Not synced", or why it
 * doesn't open, with a ⋮ menu of Details, for a library that opens, and
 * Remove, or Delete for a library that was never synced. A tap opens it,
 * unless its settings can't be read. While it is counted or removed the row
 * shows the step, and when its removal failed, why, with Retry in place of the
 * menu.
 */
@Composable
private fun WelcomeLibraryRow(
    library: BridgeLibrary,
    busy: Boolean,
    running: RunningRemoval?,
    failed: FailedRemoval?,
    callbacks: WelcomeLibrariesCallbacks,
) {
    if (running != null) {
        RunningLibraryRow(library, running)
        return
    }
    val detail = libraryDetail(library.state)
    val subtitle = failed?.error?.line ?: detail.text
    CardRow(
        title = library.name,
        subtitle = subtitle,
        tone = if (failed != null) StatusTone.DANGER else detail.tone,
        // A library whose settings can't be read offers nothing to open.
        onClick =
            if (!busy && library.state !is BridgeLibraryState.Unreadable) {
                { callbacks.onOpen(library) }
            } else {
                null
            },
    ) {
        if (failed != null) {
            TextButton(onClick = { callbacks.onRetry(failed, library) }, enabled = !busy) {
                Text(stringResource(R.string.retry))
            }
        } else {
            LibraryMenu(library, busy, callbacks)
        }
    }
}

/** The row's ⋮ menu: its details, for a library that opens, then ending it. */
@Composable
private fun LibraryMenu(
    library: BridgeLibrary,
    busy: Boolean,
    callbacks: WelcomeLibrariesCallbacks,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { expanded = true }, enabled = !busy) {
            Icon(imageVector = Icons.Filled.MoreVert, contentDescription = stringResource(R.string.more))
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            if (library.state is BridgeLibraryState.Openable) {
                DropdownMenuItem(
                    text = { Text(stringResource(R.string.library_details_menu)) },
                    onClick = {
                        expanded = false
                        callbacks.onDetails(library)
                    },
                )
            }
            DropdownMenuItem(
                text = {
                    Text(
                        stringResource(
                            when (LibraryEnding.of(library.state)) {
                                LibraryEnding.REMOVE -> R.string.library_remove_menu
                                LibraryEnding.DELETE -> R.string.library_delete_menu
                            },
                        ),
                    )
                },
                onClick = {
                    expanded = false
                    callbacks.onRemove(library)
                },
            )
        }
    }
}

/**
 * A library being counted or removed: the removal's step, else the open's,
 * else what the row waits on, in place of its provider line.
 */
@Composable
private fun RunningLibraryRow(
    library: BridgeLibrary,
    running: RunningRemoval,
) {
    val context = LocalContext.current
    val waiting =
        if (running.removing) R.string.loading_removing_library else R.string.loading_opening_library
    Column(
        modifier =
            Modifier
                .cardRowDivider()
                .fillMaxWidth()
                .padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.group),
        horizontalAlignment = Alignment.Start,
    ) {
        Text(text = library.name, style = ThemeText.rowTitle.style)
        StepProgressLine(
            steps = running.closing,
            title = { context.coreString(bridgeClosingStepKey(it)) },
            fraction = { null },
            modifier = Modifier.fillMaxWidth(),
        ) {
            OpenProgressLine(steps = running.opening, modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = stringResource(waiting),
                    style = ThemeText.body.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

/**
 * The welcome on a phone that holds a synced library and one that was never
 * synced, for the `welcome-libraries` screenshot scene.
 */
@Composable
internal fun WelcomeLibrariesScene() {
    WelcomeScene(libraries = { WelcomeLibrariesPreviewContent(failed = null) })
}

@Composable
private fun WelcomeLibrariesPreviewContent(failed: FailedRemoval?) {
    val dropbox =
        BridgeLibraryState.Openable(
            storage =
                BridgeStorageLocation.Dropbox(
                    location = BridgeDropboxLocation.AppFolder,
                    accountLabel = "name@example.com",
                    folderLabel = null,
                ),
        )
    WelcomeLibrariesContent(
        list =
            WelcomeLibrariesList.Loaded(
                listOf(
                    BridgeLibrary("lib-1", "Library Name", "/tmp/lib-1", dropbox, isActive = false),
                    PreviewData.library(id = "lib-2", name = "Local Library", isActive = false),
                ),
            ),
        running = null,
        failed = failed,
        callbacks = WelcomeLibrariesCallbacks(onOpen = {}, onDetails = {}, onRemove = {}, onRetry = { _, _ -> }),
    )
}

@Preview(showBackground = true)
@Composable
private fun WelcomeLibrariesPreview() {
    BaeTheme {
        Column(modifier = Modifier.padding(ThemeSpace.page)) {
            WelcomeLibrariesPreviewContent(
                failed = FailedRemoval("lib-1", DisplayError("Couldn’t reach your storage."), confirmed = null),
            )
        }
    }
}
