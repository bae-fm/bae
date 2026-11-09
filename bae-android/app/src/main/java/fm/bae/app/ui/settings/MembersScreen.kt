package fm.bae.app.ui.settings

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.appearance.ThemeSpace
import fm.bae.app.ui.appearance.ThemeText
import fm.bae.app.ui.components.ErrorNotice
import fm.bae.app.ui.components.LoadFailure
import fm.bae.app.ui.components.StatusChip
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.rememberScreenScope
import fm.bae.screensubscription.ScreenSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteProgress
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgeRequestedProfile
import uniffi.bae_bridge.BridgeStorageLocation

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

    /** This device's open invites, each with its request once it arrives. */
    var invites by mutableStateOf<List<BridgeInvite>>(emptyList())
        private set
    var storage by mutableStateOf<BridgeStorageLocation?>(null)
        private set

    /** The profile this device's person asked for, while it isn't written yet; null before they ask and once it is. */
    var requestedProfile by mutableStateOf<BridgeRequestedProfile?>(null)
        private set

    /** The admin changes this device asked for, each until the person dismisses its outcome. */
    var adminChanges by mutableStateOf<List<BridgeAdminChange>>(emptyList())
        private set
    var error by mutableStateOf<DisplayError?>(null)
    private var reloading: Job? = null

    /** Read the joined list again; a newer read replaces one still running. */
    fun reload() {
        reloading?.cancel()
        reloading =
            scope.launch {
                try {
                    people = PeopleRead.Loaded(appHandle.people())
                } catch (e: CancellationException) {
                    throw e
                } catch (e: BridgeException) {
                    logger.error("reading the library's people failed", e)
                    // A cancelled read shows nothing: the list stays as it was.
                    errors.display(e)?.let { people = PeopleRead.Failed(it) }
                }
            }
    }

    /** Read the list again after a failed read, showing the spinner while it runs. */
    fun retry() {
        people = PeopleRead.Loading
        reload()
    }

    suspend fun loadStorage() {
        performBridgeAction(logger, "read the library's storage", errors, { error = it }) {
            storage = appHandle.storageLocation()
        }
    }

    /**
     * Follow the profile this person asked for. The request ends in the write that makes it, so each change reads the
     * people again.
     */
    private fun subscribeRequestedProfile(): ScreenSubscription<BridgeRequestedProfile?> =
        ScreenSubscription(
            appHandle.subscribeRequestedProfile().let { pulled(it::next, it::cancel) },
            scope,
            onValue = { value ->
                requestedProfile = value
                reload()
            },
            onError = { error ->
                logger.error("reading the requested profile failed", error)
                this@MembersModel.error = errors.display(error)
            },
        )

    fun subscribe(): () -> Unit {
        val members =
            ScreenSubscription(
                appHandle.subscribeMembers().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value ->
                    selfIsAdmin = value.selfIsAdmin
                    reload()
                },
                onError = { error ->
                    logger.error("reading the library's members failed", error)
                    this@MembersModel.error = errors.display(error)
                },
            )
        val profiles =
            ScreenSubscription(
                appHandle.subscribeProfiles().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { reload() },
                onError = { error ->
                    logger.error("reading the library's profiles failed", error)
                    this@MembersModel.error = errors.display(error)
                },
            )
        val open =
            ScreenSubscription(
                appHandle.subscribeInvites().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value -> invites = value },
                onError = { error ->
                    logger.error("reading the library's invites failed", error)
                    this@MembersModel.error = errors.display(error)
                },
            )
        val requested = subscribeRequestedProfile()
        val changes =
            ScreenSubscription(
                appHandle.subscribeAdminChanges().let { pulled(it::next, it::cancel) },
                scope,
                onValue = { value -> adminChanges = value },
                onError = { error ->
                    logger.error("reading the library's admin changes failed", error)
                    this@MembersModel.error = errors.display(error)
                },
            )
        return {
            members.stop()
            profiles.stop()
            open.stop()
            requested.stop()
            changes.stop()
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
 * The library's people and devices, on pages: the first holds You and People, a person's page and this person's own
 * devices open from it. Admins invite people, change roles and remove people; each person adds and removes their own
 * devices, and an admin removes anyone's.
 */
@Composable
fun MembersScreen(
    session: OpenLibrary,
    onBack: () -> Unit,
) {
    val scope = rememberScreenScope()
    val model = remember(session) { MembersModel(session.appHandle, session.errors, scope) }
    DisposableEffect(model) { onDispose(model.subscribe()) }
    LaunchedEffect(model) { model.loadStorage() }
    val dialogs = remember { MembersDialogs() }
    var pageKey by rememberSaveable { mutableStateOf(PeoplePage.People.key()) }
    val read = model.people
    // A person's page whose person has left the library falls back to the first page.
    val page =
        peoplePage(pageKey).let { requested ->
            val people = (read as? PeopleRead.Loaded)?.people.orEmpty()
            val gone = requested is PeoplePage.Person && people.none { it.membership.id == requested.memberId }
            if (gone) PeoplePage.People else requested
        }
    val goBack = { if (page == PeoplePage.People) onBack() else pageKey = PeoplePage.People.key() }
    BackHandler(enabled = page != PeoplePage.People, onBack = goBack)

    Column(modifier = Modifier.fillMaxSize()) {
        SettingsTopBar(title = pageTitle(page, read), onBack = goBack)
        model.error?.let { shown ->
            ErrorNotice(
                error = shown,
                modifier = Modifier.padding(horizontal = ThemeSpace.edge, vertical = ThemeSpace.related),
            )
        }
        PeopleContent(
            session = session,
            model = model,
            dialogs = dialogs,
            page = page,
            onOpen = { pageKey = it.key() },
        )
    }
    MembersDialogsShown(session = session, model = model, dialogs = dialogs)
}

/** The page's body: the people read, shown or failed, and the page it names. */
@Composable
private fun PeopleContent(
    session: OpenLibrary,
    model: MembersModel,
    dialogs: MembersDialogs,
    page: PeoplePage,
    onOpen: (PeoplePage) -> Unit,
) {
    when (val read = model.people) {
        // Who is in the library is a local read; nothing shows until it lands.
        PeopleRead.Loading -> {}

        is PeopleRead.Failed -> {
            Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                LoadFailure(read.error, onRetry = model::retry)
            }
        }

        is PeopleRead.Loaded -> {
            val callbacks = memberCallbacks(session, model, dialogs)
            when (page) {
                PeoplePage.People -> {
                    PeopleHome(
                        you = youActions(session),
                        people = read.people,
                        selfIsAdmin = model.selfIsAdmin,
                        requested = model.requestedProfile,
                        invites = model.invites,
                        adminChanges = model.adminChanges,
                        callbacks = callbacks,
                        inviteCallbacks = inviteCallbacks(session, dialogs),
                        onOpen = onOpen,
                    )
                }

                is PeoplePage.Person -> {
                    read.people.firstOrNull { it.membership.id == page.memberId }?.let { person ->
                        PersonPage(
                            member = person.membership,
                            selfIsAdmin = model.selfIsAdmin,
                            name = person.profile?.name ?: stringResource(R.string.members_unnamed),
                            callbacks = callbacks,
                        )
                    }
                }

                PeoplePage.Devices -> {
                    read.people.firstOrNull { it.membership.isSelf }?.let { person ->
                        DevicesPage(member = person.membership, callbacks = callbacks)
                    }
                }
            }
        }
    }
}

