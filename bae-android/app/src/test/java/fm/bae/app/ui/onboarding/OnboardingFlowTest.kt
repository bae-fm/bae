package fm.bae.app.ui.onboarding

import android.app.Application
import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.NoErrorDisplays
import fm.bae.app.data.LibrarySetupStatus
import fm.bae.fastvalues.LatestStream
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeInfo
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeJoinControl
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeReplacedBy
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeUnfinishedJoin
import uniffi.bae_bridge.BridgeUnfinishedRestore
import uniffi.bae_bridge.NoHandle

/** A scanned code goes straight into adding this device; a typed one shows what it read first. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingFlowTest {
    /**
     * A host whose restore from a code records the code and runs until the
     * test ends, whose finishing of a replacement records the library and
     * call, then fails with [finishFailure], or runs until the test ends
     * without one, and which lists [unfinished] as the restores under way,
     * recording each it finishes and running until the test ends. A restore
     * from a code answers [restoreAnswer] when a test sets one.
     */
    private class RestoreRecordingHost : NoJoinsHost() {
        val restoredCodes = mutableListOf<String>()
        val finished = mutableListOf<Pair<String, BridgeReplacedBy>>()
        var finishFailure: BridgeException? = null
        var unfinished = emptyList<BridgeUnfinishedRestore>()
        val resumed = mutableListOf<BridgeUnfinishedRestore>()
        var restoreAnswer: BridgeBootstrapResult? = null
        var joinsUnderWay = emptyList<BridgeUnfinishedJoin>()
        var joinAnswer: BridgeBootstrapResult? = null
        val joined = mutableListOf<Pair<String, String>>()
        val resumedJoins = mutableListOf<BridgeUnfinishedJoin>()
        var pairAnswer: BridgeBootstrapResult? = null
        val paired = mutableListOf<Pair<String, BridgeS3Credentials?>>()

        override suspend fun unfinishedJoins(): List<BridgeUnfinishedJoin> = joinsUnderWay

        override suspend fun joinWithInvite(
            code: String,
            name: String,
            deviceName: String,
            setup: BridgeLibrarySetup,
            control: BridgeJoinControl,
        ): BridgeBootstrapResult {
            joined += code to name
            joinAnswer?.let { return it }
            awaitCancellation()
        }

        override suspend fun joinWithPairing(
            code: String,
            credentials: BridgeS3Credentials?,
            deviceName: String,
            setup: BridgeLibrarySetup,
            control: BridgeJoinControl,
        ): BridgeBootstrapResult {
            paired += code to credentials
            pairAnswer?.let { return it }
            awaitCancellation()
        }

        override suspend fun resumeJoin(
            join: BridgeUnfinishedJoin,
            setup: BridgeLibrarySetup,
            control: BridgeJoinControl,
        ): BridgeBootstrapResult {
            resumedJoins += join
            joinAnswer?.let { return it }
            awaitCancellation()
        }

        override suspend fun unfinishedRestores(): List<BridgeUnfinishedRestore> = unfinished

        override suspend fun resumeRestore(
            restore: BridgeUnfinishedRestore,
            setup: BridgeLibrarySetup,
            cancellation: BootstrapCancellation,
        ): BridgeBootstrapResult {
            resumed += restore
            awaitCancellation()
        }

        override suspend fun resumeReplacement(
            libraryId: String,
            by: BridgeReplacedBy,
            setup: BridgeLibrarySetup,
            cancellation: BootstrapCancellation,
        ): BridgeBootstrapResult {
            finished += libraryId to by
            finishFailure?.let { throw it }
            awaitCancellation()
        }

        override suspend fun restoreFromCode(
            code: String,
            credentials: BridgeS3Credentials?,
            deviceName: String,
            setup: BridgeLibrarySetup,
            cancellation: BootstrapCancellation,
        ): BridgeBootstrapResult {
            restoredCodes += code
            restoreAnswer?.let { return it }
            awaitCancellation()
        }
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val host = RestoreRecordingHost()

    @After
    fun endRestores() {
        scope.cancel()
    }

    /** A restore code for an S3 library, holding the S3 key as the QR code the person's own device shows does, or not. */
    private fun restoreCode(carriesS3Key: Boolean) =
        BridgeCodeInfo.Restore(
            BridgeCodeMetadata(
                libraryId = "library-1",
                memberId = "member-1",
                provider = BridgeCloudProvider.S3,
                location =
                    BridgeStorageLocation.S3(
                        bucket = "music-library",
                        region = "us-east-1",
                        endpoint = null,
                        profile = null,
                    ),
                oauthAccess = null,
                carriesS3Key = carriesS3Key,
            ),
        )

    /**
     * Where each restore the flow runs stands, which a test says in place of
     * core: one subscription per setup, as core opens one for each, the
     * latest last.
     */
    private val said = mutableListOf<SaidStates>()

    private fun newFlow(restoreOver: RestoreOver? = null): OnboardingFlow {
        val flow =
            OnboardingFlow(
                scope,
                RuntimeEnvironment.getApplication(),
                OnboardingCore(
                    host,
                    NoErrorDisplays::display,
                    ::FakeBootstrapCancellation,
                    ::unfakedOpenProgress,
                    {
                        LibrarySetupStatus.written(
                            FakeLibrarySetup(),
                            SaidStates().also { said += it },
                            LatestStream.written(),
                            scope,
                        )
                    },
                    // A join under way here is for the invite's library.
                    JoinCore({ "library-1" }, { BridgeJoinControl(NoHandle) }),
                ),
                null,
                restoreOver,
            ) {}
        shadowOf(Looper.getMainLooper()).idle()
        return flow
    }

    private val enterCode = OnboardingStep.EnterCode(CodeEntry.NewDevice)

    /** A pairing code for an S3 library, whose QR code holds the S3 key and whose code to type does not. */
    private fun pairingCode(carriesS3Key: Boolean) =
        BridgeCodeInfo.Pairing(
            (restoreCode(carriesS3Key) as BridgeCodeInfo.Restore).metadata,
        )

    /** A scanned pairing code, which holds the S3 key, asks the other device to add this one at once. */
    @Test
    fun aScannedPairingCodeAsksTheOtherDeviceAtOnce() {
        val info = pairingCode(carriesS3Key = true)
        val flow = newFlow()

        flow.advance(enterCode, CODE, info, CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        val running = flow.step as OnboardingStep.Running
        assertEquals(OnboardingStep.Pair(CODE, info.metadata, CodeEntry.NewDevice), running.from)
        assertEquals(listOf<Pair<String, BridgeS3Credentials?>>(CODE to null), host.paired)
        assertEquals(emptyList<String>(), host.restoredCodes)
    }

    /** The code to type holds no S3 key: it waits for the key, and a typed code shows what it read first. */
    @Test
    fun aTypedPairingCodeWaitsForTheKeyAndForThePerson() {
        val keyless = pairingCode(carriesS3Key = false)
        val flow = newFlow()
        flow.advance(enterCode, CODE, keyless, CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(OnboardingStep.Pair(CODE, keyless.metadata, CodeEntry.NewDevice), flow.step)

        val keyed = pairingCode(carriesS3Key = true)
        flow.advance(enterCode, CODE, keyed, CodeSource.TYPED)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(OnboardingStep.Pair(CODE, keyed.metadata, CodeEntry.NewDevice), flow.step)
        assertEquals(emptyList<Pair<String, BridgeS3Credentials?>>(), host.paired)

        val read = flow.step as OnboardingStep.Pair
        val credentials = BridgeS3Credentials("key", "secret")
        flow.add(read, credentials)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(listOf<Pair<String, BridgeS3Credentials?>>(CODE to credentials), host.paired)
    }

    /** A pairing the other device declined or let expire ends on its own step, which goes back to the code screen. */
    @Test
    fun aPairingThatEndedWithNoDeviceSaysSo() {
        host.pairAnswer = BridgeBootstrapResult.Declined
        val flow = newFlow()

        flow.advance(enterCode, CODE, pairingCode(carriesS3Key = true), CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        val ended = flow.step as OnboardingStep.PairingEnded
        assertEquals(OnboardingStep.PairingEnded(CodeEntry.NewDevice), ended)
        flow.back(ended)
        assertEquals(enterCode, flow.step)
    }

    @Test
    fun aScannedRestoreCodeAddsThisDeviceAtOnce() {
        val info = restoreCode(carriesS3Key = true)
        val flow = newFlow()

        flow.advance(enterCode, CODE, info, CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        val running = flow.step as OnboardingStep.Running
        assertEquals(OnboardingStep.Restore(CODE, info.metadata, CodeEntry.NewDevice), running.from)
        assertNull(running.state)
        assertEquals(listOf(CODE), host.restoredCodes)
    }

    /** Where a running restore stands is each state core says, as it says it. */
    @Test
    fun aRunningRestoreShowsWhereCoreSaysItStands() {
        val info = restoreCode(carriesS3Key = true)
        val flow = newFlow()
        flow.advance(enterCode, CODE, info, CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        val loading = BridgeBootstrapState(BridgeBootstrapStatus.Loading, cancellable = true)
        said.last().say(loading)
        shadowOf(Looper.getMainLooper()).idle()

        val running = flow.step as OnboardingStep.Running
        assertEquals(OnboardingStep.Restore(CODE, info.metadata, CodeEntry.NewDevice), running.from)
        assertEquals(loading, running.state)
    }

    @Test
    fun aScannedS3CodeWithoutItsKeyAsksForTheKey() {
        val info = restoreCode(carriesS3Key = false)
        val flow = newFlow()

        flow.advance(enterCode, CODE, info, CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(OnboardingStep.Restore(CODE, info.metadata, CodeEntry.NewDevice), flow.step)
        assertEquals(emptyList<String>(), host.restoredCodes)
    }

    @Test
    fun aTypedRestoreCodeShowsWhatItReadBeforeAdding() {
        val info = restoreCode(carriesS3Key = true)
        val flow = newFlow()

        flow.advance(enterCode, CODE, info, CodeSource.TYPED)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(OnboardingStep.Restore(CODE, info.metadata, CodeEntry.NewDevice), flow.step)
        assertEquals(emptyList<String>(), host.restoredCodes)
    }

    /** A restore over this device's copy that stopped partway, whose screen finishes it. */
    private val finishing =
        RestoreOver(
            libraryId = "library-1",
            title = "Finish restoring this library",
            message = "Restoring this library on this device did not finish.",
            onBack = {},
            start = RestoreOver.Start.FINISH,
        )

    /** A restore that stopped partway finishes from what core kept as soon as its screen opens, with no code. */
    @Test
    fun finishingARestoreBeginsAtOnceWithoutACode() {
        val flow = newFlow(finishing)

        val running = flow.step as OnboardingStep.Running
        assertEquals(OnboardingStep.FinishRestore(finishing), running.from)
        assertEquals(listOf("library-1" to BridgeReplacedBy.RESTORE), host.finished)
    }

    /** A finish that fails returns to the code screen, where the person's current code finishes it too. */
    @Test
    fun aFailedFinishReturnsToTheCodeToFinishWith() {
        host.finishFailure = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")
        val flow = newFlow(finishing)

        assertEquals(listOf("library-1" to BridgeReplacedBy.RESTORE), host.finished)
        assertEquals(OnboardingStep.EnterCode(finishing), flow.step)
        assertEquals(DisplayError("failed", "offline"), flow.error)
    }

    /** A restore onto this phone that stopped after it recorded itself goes on finishing as the welcome opens. */
    @Test
    fun aStoppedRestoreGoesOnFinishingAtOnce() {
        val restore = BridgeUnfinishedRestore(NoHandle)
        host.unfinished = listOf(restore)
        val flow = newFlow()

        val running = flow.step as OnboardingStep.Running
        assertEquals(OnboardingStep.ResumeRestore(restore), running.from)
        assertEquals(listOf(restore), host.resumed)
    }

    /** An invite for the library [restoreCode]'s metadata names. */
    private fun invite() =
        BridgeInviteInfo(
            metadata = (restoreCode(carriesS3Key = false) as BridgeCodeInfo.Restore).metadata,
            libraryName = "Household",
            inviter = "Ana",
        )

    /** An invite's code asks the person's name before it joins, and Join sends the trimmed name. */
    @Test
    fun anInvitesCodeAsksTheNameThenJoinsWithIt() {
        val flow = newFlow()
        val invite = invite()

        flow.advance(enterCode, INVITE_CODE, BridgeCodeInfo.Invite(invite), CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        val asking = flow.step as OnboardingStep.JoinName
        assertEquals(OnboardingStep.JoinName(INVITE_CODE, invite, CodeEntry.NewDevice), asking)
        assertEquals(emptyList<Pair<String, String>>(), host.joined)

        flow.join(asking, "  Dan ")
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf(INVITE_CODE to "Dan"), host.joined)
    }

    /** A code whose library has a join under way here goes on with that join, which asks for no name. */
    @Test
    fun aJoinUnderWayForTheInvitesLibraryAsksNoName() {
        val flow = newFlow()
        val underWay = BridgeUnfinishedJoin(NoHandle)
        host.joinsUnderWay = listOf(underWay)

        flow.advance(enterCode, INVITE_CODE, BridgeCodeInfo.Invite(invite()), CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        val running = flow.step as OnboardingStep.Running
        assertEquals(JoinStart.Unfinished(underWay), (running.from as OnboardingStep.Invite).start)
        assertEquals(listOf(underWay), host.resumedJoins)
        assertEquals(emptyList<Pair<String, String>>(), host.joined)
    }

    /** A join that ends without letting the device in shows the ended step, and OK returns to the code screen. */
    @Test
    fun aDeclinedJoinEndsOnTheEndedStep() {
        val flow = newFlow()
        val invite = invite()
        host.joinAnswer = BridgeBootstrapResult.Declined
        flow.advance(enterCode, INVITE_CODE, BridgeCodeInfo.Invite(invite), CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        flow.join(flow.step as OnboardingStep.JoinName, "Dan")
        shadowOf(Looper.getMainLooper()).idle()

        val ended = flow.step as OnboardingStep.JoinEnded
        assertEquals(OnboardingStep.JoinEnded(invite, CodeEntry.NewDevice), ended)
        assertNull(flow.error)

        flow.back(ended)

        assertEquals(enterCode, flow.step)
    }

    /** A join under way that ends is the same ended step. */
    @Test
    fun aDeclinedResumedJoinEndsOnTheEndedStep() {
        val flow = newFlow()
        val invite = invite()
        host.joinsUnderWay = listOf(BridgeUnfinishedJoin(NoHandle))
        host.joinAnswer = BridgeBootstrapResult.Declined

        flow.advance(enterCode, INVITE_CODE, BridgeCodeInfo.Invite(invite), CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(OnboardingStep.JoinEnded(invite, CodeEntry.NewDevice), flow.step)
    }

    /** A restore refused because another join or restore of the library is under way here goes on with that one. */
    @Test
    fun aRestoreRefusedForACallUnderWayGoesOnWithIt() {
        val restore = BridgeUnfinishedRestore(NoHandle)
        val flow = newFlow()
        host.unfinished = listOf(restore)
        host.restoreAnswer = BridgeBootstrapResult.CallUnderWay

        flow.advance(enterCode, CODE, restoreCode(carriesS3Key = true), CodeSource.SCANNED)
        shadowOf(Looper.getMainLooper()).idle()

        val running = flow.step as OnboardingStep.Running
        assertEquals(OnboardingStep.ResumeRestore(restore), running.from)
        assertEquals(listOf(restore), host.resumed)
    }

    private companion object {
        const val CODE = "CVR1-RESTORE-CODE"
        const val INVITE_CODE = "CVI1-INVITE-CODE"
    }
}
