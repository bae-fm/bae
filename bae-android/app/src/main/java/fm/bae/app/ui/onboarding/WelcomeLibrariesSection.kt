package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
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
import fm.bae.app.ui.components.MoreMenu
import fm.bae.app.ui.components.OpenProgressLine
import fm.bae.app.ui.components.RemoveLibraryDialog
import fm.bae.app.ui.components.RowCard
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
    val onAdd: () -> Unit,
)

/**
 * The welcome's list of this device's libraries, read each time the welcome
 * shows. Tapping a library opens it; its ⋮ menu shows its details in a sheet
 * or removes it, Remove, or Delete for one that was never synced, asking first.
 * The list ends in Add library. Nothing shows while the list loads or when the
 * device holds no library.
 */
@Composable
internal fun WelcomeLibrariesSection(
    libraries: WelcomeLibraries,
    onOpen: (BridgeLibrary) -> Unit,
    onAdd: () -> Unit,
) {
    LaunchedEffect(libraries) { libraries.load() }
    WelcomeLibrariesContent(
        list = libraries.list,
        running = libraries.running,
        callbacks =
            WelcomeLibrariesCallbacks(
                onOpen = onOpen,
                onDetails = libraries::showDetails,
                onRemove = libraries::askToRemove,
                onAdd = onAdd,
            ),
    )
    libraries.failed?.let { failure ->
        RemovalFailedDialog(
            library = failure.library,
            error = failure.error,
            onRetry = { libraries.retry(failure) },
            onCancel = libraries::dismissFailure,
        )
    }
    libraries.details?.let { LibraryDetailsSheet(it, onDismiss = libraries::closeDetails) }
    libraries.pending?.let { pending ->
        RemoveLibraryDialog(
            library = pending.library,
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
                            callbacks = callbacks,
                        )
                    }
                    CardRow(
                        title = stringResource(R.string.welcome_add_library),
                        action = true,
                        leadingIcon = Icons.Filled.Add,
                        onClick = callbacks.onAdd.takeIf { running == null },
                    )
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
 * shows the step.
 */
@Composable
private fun WelcomeLibraryRow(
    library: BridgeLibrary,
    busy: Boolean,
    running: RunningRemoval?,
    callbacks: WelcomeLibrariesCallbacks,
) {
    if (running != null) {
        RunningLibraryRow(library, running)
        return
    }
    val detail = libraryDetail(library.state)
    CardRow(
        title = library.name,
        subtitle = detail.text,
        tone = detail.tone,
        // A library whose settings can't be read offers nothing to open.
        onClick =
            if (!busy && library.state !is BridgeLibraryState.Unreadable) {
                { callbacks.onOpen(library) }
            } else {
                null
            },
    ) {
        LibraryMenu(library, busy, callbacks)
    }
}

/** The row's ⋮ menu: its details, for a library that opens, then ending it. */
@Composable
private fun LibraryMenu(
    library: BridgeLibrary,
    busy: Boolean,
    callbacks: WelcomeLibrariesCallbacks,
) {
    MoreMenu(enabled = !busy) { close ->
        if (library.state is BridgeLibraryState.Openable) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.library_details_menu)) },
                onClick = {
                    close()
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
                close()
                callbacks.onRemove(library)
            },
        )
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
    WelcomeScene(libraries = WelcomeLibrariesView(listed = true) { onAdd -> WelcomeLibrariesPreviewContent(onAdd) })
}

@Composable
private fun WelcomeLibrariesPreviewContent(onAdd: () -> Unit) {
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
        callbacks =
            WelcomeLibrariesCallbacks(
                onOpen = {},
                onDetails = {},
                onRemove = {},
                onAdd = onAdd,
            ),
    )
}

@Preview(showBackground = true)
@Composable
private fun WelcomeLibrariesPreview() {
    BaeTheme {
        Column(modifier = Modifier.padding(ThemeSpace.page)) {
            WelcomeLibrariesPreviewContent(onAdd = {})
        }
    }
}
