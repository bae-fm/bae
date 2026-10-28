package fm.bae.app.ui.onboarding

import android.app.Application
import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BootstrapCallback
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeInfo
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeStorageLocation

/** A scanned code goes straight into adding this device; a typed one shows what it read first. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingFlowTest {
    /** A host whose restore from a code records the code and runs until the test ends. */
    private class RestoreRecordingHost : NoJoinsHost() {
        val restoredCodes = mutableListOf<String>()

        override suspend fun restoreFromCode(
            code: String,
            credentials: BridgeS3Credentials?,
            deviceName: String,
            callback: BootstrapCallback,
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
                        prefix = "",
                    ),
                oauthAccess = null,
                carriesS3Key = carriesS3Key,
            ),
        )

    private fun newFlow(): OnboardingFlow {
        val flow =
            OnboardingFlow(
                scope,
                RuntimeEnvironment.getApplication(),
                OnboardingCore(host, ::FakeBootstrapCancellation),
                null,
                null,
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

        assertEquals(
            OnboardingStep.Running(OnboardingStep.Restore(CODE, info.metadata, CodeEntry.NewDevice), status = null),
            flow.step,
        )
        assertEquals(listOf(CODE), host.restoredCodes)
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

    private companion object {
        const val CODE = "CVR1-RESTORE-CODE"
    }
}
