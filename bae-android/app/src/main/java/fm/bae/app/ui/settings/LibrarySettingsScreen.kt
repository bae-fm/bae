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
import fm.bae.app.LocalBugReports
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.data.LibraryWithoutStorage
import fm.bae.app.data.SyncLine
import fm.bae.app.factSeparator
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidScreen
import fm.bae.app.ui.components.RowIcon
import fm.bae.app.ui.components.SomethingWentWrongNotice
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.library.LibraryDestination
import fm.bae.app.ui.rememberScreenScope
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeAdminChangePlace
import uniffi.bae_bridge.BridgeAdminChanges
import uniffi.bae_bridge.BridgeRatedItems
import uniffi.bae_bridge.BridgeRatingSystem
import uniffi.bae_bridge.BridgeRatingSystems

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
    val libraryName = rememberLibraryNameRead(session)
    val ratingSystems = rememberRatingSystemsRead(session)
    val members = remember(session) { MembersModel(session.appHandle, session.errors, scope) }
    DisposableEffect(members) { onDispose(members.subscribe()) }
    LaunchedEffect(members) { members.adminChanges.show(BridgeAdminChangePlace.Library) }
    val line by session.syncStatusStore.line.collectAsState()
    val config by session.configStore.config.collectAsState()
    val hasStorage = config.storage != null
    val bugs = LocalBugReports.current
    // A phone joins a library and never makes one, so a library without storage is a bug, said in place.
    LaunchedEffect(session, hasStorage) {
        if (!hasStorage) bugs.reportShownInPlace("show a library's storage", LibraryWithoutStorage())
    }
    var renaming by remember { mutableStateOf(false) }
    val name = libraryName.value
    AndroidSettingsLibrary(
        name = name,
        canChange = members.selfIsAdmin,
        sync = syncRowSummary(line),
        peopleAndDevices = if (hasStorage) peopleAndDevices(members.people) else null,
        hasStorage = hasStorage,
        adminChanges =
            members.adminChangesAt(BridgeAdminChangePlace.Library),
        onDismissChange = { change ->
            members.perform("dismiss an admin change") { session.appHandle.acknowledgeAdminChange(change.operation) }
        },
        ratingSystems = { ratingSystems.value },
        onChangeRatingSystem = { items, system ->
            session.messages.performAndWait("set the rating system") {
                session.appHandle.setRatingSystem(items, system)
            }
        },
        onBack = onBack,
        onRename = { renaming = true },
        onOpenSync = { onPush(LibraryDestination.Sync) },
        onOpenPeople = { onPush(LibraryDestination.Members()) },
    ) {
        if (!hasStorage) SomethingWentWrongNotice()
        LibraryReadFailures(libraryName, ratingSystems, members)
    }
    if (renaming && name != null) {
        RenameLibraryDialog(session = session, current = name, onDismiss = { renaming = false })
    }
}

/**
 * The Library page over what it shows: a [name] with an edit mark when [canChange], the one permission to rename the
 * library and to change its rating systems; null while the name is read, when no row shows. The Ratings card shows
 * [ratingSystems] once they are read, and opens its choices only when [canChange]. [aboveRows] holds the failures of
 * the reads behind it.
 */
@Composable
internal fun AndroidSettingsLibrary(
    name: String?,
    canChange: Boolean,
    sync: SyncRowSummary,
    peopleAndDevices: String?,
    hasStorage: Boolean,
    adminChanges: BridgeAdminChanges?,
    onDismissChange: (BridgeAdminChange) -> Unit,
    ratingSystems: () -> BridgeRatingSystems?,
    onChangeRatingSystem: suspend (BridgeRatedItems, BridgeRatingSystem) -> Unit,
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
                        onClick = if (canChange) onRename else null,
                        onClickLabel = stringResource(R.string.settings_rename_library),
                        icon = if (canChange) RowIcon.EDIT else null,
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
            RatingSystemsCard(ratingSystems, canChange = canChange, onChange = onChangeRatingSystem)
            AdminChangesList(changes = adminChanges, onDismiss = onDismissChange)
        }
    }
}

/** Shows why the screen could not read the name, the rating systems or the people, each with Retry. */
@Composable
private fun LibraryReadFailures(
    libraryName: SettingsRead<String>,
    ratingSystems: SettingsRead<BridgeRatingSystems>,
    members: MembersModel,
) {
    (libraryName.state as? SettingsReadState.Failed)?.let {
        ReadFailure(error = it.error, onRetry = libraryName::watch)
    }
    (ratingSystems.state as? SettingsReadState.Failed)?.let {
        ReadFailure(error = it.error, onRetry = ratingSystems::watch)
    }
    (members.people as? PeopleRead.Failed)?.let { ReadFailure(error = it.error, onRetry = members::retry) }
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
    val peopleCount = loaded.people.toInt()
    val devices = loaded.devices.toInt()
    val peopleText = pluralStringResource(R.plurals.people_count, peopleCount, peopleCount)
    val devicesText = pluralStringResource(R.plurals.device_count, devices, devices)
    return "$peopleText${LocalContext.current.factSeparator()}$devicesText"
}
