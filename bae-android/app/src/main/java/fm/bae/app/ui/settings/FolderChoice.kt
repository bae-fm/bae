package fm.bae.app.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import fm.bae.app.DisplayError
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OAuthProviders
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.ui.BaeAppChrome
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorAction
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.WaitingIndicator
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeStorageFolder
import uniffi.bae_bridge.BridgeStorageLocation

/**
 * The folder choice Dropbox and OneDrive share: the empty folder the library lives in, picked from
 * the signed-in provider's list or made here. [calls] and [words] are the provider's, and the choice
 * draws both the same: a path when folders open into others, the folders (a chosen one marked), a new
 * folder, and the folder setup found holding other files. [setup] runs the setup, which the person
 * can stop with Cancel, calling [onCancel].
 */
@Composable
internal fun FolderChoice(
    session: OpenLibrary,
    providers: OAuthProviders,
    calls: FolderCalls,
    words: FolderWords,
    setup: StorageSetupRunner,
    onError: (DisplayError?) -> Unit,
    onCancel: () -> Unit,
) {
    val scope = rememberScreenScope()
    val browser = remember(session) { FolderBrowser(calls, session.errors, words.provider) }
    LaunchedEffect(browser) { browser.openTop() }

    FolderChoiceBody(browser, words, setup, onCancel) { target ->
        browser.work = FolderWork.SETTING_UP
        onError(null)
        scope.launch {
            setup.run("set up storage", claim = { browser.place(it) }) { deviceName, bridgeSetup ->
                providers.setUpStorage(session.appHandle, target.location, deviceName, bridgeSetup)
            }
            browser.work = null
        }
    }
}

/** What [FolderChoice] shows of [browser]'s state; [onSetUp] sets the chosen folder up. */
@Composable
internal fun FolderChoiceBody(
    browser: FolderBrowser,
    words: FolderWords,
    setup: StorageSetupRunner?,
    onCancel: () -> Unit,
    onSetUp: (BridgeStorageFolder) -> Unit,
) {
    val scope = rememberScreenScope()
    val shown = browser.shown
    if (shown != null) {
        FolderHeading(browser, shown, words)
    }
    browser.failure?.let { failure ->
        val retry =
            ErrorAction(stringResource(R.string.retry), enabled = !browser.busy) { scope.launch { failure.retry() } }
        val error = failure.error
        if (error != null) {
            ErrorNotice(error, action = retry)
        } else {
            TextButton(onClick = retry.onClick, enabled = retry.enabled) { Text(retry.label) }
        }
    }
    FolderWorkLine(browser.work, words, setup, onCancel)
    if (shown == null) return
    FolderList(browser, shown, words)
    FolderActions(browser, shown, onSetUp)
}

/** What the person is choosing, and where they are when folders open into others. */
@Composable
private fun FolderHeading(
    browser: FolderBrowser,
    shown: FolderListing,
    words: FolderWords,
) {
    val scope = rememberScreenScope()
    Text(stringResource(words.heading), style = ThemeText.heading.style)
    if (browser.nests) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (shown.path.size > 1) {
                TextButton(onClick = { scope.launch { browser.up(shown) } }, enabled = !browser.busy) {
                    Text(stringResource(R.string.back))
                }
            }
            Text(text = shown.path.joinToString(" › ") { it.name }, modifier = Modifier.weight(1f))
        }
    }
}

/** The folders to choose from, the line when there are none, and the notes under them. */
@Composable
private fun FolderList(
    browser: FolderBrowser,
    shown: FolderListing,
    words: FolderWords,
) {
    val scope = rememberScreenScope()
    if (shown.folders.isEmpty()) {
        Text(stringResource(words.empty))
    }
    shown.folders.forEach { folder ->
        FolderRow(
            folder = folder,
            chosen = browser.selected == folder,
            opens = browser.nests,
            enabled = !browser.busy,
            onChoose = { browser.select(folder) },
            onOpen = { scope.launch { browser.open(shown, folder) } },
        )
    }
    browser.takenName?.let { Text(stringResource(words.nameTaken, it), color = MaterialTheme.colorScheme.error) }
    browser.notEmpty?.let {
        Text(stringResource(R.string.storage_folder_not_empty, it.name), color = MaterialTheme.colorScheme.error)
    }
}

