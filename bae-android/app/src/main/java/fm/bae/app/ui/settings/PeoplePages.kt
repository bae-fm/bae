package fm.bae.app.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.R
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.components.AndroidCard
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.AndroidSection
import fm.bae.app.ui.components.RowIcon
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeHistorySharing
import uniffi.bae_bridge.BridgeInviteEntry
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePersonProfile
import uniffi.bae_bridge.BridgeRequestedProfile

/** The pages of People and devices: the first, a person's, and this person's own devices. */
internal sealed interface PeoplePage {
    data object People : PeoplePage

    data class Person(
        val memberId: String,
    ) : PeoplePage

    data object Devices : PeoplePage
}

/** The page as a saveable string, which survives the screen being recreated. */
internal fun PeoplePage.key(): String =
    when (this) {
        PeoplePage.People -> "people"
        is PeoplePage.Person -> "person:$memberId"
        PeoplePage.Devices -> "devices"
    }

internal fun peoplePage(key: String): PeoplePage =
    when {
        key == "devices" -> PeoplePage.Devices
        key.startsWith("person:") -> PeoplePage.Person(key.removePrefix("person:"))
        else -> PeoplePage.People
    }

/** What this person can do to themselves on the first page: rename, choose who sees their plays, make a profile. */
internal class YouActions(
    val errors: ErrorDisplays,
    val rename: suspend (String) -> Unit,
    val setHistory: (BridgeHistorySharing) -> Unit,
    val createProfile: (String) -> Unit,
)

/** The first page with its bar: People and devices. */
@Composable
internal fun AndroidSettingsPeople(
    you: YouActions,
    people: List<BridgePerson>,
    selfIsAdmin: Boolean,
    requested: BridgeRequestedProfile?,
    invites: List<BridgeInviteEntry>,
    adminChanges: List<BridgeAdminChange>,
    callbacks: MemberCallbacks,
    inviteCallbacks: InviteCallbacks,
    offline: Boolean,
    error: DisplayError?,
    onOpen: (PeoplePage) -> Unit,
    onBack: () -> Unit,
) {
    PeopleFrame(title = stringResource(R.string.members_title), error = error, onBack = onBack) {
        PeopleHome(
            you = you,
            people = people,
            selfIsAdmin = selfIsAdmin,
            requested = requested,
            invites = invites,
            adminChanges = adminChanges,
            callbacks = callbacks,
            inviteCallbacks = inviteCallbacks,
            offline = offline,
            onOpen = onOpen,
        )
    }
}

/**
 * The first page: You (name, role, listening history, devices) and People, one row for each other person, with the
 * invites this device has open. A person's row opens their page only for an admin: a member has nothing to do there.
 */
@Composable
internal fun PeopleHome(
    you: YouActions,
    people: List<BridgePerson>,
    selfIsAdmin: Boolean,
    requested: BridgeRequestedProfile?,
    invites: List<BridgeInviteEntry>,
    adminChanges: List<BridgeAdminChange>,
    callbacks: MemberCallbacks,
    inviteCallbacks: InviteCallbacks,
    offline: Boolean,
    onOpen: (PeoplePage) -> Unit,
) {
    val me = people.firstOrNull { it.membership.isSelf }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        // A section heading first on the page counts on a gap above it, so the page starts 8dp, not 16, from the bar.
        contentPadding =
            PaddingValues(
                start = ThemeSpace.edge,
                top = ThemeSpace.related,
                end = ThemeSpace.edge,
                bottom = ThemeSpace.edge,
            ),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        val shownChanges = adminChanges.at(AdminChangePlace.People)
        if (shownChanges.isNotEmpty()) {
            item { AdminChangesList(changes = shownChanges, onDismiss = callbacks.onDismissChange) }
        }
        item { AndroidSection(text = stringResource(R.string.members_you)) }
        item { YouCard(you = you, me = me, requested = requested, onOpenDevices = { onOpen(PeoplePage.Devices) }) }
        item { AndroidSection(text = stringResource(R.string.people_people)) }
        item {
            PeopleCard(
                people = people,
                selfIsAdmin = selfIsAdmin,
                invites = invites,
                callbacks = callbacks,
                inviteCallbacks = inviteCallbacks,
                offline = offline,
                onOpen = onOpen,
            )
        }
    }
}

