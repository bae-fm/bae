package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
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
import fm.bae.app.ui.components.RowIcon
import fm.bae.app.ui.components.SomethingWentWrongNotice
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.library.LibraryDestination
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.launch
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeMemberRole

/**
 * The open library: its name, with Rename for an admin, and rows that show where its sync and its people and devices
 * stand and open their pages.
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
    val storage = model.storage
    var renaming by remember { mutableStateOf(false) }
    val name = (model.name as? LibraryNameRead.Named)?.name
    AndroidSettingsLibrary(
        name = name,
        canRename = (model.self as? SelfRead.Loaded)?.person?.membership?.role == BridgeMemberRole.ADMIN,
        sync = syncRowSummary(line),
        peopleAndDevices = if (storage is Storage.At) peopleAndDevices(members.people) else null,
        hasStorage = storage is Storage.At,
        adminChanges = members.adminChanges.at(AdminChangePlace.Library),
        onDismissChange = { change ->
            members.perform("dismiss an admin change") { session.appHandle.acknowledgeAdminChange(change.operation) }
        },
        onBack = onBack,
        onRename = { renaming = true },
        onOpenSync = { onPush(LibraryDestination.Sync) },
        onOpenPeople = { onPush(LibraryDestination.Members) },
    ) {
        if (storage is Storage.Missing) SomethingWentWrongNotice()
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
    adminChanges: List<BridgeAdminChange>,
    onDismissChange: (BridgeAdminChange) -> Unit,
    onBack: () -> Unit,
    onRename: () -> Unit,
    onOpenSync: () -> Unit,
    onOpenPeople: () -> Unit,
    aboveRows: @Composable ColumnScope.() -> Unit = {},
) {
    AndroidScreen(
        title = stringResource(R.string.settings_library),
        onBack = onBack,
        modifier = Modifier.fillMaxSize(),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(ThemeSpace.edge),
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
                        icon = if (canRename) RowIcon.EDIT else null,
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
                }
            }
            AdminChangesList(changes = adminChanges, onDismiss = onDismissChange)
        }
    }
}

/** Why the storage, the name or the person couldn't be read, each with Retry. */
@Composable
private fun LibraryNameFailures(model: SettingsLibraryModel) {
    val scope = rememberScreenScope()
    (model.storage as? Storage.Failed)?.let {
        ReadFailure(error = it.error, onRetry = { scope.launch { model.loadStorage() } })
    }
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

/** The Sync row's state: the state's words in the color core gives it. */
@Composable
internal fun syncRowSummary(line: SyncLine): SyncRowSummary {
    val context = LocalContext.current
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
