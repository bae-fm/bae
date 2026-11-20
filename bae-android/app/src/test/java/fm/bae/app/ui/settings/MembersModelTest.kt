package fm.bae.app.ui.settings

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.PreviewData
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAdminChange
import uniffi.bae_bridge.BridgeAdminChangeKind
import uniffi.bae_bridge.BridgeAdminChangePlace
import uniffi.bae_bridge.BridgeAdminChangePlace.Library
import uniffi.bae_bridge.BridgeAdminChangePlace.People
import uniffi.bae_bridge.BridgeAdminChangeState
import uniffi.bae_bridge.BridgeAdminChanges
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteEntry
import uniffi.bae_bridge.BridgeInviteProgress
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMembership
import uniffi.bae_bridge.BridgePeople
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeRefusedInvite
import uniffi.bae_bridge.BridgeSharingRefusal
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.MembersSubscription
import uniffi.bae_bridge.NoHandle

/**
 * A failed read of the library's people ends the spinner with the failure, and
 * Retry subscribes again; a cancelled read shows nothing and leaves the list
 * as it was. The people and one person's page each follow their own
 * subscription.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class MembersModelTest {
    @Test
    fun aFailedFirstReadShowsTheFailureAndRetrySubscribesAgain() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        model.watchPeople()
        handle.peopleReads
            .last()
            .pulls
            .fail(offline())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Failed(DisplayError("couldn't reach the cloud")), model.people)

        model.retry()
        assertEquals(PeopleRead.Loading, model.people)
        handle.peopleReads
            .last()
            .pulls
            .deliver(twoPeople)
        shadowOf(Looper.getMainLooper()).idle()
        val loaded = model.people as PeopleRead.Loaded
        assertEquals(twoPeople, loaded.people)
    }

    @Test
    fun aCancelledReadLeavesTheListAsItWas() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        model.watchPeople()
        handle.peopleReads
            .last()
            .pulls
            .deliver(twoPeople)
        shadowOf(Looper.getMainLooper()).idle()
        val loaded = model.people as PeopleRead.Loaded

        handle.peopleReads
            .last()
            .pulls
            .fail(BridgeException.Cancelled())
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(loaded, model.people)
    }

    /** Each value the people subscription pulls replaces the one shown, with its counts. */
    @Test
    fun eachPulledValueOfThePeopleReplacesTheShownOne() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val stop = model.subscribe()

        handle.peopleReads
            .last()
            .pulls
            .deliver(twoPeople)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Loaded(twoPeople), model.people)

        val alone = BridgePeople(own = ana, others = emptyList(), people = 1uL, devices = 1uL)
        handle.peopleReads
            .last()
            .pulls
            .deliver(alone)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Loaded(alone), model.people)
        stop()
    }

    /** A person's page follows the subscription keyed by their member id; when the person has left, the subscription delivers null. */
    @Test
    fun aPersonsPageFollowsTheSubscriptionKeyedByTheirMemberId() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        model.person.show("member-2")
        assertEquals(listOf("member-2"), handle.persons.map { it.first })
        handle.persons
            .last()
            .second.pulls
            .deliver(ana)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(Shown("member-2", ana), model.person.shown)

        handle.persons
            .last()
            .second.pulls
            .deliver(null)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(Shown<String, BridgePerson?>("member-2", null), model.person.shown)

        model.person.hide()
        assertEquals(null, model.person.shown)
    }

    /**
     * The join request dialog follows the read of the first waiting request, which leaves out the invites whose dialog
     * the person closed. While a new read for more closed invites runs, the model keeps showing the last request
     * until the read returns its first value.
     */
    @Test
    fun theWaitingRequestFollowsTheReadThatLeavesOutTheClosedInvites() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val stop = model.subscribe()
        val request =
            BridgeInvite(
                id = "11",
                member = "member-1",
                label = "dan@example.com",
                labelInitials = "D",
                access = BridgeMemberAccess.ProviderAccount("dan@example.com"),
                shown = BridgeInviteShown.Code(code = "invite-code", progress = BridgeInviteProgress.Waiting),
                held = null,
            )

        model.waitingRequest.show(emptySet())
        handle.waitingRequests
            .last()
            .second.pulls
            .deliver(request)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(request, model.waitingRequest.shown?.value)

        model.waitingRequest.show(setOf("11"))
        assertEquals(listOf(emptyList(), listOf("11")), handle.waitingRequests.map { it.first })
        assertEquals("the last request stays until the new read answers", request, model.waitingRequest.shown?.value)
        handle.waitingRequests
            .last()
            .second.pulls
            .deliver(null)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(Shown<Set<String>, BridgeInvite?>(setOf("11"), null), model.waitingRequest.shown)
        stop()
    }

    /** An invite's dialog follows the read keyed by its id; an invite that ended arrives as none. */
    @Test
    fun anInvitesDialogFollowsTheReadKeyedByItsId() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val invite =
            BridgeInvite(
                id = "11",
                member = "member-1",
                label = "dan@example.com",
                labelInitials = "D",
                access = BridgeMemberAccess.ProviderAccount("dan@example.com"),
                shown = BridgeInviteShown.Code(code = "invite-code", progress = BridgeInviteProgress.Waiting),
                held = null,
            )

        model.invite.show("11")
        assertEquals(listOf("11"), handle.inviteReads.map { it.first })
        handle.inviteReads
            .last()
            .second.pulls
            .deliver(invite)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(Shown("11", invite), model.invite.shown)

        handle.inviteReads
            .last()
            .second.pulls
            .deliver(null)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(Shown<String, BridgeInvite?>("11", null), model.invite.shown)

        model.invite.hide()
        assertEquals(null, model.invite.shown)
    }

    private val ana = PreviewData.person()
    private val twoPeople = BridgePeople(own = ana, others = listOf(PreviewData.person()), people = 2uL, devices = 3uL)

    /** Each list of invites the model pulls replaces the one it shows. */
    @Test
    fun eachPulledListOfInvitesReplacesTheShownOne() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val stop = model.subscribe()
        assertEquals(emptyList<BridgeInviteEntry>(), model.inviteEntries)

        val invite =
            BridgeInvite(
                id = "11",
                member = "member-1",
                label = "dan@example.com",
                labelInitials = "D",
                access = BridgeMemberAccess.ProviderAccount("dan@example.com"),
                shown = BridgeInviteShown.Code(code = "invite-code", progress = BridgeInviteProgress.Waiting),
                held = null,
            )
        handle.invites.deliver(listOf(invite))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf<BridgeInviteEntry>(BridgeInviteEntry.Open(invite)), model.inviteEntries)

        handle.invites.deliver(emptyList())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(emptyList<BridgeInviteEntry>(), model.inviteEntries)
        stop()
    }

    /** An invite a sharing refusal ended arrives beside the open ones, in the order they came, and leaves with the list. */
    @Test
    fun anInviteARefusalEndedArrivesBesideTheOpenOnes() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val stop = model.subscribe()
        val refused =
            BridgeRefusedInvite(
                operation = 7L,
                address = "dan@example.com",
                addressInitials = "D",
                refusal = BridgeSharingRefusal.INVALID_ADDRESS,
                reasonKey = "core.invite.refused.invalid_address",
            )

        handle.invites.deliver(emptyList(), refused = listOf(refused))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf<BridgeInviteEntry>(BridgeInviteEntry.Refused(refused)), model.inviteEntries)

        handle.invites.deliver(emptyList())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(emptyList<BridgeInviteEntry>(), model.inviteEntries)
        stop()
    }

    /**
     * The model reads the admin changes for the place the page shows, and each list the read returns replaces the list
     * shown for that place; another place's page shows none of them.
     */
    @Test
    fun eachPulledListOfAdminChangesReplacesTheShownOne() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val stop = model.subscribe()
        model.adminChanges.show(BridgeAdminChangePlace.People)
        assertEquals(listOf<BridgeAdminChangePlace>(BridgeAdminChangePlace.People), handle.adminChangePlaces)
        assertEquals(null, model.adminChangesAt(People))

        val waiting =
            BridgeAdminChange(
                operation = 7,
                change = BridgeAdminChangeKind.RemoveDevice(device = 4uL, name = "Tablet"),
                state = BridgeAdminChangeState.Waiting,
            )
        handle.adminChanges.deliver(BridgeAdminChanges(listOf(waiting), any = true))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(BridgeAdminChanges(listOf(waiting), any = true), model.adminChangesAt(People))
        assertEquals(null, model.adminChangesAt(Library))

        handle.adminChanges.deliver(BridgeAdminChanges(emptyList(), any = false))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(BridgeAdminChanges(emptyList(), any = false), model.adminChangesAt(People))
        stop()
    }

    /** Core's answer for what each failure shows: a cancellation shows nothing. */
    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) =
            if (error is BridgeException.Cancelled) null else DisplayError("couldn't reach the cloud")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }

    private fun offline() = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

    /** A handle whose members never change: a pull waits until cancelled. */
    private class SilentMembersHandle : FakeAppHandle() {
        override fun subscribeMembers(): MembersSubscription =
            object : MembersSubscription(NoHandle) {
                private val values = Channel<BridgeMembership>()

                override suspend fun next(): BridgeMembership = values.receive()

                override suspend fun cancel() {
                    values.close(BridgeException.Cancelled())
                }

                override fun close() = Unit
            }
    }
}
