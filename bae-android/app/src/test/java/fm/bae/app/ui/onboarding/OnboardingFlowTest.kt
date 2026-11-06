package fm.bae.app.ui.onboarding

import android.app.Application
import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
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
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgeReplacedBy
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeStorageLocation

/** A scanned code goes straight into adding this device; a typed one shows what it read first. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingFlowTest {
    /**
     * A host whose restore from a code records the code and runs until the
     * test ends, and whose finishing of a replacement records the library
     * and call, then fails with [finishFailure], or runs until the test ends
     * without one.
     */
    private class RestoreRecordingHost : NoJoinsHost() {
        val restoredCodes = mutableListOf<String>()
        val finished = mutableListOf<Pair<String, BridgeReplacedBy>>()
        var finishFailure: BridgeException? = null

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

    /** Where the running restore stands, which a test says in place of core. */
    private val states = SaidStates()

    private fun newFlow(restoreOver: RestoreOver? = null): OnboardingFlow {
        val flow =
            OnboardingFlow(
                scope,
                RuntimeEnvironment.getApplication(),
                OnboardingCore(host, ::FakeBootstrapCancellation, ::unfakedOpenProgress) {
                    LibrarySetupStatus.written(FakeLibrarySetup(), states, LatestStream.written(), scope)
                },
                null,
                restoreOver,
            ) {}
        shadowOf(Looper.getMainLooper()).idle()
        return flow
    }

    private val enterCode = OnboardingStep.EnterCode(CodeEntry.NewDevice)

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
        states.say(loading)
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
    }

    private companion object {
        const val CODE = "CVR1-RESTORE-CODE"
    }
}
