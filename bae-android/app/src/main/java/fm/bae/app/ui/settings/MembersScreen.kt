package fm.bae.app.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.LocaleErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorDetailDisclosure
import fm.bae.app.ui.components.PrimaryButton
import fm.bae.app.ui.components.StatusChip
import fm.bae.app.ui.rememberScreenScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeMembership
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePersonProfile
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.InvitesCallback
import uniffi.bae_bridge.MembershipCallback
import uniffi.bae_bridge.ProfilesCallback

private const val TAG = "bae.MembersScreen"
private val logger = BaeLogger(TAG)

/** The read of the library's people: running, answered, or failed with what to show. */
internal sealed interface PeopleRead {
    data object Loading : PeopleRead

    data class Loaded(
        val people: List<BridgePerson>,
    ) : PeopleRead

    /** The read failed; the list is not known, so none is shown, and Retry reads it again. */
    data class Failed(
        val error: DisplayError,
    ) : PeopleRead
}

/**
 * The library's people as core joins them: each member with their profile.
 * Membership and profiles each arrive live; either change reads the joined
 * list again.
 */
internal class MembersModel(
    private val appHandle: AppHandleInterface,
    private val errors: ErrorDisplays,
    private val scope: CoroutineScope,
) {
    var people by mutableStateOf<PeopleRead>(PeopleRead.Loading)
        private set
    var selfIsAdmin by mutableStateOf(false)
        private set

    /** This device's open invites, each with a sent invite's request once it arrives. */
    var invites by mutableStateOf<List<BridgeInvite>>(emptyList())
        private set
    var storage by mutableStateOf<BridgeStorageLocation?>(null)
        private set
    var error by mutableStateOf<DisplayError?>(null)
    private var reloading: Job? = null

    /** Read the joined list again; a newer read replaces one still running. */
    fun reload() {
        reloading?.cancel()
        reloading =
            scope.launch {
                people =
                    try {
                        PeopleRead.Loaded(appHandle.people())
                    } catch (e: CancellationException) {
                        throw e
                    } catch (e: BridgeException) {
                        logger.error("reading the library's people failed", e)
                        PeopleRead.Failed(errors.display(e) ?: DisplayError(e.toString()))
                    }
            }
    }

    /** Read the list again after a failed read, showing the spinner while it runs. */
    fun retry() {
        people = PeopleRead.Loading
        reload()
    }

    suspend fun loadStorage() {
        performBridgeAction(logger, "read the library's cloud home", errors, { error = it }) {
            storage = appHandle.storageLocation()
        }
    }

    fun subscribe(): () -> Unit {
        val members =
            appHandle.subscribeMembers(
                object : MembershipCallback {
                    override fun onValue(value: BridgeMembership) {
                        scope.launch(Dispatchers.Main.immediate) {
                            selfIsAdmin = value.selfIsAdmin
                            reload()
                        }
                    }

                    override fun onError(error: BridgeException) {
                        logger.error("reading the library's members failed", error)
                        scope.launch(Dispatchers.Main.immediate) {
                            this@MembersModel.error = errors.display(error)
                        }
                    }
                },
            )
        val profiles =
            appHandle.subscribeProfiles(
                object : ProfilesCallback {
                    override fun onValue(value: List<BridgePersonProfile>) {
                        scope.launch(Dispatchers.Main.immediate) { reload() }
                    }

                    override fun onError(error: BridgeException) {
                        logger.error("reading the library's profiles failed", error)
                        scope.launch(Dispatchers.Main.immediate) {
                            this@MembersModel.error = errors.display(error)
                        }
                    }
                },
            )
        val open =
            appHandle.subscribeInvites(
                object : InvitesCallback {
                    override fun onValue(value: List<BridgeInvite>) {
                        scope.launch(Dispatchers.Main.immediate) { invites = value }
                    }
                },
            )
        return {
            members.cancel()
            profiles.cancel()
            open.cancel()
        }
    }

    /** Run a membership change, showing its failure above the list; the subscriptions show its effect. */
    fun perform(
        operation: String,
        action: suspend () -> Unit,
    ) {
        error = null
        scope.launch {
            performBridgeAction(logger, operation, errors, { error = it }, action)
        }
    }
}

/**
 * Everyone in the library and their devices, and the invites this device has
 * open. Admins invite people, change roles and remove people; each person
 * adds and removes their own devices, and an admin removes anyone's.
 */
