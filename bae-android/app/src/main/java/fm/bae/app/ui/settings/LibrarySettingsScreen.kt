package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.SyncLine
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.library.LibraryDestination
import fm.bae.app.ui.rememberScreenScope
import fm.bae.app.ui.storageLocationLabel
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeStorageLocation

/**
 * The open library: its name, with Rename for an admin, and rows that show where its sync, its people and devices and
 * its restore code stand and open their pages. A library with no storage has the name and a Sync row that offers
 * setting sync up; people and the restore code live in the library's store, which only storage has.
 */
@Composable
internal fun LibrarySettingsScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
    onPush: (LibraryDestination) -> Unit,
) {
    val scope = rememberScreenScope()
    val model = rememberSettingsLibraryModel(session)
    val members = remember(session) { MembersModel(session.appHandle, session.errors, scope) }
    LaunchedEffect(model) { model.load() }
    DisposableEffect(members) { onDispose(members.subscribe()) }
    val line by session.syncStatusStore.line.collectAsState()
    val setupRunning by session.syncStatusStore.setupRunning.collectAsState()
    val storage = model.storage
    var renaming by remember { mutableStateOf(false) }
    val name = (model.name as? LibraryNameRead.Named)?.name
    AndroidSettingsLibrary(
        name = name,
        canRename = (model.self as? SelfRead.Loaded)?.person?.membership?.role == BridgeMemberRole.ADMIN,
        sync = syncRowSummary(storage = storage, line = line, setupRunning = setupRunning),
        peopleAndDevices = if (storage is Storage.At) peopleAndDevices(members.people) else null,
        hasStorage = storage is Storage.At,
        onBack = onBack,
        onRename = { renaming = true },
        onOpenSync = { onPush(LibraryDestination.Sync) },
        onOpenPeople = { onPush(LibraryDestination.Members) },
        onOpenRestoreCode = { onPush(LibraryDestination.RestoreCode) },
    ) {
        LibraryNameFailures(model = model)
    }
    if (renaming && name != null) {
        RenameLibraryDialog(session = session, current = name, onDismiss = { renaming = false })
    }
}

/**
 * The Library page over what it shows: a [name] with an edit mark when [canRename]; null while the name is read, when
 * no row shows. [aboveRows] holds the failures of the reads behind it.
 */
@Composable
internal fun AndroidSettingsLibrary(
    name: String?,
    canRename: Boolean,
    sync: SyncRowSummary,
    peopleAndDevices: String?,
    hasStorage: Boolean,
    onBack: () -> Unit,
    onRename: () -> Unit,
    onOpenSync: () -> Unit,
    onOpenPeople: () -> Unit,
    onOpenRestoreCode: () -> Unit,
    aboveRows: @Composable ColumnScope.() -> Unit = {},
) {
    AndroidScreen(
        title = stringResource(R.string.settings_library),
        onBack = onBack,
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.edge),
            verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            aboveRows()
            AndroidCard {
                if (name != null) {
                    AndroidRow(
                        title = stringResource(R.string.library_name_row),
                        subtitle = name,
                        onClick = if (canRename) onRename else null,
                        onClickLabel = stringResource(R.string.settings_rename_library),
                        trailing =
                            if (canRename) {
                                { Icon(Icons.Filled.Edit, contentDescription = null) }
                            } else {
                                null
                            },
                    )
                }
                AndroidRow(
                    title = stringResource(R.string.settings_sync),
                    subtitle = sync.text,
                    tone = sync.tone,
                    onClick = onOpenSync,
                )
                if (hasStorage) {
                    AndroidRow(
                        title = stringResource(R.string.members_title),
                        subtitle = peopleAndDevices,
                        onClick = onOpenPeople,
                    )
                    AndroidRow(
                        title = stringResource(R.string.restore_code_title),
                        subtitle = stringResource(R.string.restore_code_summary),
                        onClick = onOpenRestoreCode,
                    )
                }
            }
        }
    }
}

/** Why the name or the person couldn't be read, each with Retry. */
@Composable
private fun LibraryNameFailures(model: SettingsLibraryModel) {
    val scope = rememberScreenScope()
    (model.name as? LibraryNameRead.Failed)?.let { ReadFailure(error = it.error, onRetry = model::watchName) }
    (model.self as? SelfRead.Failed)?.let {
        ReadFailure(error = it.error, onRetry = { scope.launch { model.loadSelf() } })
    }
}

/** What the Sync row says under its title, and the tone it is in. */
internal data class SyncRowSummary(
    val text: String?,
    val tone: StatusTone?,
)

/**
 * The Sync row's state: "Not synced" for a library with no storage, otherwise the state's words in the color core
 * gives it.
 */
@Composable
internal fun syncRowSummary(
    storage: Storage,
    line: SyncLine,
    setupRunning: Boolean,
): SyncRowSummary {
    val context = LocalContext.current
    if (storage is Storage.None && !setupRunning) {
        return SyncRowSummary(stringResource(R.string.library_not_synced), tone = null)
    }
    return SyncRowSummary(syncLineText(context, line), (line as? SyncLine.Read)?.shown?.tone?.statusTone())
}

/** "2 people · 3 devices", once the people have been read. */
@Composable
private fun peopleAndDevices(people: PeopleRead): String? {
    val loaded = (people as? PeopleRead.Loaded)?.people ?: return null
    val devices = loaded.sumOf { it.membership.activeDevices.size }
    val peopleText = pluralStringResource(R.plurals.people_count, loaded.size, loaded.size)
    val devicesText = pluralStringResource(R.plurals.device_count, devices, devices)
    return "$peopleText · $devicesText"
}

/** The Restore code page: the person's recovery code, shown on request. */
@Composable
internal fun RestoreCodeScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val scope = rememberScreenScope()
    val model = remember(session) { SettingsLibraryModel(session.appHandle, session.errors, scope) }
    LaunchedEffect(model) { model.loadStorage() }
    val location = (model.storage as? Storage.At)?.location
    AndroidScreen(
        title = stringResource(R.string.restore_code_title),
        onBack = onBack,
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()),
    ) {
        // The code is made once the library's storage is read, which prints on its page.
        location?.let {
            Column(modifier = Modifier.padding(ThemeSpace.edge)) {
                RestoreCodeSection(
                    needsS3Key = it is BridgeStorageLocation.S3,
                    location = storageLocationLabel(it),
                    generate = { session.appHandle.generateRestoreCode().code },
                )
            }
        }
    }
}
