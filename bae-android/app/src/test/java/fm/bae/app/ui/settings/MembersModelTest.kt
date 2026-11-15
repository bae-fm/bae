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
import uniffi.bae_bridge.BridgeAdminChangeState
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteEntry
import uniffi.bae_bridge.BridgeInviteProgress
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMembership
import uniffi.bae_bridge.BridgePeople
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeRefusedInvite
import uniffi.bae_bridge.BridgeRequestedProfile
import uniffi.bae_bridge.BridgeRequestedProfileState
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
        handle.people.pulls.fail(offline())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Failed(DisplayError("couldn't reach the cloud")), model.people)

        model.retry()
        assertEquals(PeopleRead.Loading, model.people)
        handle.people.pulls.deliver(twoPeople())
        shadowOf(Looper.getMainLooper()).idle()
        val loaded = model.people as PeopleRead.Loaded
        assertEquals(twoPeople(), loaded.people)
    }

    @Test
    fun aCancelledReadLeavesTheListAsItWas() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        model.watchPeople()
        handle.people.pulls.deliver(twoPeople())
        shadowOf(Looper.getMainLooper()).idle()
        val loaded = model.people as PeopleRead.Loaded

        handle.people.pulls.fail(BridgeException.Cancelled())
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

        handle.people.pulls.deliver(twoPeople())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Loaded(twoPeople()), model.people)

        val alone = BridgePeople(own = PreviewData.person(), others = emptyList(), people = 1uL, devices = 1uL)
        handle.people.pulls.deliver(alone)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Loaded(alone), model.people)
        stop()
    }

    /** A person's page follows the subscription keyed by their member id; a person who left arrives as none. */
    @Test
    fun aPersonsPageFollowsTheSubscriptionKeyedByTheirMemberId() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        model.watchPerson("member-2")
        assertEquals(listOf("member-2"), handle.persons.map { it.first })
        handle.persons
            .last()
            .second.pulls
            .deliver(PreviewData.person())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PersonRead.Loaded("member-2", PreviewData.person()), model.person)

        handle.persons
            .last()
            .second.pulls
            .deliver(null)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PersonRead.Loaded("member-2", null), model.person)

        model.stopWatchingPerson()
        assertEquals(PersonRead.Loading, model.person)
    }

    private fun twoPeople() = BridgePeople(own = PreviewData.person(), others = listOf(PreviewData.person()), people = 2uL, devices = 3uL)

    /** Each list of open invites the model pulls replaces the one it shows. */
    @Test
    fun eachPulledListOfOpenInvitesReplacesTheShownOne() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val stop = model.subscribe()
        assertEquals(emptyList<BridgeInvite>(), model.invites)

        val invite =
            BridgeInvite(
                id = "11",
                member = "member-1",
                label = "dan@example.com",
                access = BridgeMemberAccess.ProviderAccount("dan@example.com"),
                shown = BridgeInviteShown.Code(code = "invite-code", progress = BridgeInviteProgress.Waiting),
                held = null,
            )
        handle.invites.deliver(listOf(invite))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf("11"), model.invites.map { it.id })

        handle.invites.deliver(emptyList())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(emptyList<BridgeInvite>(), model.invites)
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
                refusal = BridgeSharingRefusal.INVALID_ADDRESS,
                reasonKey = "core.invite.refused.invalid_address",
            )

        handle.invites.deliver(emptyList(), refused = listOf(refused))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf<BridgeInviteEntry>(BridgeInviteEntry.Refused(refused)), model.inviteEntries)
        assertEquals(emptyList<BridgeInvite>(), model.invites)

        handle.invites.deliver(emptyList())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(emptyList<BridgeInviteEntry>(), model.inviteEntries)
        stop()
    }

    /** Each list of admin changes the model pulls replaces the one it shows. */
    @Test
    fun eachPulledListOfAdminChangesReplacesTheShownOne() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val stop = model.subscribe()
        assertEquals(emptyList<BridgeAdminChange>(), model.adminChanges)

        val waiting =
            BridgeAdminChange(
                operation = 7,
                change = BridgeAdminChangeKind.RemoveDevice(device = 4uL, name = "Tablet"),
                state = BridgeAdminChangeState.Waiting,
            )
        handle.adminChanges.deliver(listOf(waiting))
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf(waiting), model.adminChanges)

        handle.adminChanges.deliver(emptyList())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(emptyList<BridgeAdminChange>(), model.adminChanges)
        stop()
    }

    /** The request ends in the write that makes the person's profile, which the people subscription shows. */
    @Test
    fun eachRequestedProfileValueIsShown() {
        val handle = SilentMembersHandle()
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val stop = model.subscribe()
        val making = BridgeRequestedProfile(name = "Robin", state = BridgeRequestedProfileState.Making)

        handle.requestedProfile.pulls.deliver(making)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(making, model.requestedProfile)

        handle.requestedProfile.pulls.deliver(null)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(null, model.requestedProfile)
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
