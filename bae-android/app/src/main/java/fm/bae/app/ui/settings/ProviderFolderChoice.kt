package fm.bae.app.ui.settings

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
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.OAuthProviders
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.WaitingIndicator
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeStorageFolder

private val logger = BaeLogger("bae.ProviderFolderChoice")

/**
 * The Dropbox folders shared with the signed-in account; choosing one makes
 * it the library's cloud home, where the household's members can reach it.
 * [setup] runs the setup, which the person can stop with Cancel, calling
 * [onCancel].
 */
@Composable
internal fun DropboxFolderChoice(
    session: OpenLibrary,
    providers: OAuthProviders,
    setup: StorageSetupRunner,
    onError: (DisplayError?) -> Unit,
    onCancel: () -> Unit,
) {
    val scope = rememberScreenScope()
    var folders by remember { mutableStateOf<List<BridgeStorageFolder>?>(null) }
    var busy by remember { mutableStateOf(false) }
    val load: suspend () -> Unit = {
        folders = null
        performBridgeAction(logger, "list Dropbox shared folders", session.errors, onError) {
            folders = providers.dropboxSharedFolders(session.appHandle)
        }
    }
    LaunchedEffect(session) { load() }

    Text(stringResource(R.string.storage_dropbox_choose), style = ThemeText.heading.style)
    when (val listed = folders) {
        null -> {
            WaitingIndicator(text = stringResource(R.string.storage_dropbox_reading))
        }

        else -> {
            if (listed.isEmpty()) {
                Text(stringResource(R.string.storage_dropbox_none))
            }
            listed.forEach { folder ->
                FolderRow(folder = folder, enabled = !busy) {
                    busy = true
                    onError(null)
                    scope.launch {
                        setup.run("set up Dropbox storage") { deviceName, bridgeSetup ->
                            providers.setUpStorage(session.appHandle, folder.location, deviceName, bridgeSetup)
                        }
                        busy = false
                    }
                }
            }
            StorageSetupRunLine(setup, onCancel)
            OutlinedButton(onClick = { scope.launch { load() } }, enabled = !busy) {
                Text(stringResource(R.string.storage_refresh))
            }
        }
    }
}

/**
 * The signed-in OneDrive, browsed from its top: open a folder to go inside,
 * make a new one, and use the folder shown as the library's cloud home. A
 * listing that fails shows its failure and Retry here, where the person is.
 * [setup] runs the setup, which the person can stop with Cancel, calling
 * [onCancel].
 */
@Composable
internal fun OneDriveFolderChoice(
    session: OpenLibrary,
    providers: OAuthProviders,
    setup: StorageSetupRunner,
    onError: (DisplayError?) -> Unit,
    onCancel: () -> Unit,
) {
    val scope = rememberScreenScope()
    val browser = remember(session) { OneDriveBrowser(session.appHandle, providers, session.errors) }
    var naming by remember { mutableStateOf(false) }
    LaunchedEffect(browser) { browser.openRoot() }

    val shown = browser.shown
    if (shown != null) {
        OneDriveHeading(shown, enabled = !browser.busy) { scope.launch { browser.up(shown) } }
    }
    browser.failure?.let { failure ->
        failure.error?.let { ErrorDetailDisclosure(it) }
        TextButton(onClick = { scope.launch { failure.retry() } }, enabled = !browser.busy) {
            Text(stringResource(R.string.retry))
        }
    }
    OneDriveWorkLine(browser.work, setup, onCancel)
    if (shown == null) return
    shown.children.forEach { folder ->
        FolderRow(folder = folder, enabled = !browser.busy) { scope.launch { browser.open(shown, folder) } }
    }
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        OutlinedButton(onClick = { naming = true }, enabled = !browser.busy) {
            Text(stringResource(R.string.storage_onedrive_new_folder))
        }
        PrimaryButton(
            enabled = !browser.busy,
            onClick = {
                browser.work = OneDriveWork.SETTING_UP
                onError(null)
                scope.launch {
                    setup.run("set up OneDrive storage") { deviceName, bridgeSetup ->
                        providers.setUpStorage(session.appHandle, shown.folder.location, deviceName, bridgeSetup)
                    }
                    browser.work = null
                }
            },
        ) {
            Text(stringResource(R.string.storage_onedrive_use_folder))
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

/** What a OneDrive folder choice has in flight, named, or [setup]'s setup while it sets up, with its Cancel. */
@Composable
private fun OneDriveWorkLine(
    work: OneDriveWork?,
    setup: StorageSetupRunner,
    onCancel: () -> Unit,
) {
    when (work) {
        OneDriveWork.LISTING -> {
            WaitingIndicator(text = stringResource(R.string.storage_onedrive_reading))
        }

        OneDriveWork.CREATING -> {
            WaitingIndicator(text = stringResource(R.string.storage_onedrive_creating))
        }

        OneDriveWork.SETTING_UP -> {
            StorageSetupRunLine(setup, onCancel)
        }

        null -> {}
    }
}

/** The folder shown, with Back to the one it was opened from below the top. */
@Composable
private fun OneDriveHeading(
    shown: OneDriveListing,
    enabled: Boolean,
    onUp: () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (shown.path.size > 1) {
            TextButton(onClick = onUp, enabled = enabled) {
                Text(stringResource(R.string.back))
            }
        }
        Text(text = shown.folder.name, style = ThemeText.heading.style, modifier = Modifier.weight(1f))
    }
}

@Composable
private fun FolderRow(
    folder: BridgeStorageFolder,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .clickable(enabled = enabled, onClick = onClick)
                .padding(vertical = ThemeSpace.related),
        horizontalArrangement = Arrangement.spacedBy(ThemeSpace.group),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Filled.Folder, contentDescription = null)
        Text(text = folder.name, modifier = Modifier.weight(1f))
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
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
        title = { Text(stringResource(R.string.storage_onedrive_new_folder)) },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text(stringResource(R.string.storage_onedrive_folder_name)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            TextButton(onClick = { onCreate(name.trim()) }, enabled = name.isNotBlank()) {
                Text(stringResource(R.string.storage_onedrive_create))
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
        },
    )
}
