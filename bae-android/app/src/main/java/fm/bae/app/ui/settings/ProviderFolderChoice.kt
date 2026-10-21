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
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OAuthProviders
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeStorageFolder
import uniffi.bae_bridge.BridgeStorageSetup

private val logger = BaeLogger("bae.ProviderFolderChoice")

/**
 * The Dropbox folders shared with the signed-in account; choosing one makes
 * it the library's cloud home, where the household's members can reach it.
 */
@Composable
internal fun DropboxFolderChoice(
    session: OpenLibrary,
    providers: OAuthProviders,
    onError: (DisplayError?) -> Unit,
    onSetUp: (BridgeStorageSetup) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    var folders by remember { mutableStateOf<List<BridgeStorageFolder>?>(null) }
    var busy by remember { mutableStateOf(false) }
    val load: suspend () -> Unit = {
        folders = null
        performBridgeAction(logger, "list Dropbox shared folders", LocaleErrorDisplays(context), onError) {
            folders = providers.dropboxSharedFolders(session.appHandle)
        }
    }
    LaunchedEffect(session) { load() }

    Text(stringResource(R.string.storage_dropbox_choose), style = ThemeText.heading.style)
    when (val listed = folders) {
        null -> {
            CircularProgressIndicator()
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
                        runSetup(context, "set up Dropbox storage", onError, onSetUp) { deviceName ->
                            providers.setUpStorage(session.appHandle, folder.location, deviceName)
                        }
                        busy = false
                    }
                }
            }
            OutlinedButton(onClick = { scope.launch { load() } }, enabled = !busy) {
                Text(stringResource(R.string.storage_refresh))
            }
        }
    }
}

/** Where browsing the signed-in OneDrive stands: the folders opened from its top, and what the last holds. */
private class OneDriveBrowser(
    private val session: OpenLibrary,
    private val providers: OAuthProviders,
    private val errors: LocaleErrorDisplays,
    private val onError: (DisplayError?) -> Unit,
) {
    /** The folders opened from the top, the last being the one shown. */
    val path = mutableStateListOf<BridgeStorageFolder>()
    var children by mutableStateOf<List<BridgeStorageFolder>?>(null)
        private set
    var busy by mutableStateOf(false)

    suspend fun openRoot() {
        performBridgeAction(logger, "open the OneDrive root", errors, onError) {
            open(providers.oneDriveRoot(session.appHandle))
        }
    }

    suspend fun open(folder: BridgeStorageFolder) {
        children = null
        path.add(folder)
        performBridgeAction(logger, "list OneDrive folders", errors, onError) {
            children = providers.oneDriveFolders(session.appHandle, folder.location)
        }
    }

    suspend fun up() {
        path.removeAt(path.lastIndex)
        open(path.removeAt(path.lastIndex))
    }

    suspend fun create(name: String) {
        val parent = path.last()
        busy = true
        onError(null)
        performBridgeAction(logger, "create a OneDrive folder", errors, onError) {
            open(providers.createOneDriveFolder(session.appHandle, parent.location, name))
        }
        busy = false
    }
}

/**
 * The signed-in OneDrive, browsed from its top: open a folder to go inside,
 * make a new one, and use the folder shown as the library's cloud home.
 */
@Composable
internal fun OneDriveFolderChoice(
    session: OpenLibrary,
    providers: OAuthProviders,
    onError: (DisplayError?) -> Unit,
    onSetUp: (BridgeStorageSetup) -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberScreenScope()
    val browser = remember(session) { OneDriveBrowser(session, providers, LocaleErrorDisplays(context), onError) }
    var naming by remember { mutableStateOf(false) }
    LaunchedEffect(browser) { browser.openRoot() }

    val current = browser.path.lastOrNull() ?: return
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (browser.path.size > 1) {
            TextButton(onClick = { scope.launch { browser.up() } }, enabled = !browser.busy) {
                Text(stringResource(R.string.back))
            }
        }
        Text(text = current.name, style = ThemeText.heading.style, modifier = Modifier.weight(1f))
    }
    browser.children?.forEach { folder ->
        FolderRow(folder = folder, enabled = !browser.busy) { scope.launch { browser.open(folder) } }
    } ?: CircularProgressIndicator()
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
        OutlinedButton(onClick = { naming = true }, enabled = !browser.busy) {
            Text(stringResource(R.string.storage_onedrive_new_folder))
        }
        PrimaryButton(
            enabled = !browser.busy,
            onClick = {
                browser.busy = true
                onError(null)
                scope.launch {
                    runSetup(context, "set up OneDrive storage", onError, onSetUp) { deviceName ->
                        providers.setUpStorage(session.appHandle, current.location, deviceName)
                    }
                    browser.busy = false
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
                scope.launch { browser.create(name) }
            },
            onDismiss = { naming = false },
        )
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
