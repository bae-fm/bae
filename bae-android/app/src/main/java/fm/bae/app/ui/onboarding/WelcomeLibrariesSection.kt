package fm.bae.app.ui.onboarding

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.AndroidSpinner
import fm.bae.app.ui.components.LibraryEnding
import fm.bae.app.ui.components.MoreMenu
import fm.bae.app.ui.components.OpenProgressLine
import fm.bae.app.ui.components.RemoveLibraryDialog
import fm.bae.app.ui.components.StepProgressLine
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
internal fun WelcomeLibrariesContent(
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
                AndroidNotice(list.error)
            }
        }

        is WelcomeLibrariesList.Loaded -> {
            if (list.libraries.isEmpty()) return
            Column(modifier = column, verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                AndroidSection(stringResource(R.string.onboarding_libraries_title))
                AndroidCard {
                    list.libraries.forEach { library ->
                        WelcomeLibraryRow(
                            library = library,
                            busy = running != null,
                            running = running?.takeIf { it.libraryId == library.id },
                            callbacks = callbacks,
                        )
                    }
                    AndroidRow(
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
 * One library: its name alone, or under it, in red, why it doesn't open, with a
 * ⋮ menu of Details, for a library that opens, and Remove, or Delete for a
 * library that was never synced. A tap opens it, unless its settings can't be
 * read. While it is counted or removed the row shows the step.
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
    val problem = libraryDetail(library.state).takeIf { library.state !is BridgeLibraryState.Openable }
    AndroidRow(
        title = library.name,
        subtitle = problem?.text,
        tone = problem?.tone,
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
                text = { Text(stringResource(R.string.details_menu)) },
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
 * A library being counted or removed: its name over the removal's step, else
 * the open's, else what the row waits on, as words alone, with a spinner where
 * the ⋮ was.
 */
@Composable
private fun RunningLibraryRow(
    library: BridgeLibrary,
    running: RunningRemoval,
) {
    val context = LocalContext.current
    val waiting =
        if (running.removing) R.string.loading_removing_library else R.string.loading_opening_library
    AndroidRow(
        title = library.name,
        subtitleContent = {
            StepProgressLine(
                steps = running.closing,
                title = { context.coreString(bridgeClosingStepKey(it)) },
                fraction = { null },
                modifier = Modifier.fillMaxWidth(),
                bar = false,
            ) {
                OpenProgressLine(steps = running.opening, modifier = Modifier.fillMaxWidth(), bar = false) {
                    Text(
                        text = stringResource(waiting),
                        style = ThemeText.body.style,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            }
        },
    ) {
        Box(modifier = Modifier.size(SPINNER_SLOT), contentAlignment = Alignment.Center) { AndroidSpinner() }
    }
}

/** The room the ⋮ takes, which the spinner takes in its place. */
private val SPINNER_SLOT = 48.dp

/**
 * The welcome on a phone that holds a synced library and one that was never
 * synced, for the `welcome-libraries` screenshot scene.
 */
@Composable
internal fun WelcomeLibrariesScene() {
    WelcomeScene(libraries = WelcomeLibrariesView(listed = true) { onAdd -> WelcomeLibrariesPreviewContent(onAdd) })
}

/**
 * The same welcome while the synced library is being removed and has not yet
 * reported a step, for the `welcome-removing` screenshot scene.
 */
@Composable
internal fun WelcomeRemovingScene() {
    val running =
        RunningRemoval(
            libraryId = "lib-1",
            removing = true,
            opening = null,
            closing = null,
        )
    WelcomeScene(
        libraries = WelcomeLibrariesView(listed = true) { onAdd -> WelcomeLibrariesPreviewContent(onAdd, running) },
    )
}

@Composable
private fun WelcomeLibrariesPreviewContent(
    onAdd: () -> Unit,
    running: RunningRemoval? = null,
) {
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
                    BridgeLibrary("lib-1", "Home music", "/tmp/lib-1", dropbox, isActive = false),
                    PreviewData.library(id = "lib-2", name = "Odile’s records", isActive = false),
                ),
            ),
        running = running,
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
