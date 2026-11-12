package fm.bae.app.ui.settings

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
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
import fm.bae.app.BaeLogger
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.R
import fm.bae.app.coreString
import fm.bae.app.performBridgeAction
import fm.bae.app.ui.components.AndroidButton
import fm.bae.app.ui.components.AndroidNotice
import fm.bae.app.ui.components.AndroidRow
import fm.bae.app.ui.components.ButtonKind
import fm.bae.app.ui.components.NoticeLook
import fm.bae.app.ui.components.RowIcon
import fm.bae.app.ui.components.StatusTone
import fm.bae.app.ui.components.retryAction
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
import uniffi.bae_bridge.BridgeInviteEntry
import uniffi.bae_bridge.BridgeInviteProgress
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgeRefusedInvite
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

    /**
     * The rows of People's invites in the order they were made: this device's open invites, each with its request
     * once it arrives, and the invites a sharing refusal ended, until the person dismisses them.
     */
    var inviteEntries by mutableStateOf<List<BridgeInviteEntry>>(emptyList())
        private set

    /** This device's open invites. */
    val invites: List<BridgeInvite>
        get() = inviteEntries.filterIsInstance<BridgeInviteEntry.Open>().map { it.invite }
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
                onValue = { value -> inviteEntries = value },
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

    PeopleContent(
        session = session,
        model = model,
        dialogs = dialogs,
        page = page,
        onOpen = { pageKey = it.key() },
        onBack = goBack,
    )
    MembersDialogsShown(session = session, model = model, dialogs = dialogs)
}

/** The page: its screen over the people read, shown or failed, and the page it names. */
@Composable
private fun PeopleContent(
    session: OpenLibrary,
    model: MembersModel,
    dialogs: MembersDialogs,
    page: PeoplePage,
    onOpen: (PeoplePage) -> Unit,
    onBack: () -> Unit,
) {
    val read = model.people
    if (read is PeopleRead.Loaded) {
        LoadedPeoplePage(
            session = session,
            model = model,
            people = read.people,
            callbacks = memberCallbacks(session, model, dialogs),
            dialogs = dialogs,
            page = page,
            onOpen = onOpen,
            onBack = onBack,
        )
        return
    }
    PeopleFrame(title = pageTitle(page, read), error = model.error, onBack = onBack) {
        // Who is in the library is a local read; nothing shows until it lands.
        if (read is PeopleRead.Failed) {
            Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                AndroidNotice(
                    error = read.error,
                    look = NoticeLook.CENTERED,
                    action = read.error.retryAction(onClick = model::retry),
                )
            }
        }
    }
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
        onRemoveDevice = { device, own -> dialogs.pendingRemoval = PendingRemoval.Device(device, own) },
        onAddDevice = { dialogs.addingDevice = true },
        onDismissChange = { change ->
            model.perform("dismiss an admin change") { handle.acknowledgeAdminChange(change.operation) }
        },
    )
}

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
    val onDismissRefused: (BridgeRefusedInvite) -> Unit,
)

/**
 * One of this device's open invites: the address it shares with (the key's id on S3), marked invited, which a tap
 * opens; "Shares access once online" while sync can't grant it because this device is [offline]; and why coven holds
 * its work, when it does. Once approval has begun the row is the person's name, marked joining, with the address under
 * it, and opens nothing: nothing is left to decide or cancel.
 */
@Composable
internal fun InviteRow(
    invite: BridgeInvite,
    offline: Boolean,
    callbacks: InviteCallbacks,
) {
    val context = LocalContext.current
    val shown = invite.shown
    val joining = (shown as? BridgeInviteShown.Code)?.progress as? BridgeInviteProgress.Joining
    if (joining != null) {
        AndroidRow(
            title = joining.name,
            subtitle = invite.label,
            tag = stringResource(R.string.members_joining),
        )
        return
    }
    val held = invite.held
    AndroidRow(
        title = invite.label,
        tag = stringResource(R.string.members_invited),
        subtitle =
            when {
                held != null -> context.coreString(held.reason.key)
                offline && shown is BridgeInviteShown.Granting -> stringResource(R.string.members_invite_shares_access)
                else -> null
            },
        tone = if (held != null) StatusTone.DANGER else null,
        icon = RowIcon.CHEVRON,
        onClick = { callbacks.onOpen(invite) },
    )
}

/**
 * An invite a sharing refusal ended, in its row's place: the address it was to share with, why the provider refused
 * and what to fix, and Dismiss, which removes the notice for good.
 */
@Composable
internal fun RefusedInviteRow(
    refused: BridgeRefusedInvite,
    callbacks: InviteCallbacks,
) {
    val context = LocalContext.current
    AndroidRow(
        title = refused.address,
        subtitle =
            stringResource(
                R.string.members_invite_refused,
                context.coreString(refused.reasonKey),
            ),
        tone = StatusTone.DANGER,
        trailing = {
            AndroidButton(kind = ButtonKind.TEXT, onClick = { callbacks.onDismissRefused(refused) }) {
                Text(stringResource(R.string.dismiss))
            }
        },
    )
}

internal fun BridgeMemberRole.labelRes(): Int =
    when (this) {
        BridgeMemberRole.ADMIN -> R.string.members_role_admin
        BridgeMemberRole.MEMBER -> R.string.members_role_member
    }