@Composable
private fun pageTitle(
    page: PeoplePage,
    read: PeopleRead,
): String =
    when (page) {
        PeoplePage.People -> {
            stringResource(R.string.members_title)
        }

        PeoplePage.Devices -> {
            stringResource(R.string.people_your_devices)
        }

        is PeoplePage.Person -> {
            (read as? PeopleRead.Loaded)
                ?.people
                ?.firstOrNull { it.membership.id == page.memberId }
                ?.profile
                ?.name ?: stringResource(R.string.members_unnamed)
        }
    }

private fun youActions(session: OpenLibrary): YouActions =
    YouActions(
        errors = session.errors,
        rename = { name -> session.appHandle.renameProfile(name) },
        setHistory = { chosen ->
            session.messages.perform("set history sharing to $chosen") { session.appHandle.setHistorySharing(chosen) }
        },
        createProfile = { name ->
            session.messages.perform("create the person's profile") { session.appHandle.createProfile(name) }
        },
    )

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
        onRemoveDevice = { device, own -> dialogs.pendingRemoval = PendingRemoval.Device(device, own) },
        onAddDevice = { dialogs.addingDevice = true },
        onDismissChange = { change ->
            model.perform("dismiss an admin change") { handle.acknowledgeAdminChange(change.operation) }
        },
    )
}

private fun inviteCallbacks(
    session: OpenLibrary,
    dialogs: MembersDialogs,
): InviteCallbacks =
    InviteCallbacks(
        onOpen = dialogs::open,
        // Retry now reads a request that has arrived at once, instead of at the next check.
        onRefresh = { session.messages.startRetrySync(session.appHandle::retryNow) },
    )

/** What this device's person can do to the library's members and their devices. */
internal class MemberCallbacks(
    val onInvite: () -> Unit,
    val onSetRole: (BridgeMember, BridgeMemberRole) -> Unit,
    val onRemovePerson: (BridgeMember, String) -> Unit,
    /** Remove a device, which is the person's own or another's. */
    val onRemoveDevice: (BridgeActiveDevice, Boolean) -> Unit,
    val onAddDevice: () -> Unit,
    val onDismissChange: (BridgeAdminChange) -> Unit,
)

/** What this device's person can do with an invite it has open. */
internal class InviteCallbacks(
    val onOpen: (BridgeInvite) -> Unit,
    val onRefresh: () -> Unit,
)

/**
 * One of this device's open invites: the person's name, marked sharing access until sync has granted it, then invited,
 * or joining once approval has begun; when it expires; and why coven holds its work, when it does.
 */
@Composable
internal fun InviteRow(
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
                InviteChip(invite.shown)
            }
            Text(
                text = stringResource(R.string.invite_expires, expires),
                style = ThemeText.detail.style,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            invite.held?.let { held ->
                Text(
                    text = context.coreString(held.reason.key),
                    style = ThemeText.detail.style,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
        IconButton(onClick = callbacks.onRefresh) {
            Icon(Icons.Filled.Refresh, contentDescription = stringResource(R.string.members_check_invites))
        }
    }
}

@Composable
private fun InviteChip(shown: BridgeInviteShown) {
    when (shown) {
        is BridgeInviteShown.Granting -> {
            StatusChip(stringResource(R.string.members_sharing_access), tone = StatusTone.ACTIVITY)
        }

        is BridgeInviteShown.Code -> {
            when (shown.progress) {
                is BridgeInviteProgress.Joining -> {
                    StatusChip(stringResource(R.string.members_joining), tone = StatusTone.ACTIVITY)
                }

                is BridgeInviteProgress.Waiting, is BridgeInviteProgress.Deciding -> {
                    StatusChip(stringResource(R.string.members_invited))
                }
            }
        }
    }
}

internal fun BridgeMemberRole.labelRes(): Int =
    when (this) {
        BridgeMemberRole.ADMIN -> R.string.members_role_admin
        BridgeMemberRole.MEMBER -> R.string.members_role_member
    }