@Composable
fun MembersScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val appContext = LocalContext.current.applicationContext
    val scope = rememberScreenScope()
    val model = remember(session) { MembersModel(session.appHandle, LocaleErrorDisplays(appContext), scope) }
    DisposableEffect(model) { onDispose(model.subscribe()) }
    LaunchedEffect(model) { model.loadStorage() }
    val dialogs = remember { MembersDialogs() }

    Column(modifier = Modifier.fillMaxSize()) {
        MembersTopBar(onBack = onBack)
        model.error?.let { shown ->
            ErrorDetailDisclosure(
                error = shown,
                modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
            )
        }
        when (val read = model.people) {
            PeopleRead.Loading -> {
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator()
                }
            }

            is PeopleRead.Failed -> {
                Column(
                    modifier = Modifier.fillMaxSize().padding(ThemeSpace.page),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center,
                ) {
                    ErrorDetailDisclosure(read.error)
                    TextButton(onClick = model::retry) { Text(stringResource(R.string.retry)) }
                }
            }

            is PeopleRead.Loaded -> {
                MembersList(
                    people = read.people,
                    selfIsAdmin = model.selfIsAdmin,
                    invites = model.invites,
                    callbacks = memberCallbacks(session, model, dialogs),
                    inviteCallbacks = inviteCallbacks(session, dialogs),
                )
            }
        }
    }
    MembersDialogsShown(session = session, model = model, dialogs = dialogs)
}

private fun memberCallbacks(
    session: OpenLibrary,
    model: MembersModel,
    dialogs: MembersDialogs,
): MemberCallbacks {
    val handle = session.appHandle
    return MemberCallbacks(
        onInvite = { dialogs.inviting = true },
        onSetRole = { member, role -> model.perform("set member role") { handle.setMemberRole(member.id, role) } },
        onRemovePerson = { member, name -> dialogs.pendingRemoval = PendingRemoval.Person(member, name) },
        onRemoveDevice = { dialogs.pendingRemoval = PendingRemoval.Device(it) },
        onAddDevice = { dialogs.addingDevice = true },
    )
}

private fun inviteCallbacks(
    session: OpenLibrary,
    dialogs: MembersDialogs,
): InviteCallbacks =
    InviteCallbacks(
        onOpen = dialogs::open,
        // Retry Now reads a request that has arrived at once, instead of at the next check.
        onRefresh = { session.messages.startRetrySync(session.appHandle::retryNow) },
    )

/** What this device's person can do to the library's members and their devices. */
private class MemberCallbacks(
    val onInvite: () -> Unit,
    val onSetRole: (BridgeMember, BridgeMemberRole) -> Unit,
    val onRemovePerson: (BridgeMember, String) -> Unit,
    val onRemoveDevice: (BridgeActiveDevice) -> Unit,
    val onAddDevice: () -> Unit,
)

/** What this device's person can do with an invite it has open. */
private class InviteCallbacks(
    val onOpen: (BridgeInvite) -> Unit,
    val onRefresh: () -> Unit,
)

@Composable
private fun MembersTopBar(onBack: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(ThemeSpace.related),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
            }
            Text(
                text = stringResource(R.string.members_title),
                style = ThemeText.title.style,
            )
        }
    }
}

@Composable
private fun MembersList(
    people: List<BridgePerson>,
    selfIsAdmin: Boolean,
    invites: List<BridgeInvite>,
    callbacks: MemberCallbacks,
    inviteCallbacks: InviteCallbacks,
) {
    // A removed person's profile stays, read-only, without a membership; only members are listed.
    val members = people.mapNotNull { person -> person.membership?.let { person to it } }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(ThemeSpace.edge),
        verticalArrangement = Arrangement.spacedBy(ThemeSpace.related),
    ) {
        items(members, key = { (person, _) -> person.id }) { (person, member) ->
            HorizontalDivider()
            MemberRow(
                name = person.profile?.name,
                member = member,
                selfIsAdmin = selfIsAdmin,
                callbacks = callbacks,
            )
        }
        items(invites, key = { invite -> invite.id }) { invite ->
            HorizontalDivider()
            InviteRow(invite = invite, callbacks = inviteCallbacks)
        }
        if (selfIsAdmin) {
            item {
                PrimaryButton(onClick = callbacks.onInvite) {
                    Text(stringResource(R.string.members_invite))
                }
            }
        }
    }
}

