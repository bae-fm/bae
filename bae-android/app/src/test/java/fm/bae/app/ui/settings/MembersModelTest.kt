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
import uniffi.bae_bridge.BridgeInviteDeliveryKind
import uniffi.bae_bridge.BridgeInviteProgress
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeMemberAccess
import uniffi.bae_bridge.BridgeMembership
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePersonProfile
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.MembersSubscription
import uniffi.bae_bridge.NoHandle
import uniffi.bae_bridge.ProfilesSubscription
import java.time.Instant

/**
 * A failed read of the library's people ends the spinner with the failure, and
 * Retry reads again; a cancelled read shows nothing and leaves the list as it
 * was.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class MembersModelTest {
    @Test
    fun aFailedFirstReadShowsTheFailureAndRetryReadsAgain() {
        val handle = PeopleHandle(listOf(Result.failure(offline()), Result.success(listOf(PreviewData.person()))))
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        model.reload()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Failed(DisplayError("couldn't reach storage")), model.people)

        model.retry()
        shadowOf(Looper.getMainLooper()).idle()
        val loaded = model.people as PeopleRead.Loaded
        assertEquals(listOf(PreviewData.person().membership.id), loaded.people.map { it.membership.id })
        assertEquals(2, handle.reads)
    }

    @Test
    fun aCancelledReadLeavesTheListAsItWas() {
        val handle =
            PeopleHandle(
                listOf(
                    Result.failure(BridgeException.Cancelled()),
                    Result.success(listOf(PreviewData.person())),
                    Result.failure(BridgeException.Cancelled()),
                ),
            )
        val model =
            MembersModel(handle, TestErrorDisplays, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        model.reload()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(PeopleRead.Loading, model.people)

        model.reload()
        shadowOf(Looper.getMainLooper()).idle()
        val loaded = model.people as PeopleRead.Loaded

        model.reload()
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(loaded, model.people)
        assertEquals(3, handle.reads)
    }

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
                invitee = "Dan",
                delivery = BridgeInviteDeliveryKind.SENT,
                access = BridgeMemberAccess.ProviderAccount("dan@example.com"),
                expiresAt = Instant.ofEpochSecond(2_000_000_000),
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

    /** Core's answer for what each failure shows: a cancellation shows nothing. */
    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) =
            if (error is BridgeException.Cancelled) null else DisplayError("couldn't reach storage")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }

    private fun offline() = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

    /** A handle whose members and profiles never change: a pull waits until cancelled. */
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

        override fun subscribeProfiles(): ProfilesSubscription =
            object : ProfilesSubscription(NoHandle) {
                private val values = Channel<List<BridgePersonProfile>>()

                override suspend fun next(): List<BridgePersonProfile> = values.receive()

                override suspend fun cancel() {
                    values.close(BridgeException.Cancelled())
                }

                override fun close() = Unit
            }
    }

    private class PeopleHandle(
        private val answers: List<Result<List<BridgePerson>>>,
    ) : FakeAppHandle() {
        var reads = 0
            private set

        override suspend fun people(): List<BridgePerson> = answers[reads++].getOrThrow()
    }
}
