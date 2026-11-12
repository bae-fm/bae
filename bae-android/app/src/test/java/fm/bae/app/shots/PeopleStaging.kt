package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.PreviewData
import fm.bae.app.ui.settings.AndroidSettingsDevices
import fm.bae.app.ui.settings.AndroidSettingsPeople
import fm.bae.app.ui.settings.AndroidSettingsPerson
import fm.bae.app.ui.settings.InviteCallbacks
import fm.bae.app.ui.settings.MemberCallbacks
import fm.bae.app.ui.settings.NothingFails
import fm.bae.app.ui.settings.YouActions
import uniffi.bae_bridge.BridgeActiveDevice
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeAdminChangeKind
import uniffi.bae_bridge.BridgeAdminChangeState
import uniffi.bae_bridge.BridgeFormerDevice
import uniffi.bae_bridge.BridgeFormerDeviceState
import uniffi.bae_bridge.BridgeHistorySharing
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteEntry
import uniffi.bae_bridge.BridgeInviteProgress
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMember
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingReason
import uniffi.bae_bridge.BridgePendingWait
import uniffi.bae_bridge.BridgePerson

// The people the People and devices boards show: Ana reads the page, Ben is the other person, Dan is invited.

private const val ANA_ID = "00000000-0000-4000-8000-000000000001"
private const val BEN_ID = "00000000-0000-4000-8000-000000000002"
private const val DAN_ID = "00000000-0000-4000-8000-000000000003"

/** The People and devices page over [people], with [invites] this device has open. */
@Composable
internal fun PeoplePageScene(
    people: List<BridgePerson>,
    selfIsAdmin: Boolean = true,
    invites: List<BridgeInvite> = emptyList(),
) = AndroidSettingsPeople(
    you = YouActions(NothingFails, {}, {}, {}),
    people = people,
    selfIsAdmin = selfIsAdmin,
    requested = null,
    invites = invites.map { BridgeInviteEntry.Open(it) },
    adminChanges = emptyList(),
    callbacks = memberCallbacks(),
    inviteCallbacks = InviteCallbacks({}, {}, {}),
    error = null,
    onOpen = {},
    onBack = {},
)

/** [person]'s page, as an admin sees it, with the [adminChanges] this phone asked for. */
@Composable
internal fun PersonPageScene(
    person: BridgePerson,
    adminChanges: List<BridgeAdminChange> = emptyList(),
) = AndroidSettingsPerson(
    member = person.membership,
    selfIsAdmin = true,
    name = checkNotNull(person.profile).name,
    adminChanges = adminChanges,
    callbacks = memberCallbacks(),
    error = null,
    onBack = {},
)

/** [person]'s own devices. */
@Composable
internal fun DevicesPageScene(person: BridgePerson) =
    AndroidSettingsDevices(member = person.membership, callbacks = memberCallbacks(), error = null, onBack = {})

internal fun memberCallbacks() = MemberCallbacks({}, { _, _ -> }, { _, _ -> }, { _, _ -> }, {}, {})

/** Dan's open invite. */
internal fun danInvite(
    shown: BridgeInviteShown,
    held: BridgePendingWait? = null,
) = invitation("dan@example.com", shown, held, DAN_ID)

/** An open invite to [address]. */
internal fun invitation(
    address: String,
    shown: BridgeInviteShown,
    held: BridgePendingWait? = null,
    member: String = "00000000-0000-4000-8000-000000000004",
) = BridgeInvite(
    id = "invite-$address",
    member = member,
    label = address,
    access = BridgeMemberAccess.ProviderAccount(address),
    shown = shown,
    held = held,
)

/** Ana, who reads the page: this phone and a laptop, and two removed devices. */
internal fun ana(
    role: BridgeMemberRole = BridgeMemberRole.ADMIN,
    name: String = "Ana",
    history: BridgeHistorySharing = BridgeHistorySharing.PRIVATE,
    devices: List<BridgeActiveDevice> =
        listOf(
            BridgeActiveDevice(1uL, "Phone", isSelf = true, canRemove = false),
            BridgeActiveDevice(2uL, "Laptop", isSelf = false, canRemove = true),
        ),
    formerDevices: List<BridgeFormerDevice> = emptyList(),
): BridgePerson = person(ANA_ID, name, role, isSelf = true, devices = devices, history = history, formerDevices = formerDevices)

/** The two devices Ana's Your devices page lists under Removed devices. */
internal val removedDevices =
    listOf(
        BridgeFormerDevice(id = 3uL, name = "Old Phone", state = BridgeFormerDeviceState.REPLACED),
        BridgeFormerDevice(id = 4uL, name = "Tablet", state = BridgeFormerDeviceState.REMOVED),
    )

/** Ben, who has one device, a tablet. */
internal fun ben(role: BridgeMemberRole = BridgeMemberRole.MEMBER): BridgePerson =
    person(
        id = BEN_ID,
        name = "Ben",
        role = role,
        isSelf = false,
        devices = listOf(BridgeActiveDevice(5uL, "Tablet", isSelf = false, canRemove = true)),
    )

/** Ben on an S3 library, where a person is known by the access key they were given. */
internal fun benOnS3(): BridgePerson =
    ben().let { it.copy(membership = it.membership.copy(access = BridgeMemberAccess.S3AccessKey("AKIA4EXAMPLEBEN"))) }

private fun person(
    id: String,
    name: String,
    role: BridgeMemberRole,
    isSelf: Boolean,
    devices: List<BridgeActiveDevice>,
    history: BridgeHistorySharing = BridgeHistorySharing.PRIVATE,
    formerDevices: List<BridgeFormerDevice> = emptyList(),
): BridgePerson {
    val member: BridgeMember =
        PreviewData.member(id = id, role = role, isSelf = isSelf).copy(
            access = BridgeMemberAccess.ProviderAccount("$name@example.com"),
            activeDevices = devices,
            formerDevices = formerDevices,
        )
    val shown = PreviewData.person(member)
    return shown.copy(profile = shown.profile?.copy(name = name, historySharing = history))
}

/** Why coven holds Dan's invite. */
internal val heldRejected =
    BridgePendingWait(
        reason = BridgePendingReason(key = "core.pending_reason.rejected", detail = null, causes = emptyList()),
        action = BridgePendingAction.Wait,
    )

/** Dan's code, for which his request hasn't arrived. */
internal val waitingCode = BridgeInviteShown.Code("0000 0000", BridgeInviteProgress.Waiting)

/** The change that makes Ben an admin, in [state]. */
internal fun makeBenAdmin(state: BridgeAdminChangeState) =
    BridgeAdminChange(
        operation = 1,
        change = BridgeAdminChangeKind.SetRole(member = BEN_ID, person = "Ben", role = BridgeMemberRole.ADMIN),
        state = state,
    )