/** The other people, one row each, then the invites this device has open and, for an admin, the Invite row. */
@Composable
private fun PeopleCard(
    people: List<BridgePerson>,
    selfIsAdmin: Boolean,
    invites: List<BridgeInviteEntry>,
    callbacks: MemberCallbacks,
    inviteCallbacks: InviteCallbacks,
    offline: Boolean,
    onOpen: (PeoplePage) -> Unit,
) {
    AndroidCard {
        people.filterNot { it.membership.isSelf }.forEach { person ->
            val member = person.membership
            AndroidRow(
                title = person.profile?.name ?: stringResource(R.string.members_unnamed),
                subtitle =
                    stringResource(
                        R.string.people_role_and_devices,
                        stringResource(member.role.labelRes()),
                        pluralStringResource(
                            R.plurals.people_device_count,
                            member.activeDevices.size,
                            member.activeDevices.size,
                        ),
                    ),
                onClick = if (selfIsAdmin) ({ onOpen(PeoplePage.Person(member.id)) }) else null,
                icon = if (selfIsAdmin) RowIcon.CHEVRON else null,
            )
        }
        invites.forEach { entry ->
            when (entry) {
                is BridgeInviteEntry.Open -> {
                    InviteRow(invite = entry.invite, offline = offline, callbacks = inviteCallbacks)
                }

                is BridgeInviteEntry.Refused -> {
                    RefusedInviteRow(refused = entry.refused, callbacks = inviteCallbacks)
                }
            }
        }
        if (selfIsAdmin) {
            AndroidRow(
                title = stringResource(R.string.members_invite),
                action = true,
                leadingIcon = Icons.Filled.Add,
                onClick = callbacks.onInvite,
            )
        }
    }
}

/** This person's name, role, listening history and devices; before they have a profile, the form that makes it. */
@Composable
private fun YouCard(
    you: YouActions,
    me: BridgePerson?,
    requested: BridgeRequestedProfile?,
    onOpenDevices: () -> Unit,
) {
    var renaming by rememberSaveable { mutableStateOf(false) }
    var choosingHistory by rememberSaveable { mutableStateOf(false) }
    val profile = me?.profile
    AndroidCard {
        if (profile != null) {
            AndroidRow(
                title = stringResource(R.string.invite_name),
                subtitle = profile.name,
                onClick = { renaming = true },
                icon = RowIcon.EDIT,
            )
        }
        me?.let { person ->
            AndroidRow(
                title = stringResource(R.string.people_role),
                subtitle = stringResource(person.membership.role.labelRes()),
            )
        }
        if (profile != null) {
            AndroidRow(
                title = stringResource(R.string.settings_history),
                subtitle = stringResource(profile.historySharing.labelRes()),
                onClick = { choosingHistory = true },
            )
        }
        me?.let { person -> DevicesRow(person.membership, onOpenDevices) }
    }
    if (me != null && profile == null) {
        ProfileSetup(requested = requested, onCreate = you.createProfile)
    }
    profile?.let { shown ->
        YouDialogs(
            you = you,
            profile = shown,
            renaming = renaming,
            choosingHistory = choosingHistory,
            onClose = {
                renaming = false
                choosingHistory = false
            },
        )
    }
}

/** The row that opens this person's own devices, with how many there are. */
@Composable
private fun DevicesRow(
    member: BridgeMember,
    onOpen: () -> Unit,
) {
    AndroidRow(
        title = stringResource(R.string.people_devices),
        subtitle =
            pluralStringResource(
                R.plurals.people_device_count,
                member.activeDevices.size,
                member.activeDevices.size,
            ),
        onClick = onOpen,
        icon = RowIcon.CHEVRON,
    )
}

/** The rename dialog, or the listening history dialog, when one is asked for. */
@Composable
private fun YouDialogs(
    you: YouActions,
    profile: BridgePersonProfile,
    renaming: Boolean,
    choosingHistory: Boolean,
    onClose: () -> Unit,
) {
    if (renaming) {
        RenameProfileDialog(you = you, current = profile.name, onDismiss = onClose)
    }
    if (choosingHistory) {
        ListeningHistoryDialog(
            current = profile.historySharing,
            onChoose = { chosen ->
                onClose()
                if (chosen != profile.historySharing) you.setHistory(chosen)
            },
            onDismiss = onClose,
        )
    }
}

internal fun BridgeHistorySharing.labelRes(): Int =
    when (this) {
        BridgeHistorySharing.OFF -> R.string.settings_history_off
        BridgeHistorySharing.PRIVATE -> R.string.settings_history_private
        BridgeHistorySharing.HOUSEHOLD -> R.string.settings_history_household
    }