/** New folder, Refresh, and Set up, which needs a chosen folder. */
@Composable
private fun FolderActions(
    browser: FolderBrowser,
    shown: FolderListing,
    onSetUp: (BridgeStorageFolder) -> Unit,
) {
    val scope = rememberScreenScope()
    var naming by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        OutlinedButton(onClick = { naming = true }, enabled = !browser.busy) {
            Text(stringResource(R.string.storage_new_folder))
        }
        OutlinedButton(onClick = { scope.launch { browser.openTop() } }, enabled = !browser.busy) {
            Text(stringResource(R.string.storage_refresh))
        }
        PrimaryButton(
            enabled = browser.target != null && !browser.busy,
            onClick = { onSetUp(checkNotNull(browser.target) { "Set up needs a chosen folder" }) },
        ) {
            Text(stringResource(R.string.storage_set_up))
        }
    }
    if (naming) {
        NewFolderDialog(
            onCreate = { name ->
                naming = false
                scope.launch { browser.create(shown, name) }
            },
            onDismiss = { naming = false },
        )
    }
}

/** What a folder choice has in flight, named, or [setup]'s setup while it sets up, with its Cancel. */
@Composable
private fun FolderWorkLine(
    work: FolderWork?,
    words: FolderWords,
    setup: StorageSetupRunner?,
    onCancel: () -> Unit,
) {
    when (work) {
        FolderWork.LISTING -> {
            WaitingIndicator(text = stringResource(words.reading))
        }

        FolderWork.CREATING -> {
            WaitingIndicator(text = stringResource(R.string.storage_creating_folder))
        }

        FolderWork.SETTING_UP -> {
            setup?.let { StorageSetupRunLine(it, onCancel) }
        }

        null -> {}
    }
}

/** A folder: tapping chooses it, marked once chosen, and where folders open into others its arrow opens it. */
@Composable
private fun FolderRow(
    folder: BridgeStorageFolder,
    chosen: Boolean,
    opens: Boolean,
    enabled: Boolean,
    onChoose: () -> Unit,
    onOpen: () -> Unit,
) {
    val marked = if (chosen) Modifier.background(MaterialTheme.colorScheme.secondaryContainer) else Modifier
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .then(marked)
                .clickable(enabled = enabled, onClick = onChoose)
                .padding(vertical = ThemeSpace.related),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.Folder, contentDescription = null)
        Text(text = folder.name, modifier = Modifier.weight(1f))
        if (opens) {
            IconButton(onClick = onOpen, enabled = enabled) {
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
            }
        }
    }
}

@Composable
private fun NewFolderDialog(
    onCreate: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.storage_new_folder)) },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text(stringResource(R.string.storage_folder_name)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(onClick = { onCreate(name.trim()) }, enabled = name.isNotBlank()) {
                Text(stringResource(R.string.storage_create_folder))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}

/** The folder choice with a folder chosen, and for Dropbox the folder setup found holding other files. */
@Composable
internal fun FolderChoiceScene(dropbox: Boolean) {
    val context = LocalContext.current
    val browser =
        remember(dropbox) {
            fun folder(name: String) =
                BridgeStorageFolder(
                    name,
                    if (dropbox) {
                        BridgeStorageLocation.Dropbox(BridgeDropboxLocation.SharedFolder(name), null, null)
                    } else {
                        BridgeStorageLocation.OneDrive("drive", name, null, null)
                    },
                )
            val names = if (dropbox) listOf("Family", "Music", "Photos") else listOf("Live", "Albums", "Mixes")
            val path = if (dropbox) emptyList() else listOf(folder("OneDrive"), folder("Music"))
            FolderBrowser.staged(
                shown = FolderListing(path, names.map(::folder)),
                selected = folder(names[1]),
                notEmpty = if (dropbox) folder(names[1]) else null,
                nests = !dropbox,
                errors = LocaleErrorDisplays(context),
            )
        }
    BaeAppChrome {
        Column(
            modifier = Modifier.padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            val words = if (dropbox) DROPBOX_WORDS else ONEDRIVE_WORDS
            FolderChoiceBody(browser, words, setup = null, onCancel = {}, onSetUp = {})
        }
    }
}