/** One of this device's open invites: the person's name, marked invited, and when it expires. */
@Composable
private fun InviteRow(
    invite: BridgeInvite,
    callbacks: InviteCallbacks,
) {
    val context = LocalContext.current
    val expires = remember(invite.expiresAt) { context.inviteExpiry(invite.expiresAt) }
    Row(
        modifier = Modifier.fillMaxWidth().clickable(role = Role.Button) { callbacks.onOpen(invite) },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
            ) {
                Text(
                    text = invite.invitee ?: stringResource(R.string.members_unnamed),
                    style = ThemeText.strong.style,
                    modifier = Modifier.weight(1f, fill = false),
                )
                StatusChip(stringResource(R.string.members_invited))
            }
            Text(
                text = stringResource(R.string.invite_expires, expires),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        IconButton(onClick = callbacks.onRefresh) {
            Icon(Icons.Filled.Refresh, contentDescription = stringResource(R.string.members_check_invites))
        }
    }
}

/**
 * One person: their name and role, their active devices, and what this
 * device's person may change, with Add device under this device's own person
 * and their removed devices collapsed below.
 */
@Composable
private fun MemberRow(
    name: String?,
    member: BridgeMember,
    selfIsAdmin: Boolean,
    callbacks: MemberCallbacks,
) {
    val shownName = name ?: stringResource(R.string.members_unnamed)
    val context = LocalContext.current
    Column(verticalArrangement = Arrangement.spacedBy(ThemeSpace.inline)) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related),
        ) {
            Text(text = shownName, style = ThemeText.strong.style, modifier = Modifier.weight(1f, fill = false))
            StatusChip(stringResource(member.role.labelRes()))
            if (member.isSelf) {
                StatusChip(stringResource(R.string.members_you))
            }
        }
        (member.access as? BridgeMemberAccess.S3AccessKey)?.let { key ->
            Text(
                text = context.coreString("core.access_key.id", mapOf("key" to key.accessKeyId)),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        member.activeDevices.forEach { device ->
            DeviceRow(device = device, onRemove = { callbacks.onRemoveDevice(device) })
        }
        if (member.isSelf) {
            TextButton(onClick = callbacks.onAddDevice, modifier = Modifier.padding(start = ThemeSpace.edge)) {
                Text(stringResource(R.string.members_add_device))
            }
        }
        if (member.formerDevices.isNotEmpty()) {
            FormerDevices(devices = member.formerDevices)
        }
        if (selfIsAdmin && !member.isSelf) {
            Row(horizontalArrangement = Arrangement.spacedBy(ThemeSpace.related)) {
                val (nextRole, label) =
                    when (member.role) {
                        BridgeMemberRole.ADMIN -> BridgeMemberRole.MEMBER to R.string.members_make_member
                        BridgeMemberRole.MEMBER -> BridgeMemberRole.ADMIN to R.string.members_make_admin
                    }
                TextButton(onClick = { callbacks.onSetRole(member, nextRole) }) {
                    Text(stringResource(label))
                }
                if (member.canRemove) {
                    TextButton(
                        onClick = { callbacks.onRemovePerson(member, shownName) },
                        colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
                    ) {
                        Text(stringResource(R.string.members_remove_person))
                    }
                }
            }
        }
    }
}

/** A device that can sync, removable when this device's person may remove it. */
@Composable
private fun DeviceRow(
    device: BridgeActiveDevice,
    onRemove: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = ThemeSpace.edge),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(text = device.name)
            if (device.isSelf) {
                Text(
                    text = stringResource(R.string.members_this_device),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        if (device.canRemove) {
            TextButton(
                onClick = onRemove,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) {
                Text(stringResource(R.string.members_remove))
            }
        }
    }
}

private fun BridgeMemberRole.labelRes(): Int =
    when (this) {
        BridgeMemberRole.ADMIN -> R.string.members_role_admin
        BridgeMemberRole.MEMBER -> R.string.members_role_member
    }

@Preview(showBackground = true)
@Composable
private fun MembersListPreview() {
    BaeTheme {
        MembersList(
            people = listOf(PreviewData.person()),
            selfIsAdmin = true,
            invites = emptyList(),
            callbacks = MemberCallbacks({}, { _, _ -> }, { _, _ -> }, {}, {}),
            inviteCallbacks = InviteCallbacks({}, {}),
        )
    }
}
