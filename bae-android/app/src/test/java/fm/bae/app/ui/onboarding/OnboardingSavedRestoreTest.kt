package fm.bae.app.ui.onboarding

import android.app.Application
import android.os.Looper
import fm.bae.app.DisplayError
import fm.bae.app.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeSavedRestoreCodeInfo

/**
 * A code saved on this phone restores as soon as the person taps it, unless
 * its S3 key has to be typed in first; one deleted since the welcome listed it
 * returns there, saying so.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingSavedRestoreTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    @After
    fun endRestores() {
        scope.cancel()
    }

    private fun newFlow(
        host: SavedCodesHost,
        oauthProvidersError: String? = null,
    ): OnboardingFlow {
        val flow =
            OnboardingFlow(
                scope,
                RuntimeEnvironment.getApplication(),
                OnboardingCore(host, ::FakeBootstrapCancellation),
                oauthProvidersError,
                null,
            ) {}
        shadowOf(Looper.getMainLooper()).idle()
        return flow
    }

    private fun saved(info: BridgeSavedRestoreCodeInfo) = SavedRestoreCode(FakeSavedRestoreCode(info), info)

    @Test
    fun aSavedCodeThatHoldsAllItNeedsRestoresAtOnce() {
        val host = SavedCodesHost(listOf(dropboxCode))
        val flow = newFlow(host)
        val code = saved(dropboxCode)

        flow.openSaved(code)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(OnboardingStep.Running(OnboardingStep.RestoreSaved(code), status = null), flow.step)
        assertEquals(listOf("saved-dropbox" to null), host.restored)
    }

    @Test
    fun anS3CodeWithItsKeySavedBesideItRestoresAtOnce() {
        val info = s3Code(hasS3Credentials = true)
        val host = SavedCodesHost(listOf(info))
        val flow = newFlow(host)

        flow.openSaved(saved(info))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("saved-s3" to null), host.restored)
    }

    @Test
    fun anS3CodeWithNoKeySavedAsksForTheKeyBeforeRestoring() {
        val info = s3Code(hasS3Credentials = false)
        val host = SavedCodesHost(listOf(info))
        val flow = newFlow(host)
        val code = saved(info)

        flow.openSaved(code)
        shadowOf(Looper.getMainLooper()).idle()

        val form = OnboardingStep.RestoreSaved(code)
        assertEquals(form, flow.step)
        assertEquals(emptyList<Pair<String, BridgeS3Credentials?>>(), host.restored)

        val key = BridgeS3Credentials("access-key", "secret")
        flow.restore(form, key)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("saved-s3" to key), host.restored)
    }

    @Test
    fun aCodeDeletedSinceTheWelcomeListedItReturnsThereSayingSo() {
        val host = SavedCodesHost(listOf(dropboxCode)).apply { savedCodeGone = true }
        val flow = newFlow(host)

        flow.openSaved(saved(dropboxCode))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(OnboardingStep.EnterCode(CodeEntry.NewDevice), flow.step)
        val removed = RuntimeEnvironment.getApplication().getString(R.string.onboarding_saved_code_removed)
        assertEquals(DisplayError(removed), flow.error)
    }

    @Test
    fun aCodeNeedingASignInTheAppCantOfferSaysWhyWithoutRestoring() {
        val host = SavedCodesHost(listOf(dropboxCode))
        val flow = newFlow(host, oauthProvidersError = "sign-in settings failed to load")

        flow.openSaved(saved(dropboxCode))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(OnboardingStep.EnterCode(CodeEntry.NewDevice), flow.step)
        assertNotNull(flow.error)
        assertEquals(emptyList<Pair<String, BridgeS3Credentials?>>(), host.restored)
    }

    @Test
    fun aRestoreOverThisDevicesCopyListsNoSavedCodes() {
        val flow =
            OnboardingFlow(
                scope,
                RuntimeEnvironment.getApplication(),
                OnboardingCore(SavedCodesHost(listOf(dropboxCode)), ::FakeBootstrapCancellation),
                null,
                RestoreOver(libraryId = "library-1", title = "Restore this library", message = "Warning", onBack = {}),
            ) {}

        assertEquals(null, flow.savedCodes)
    }
}
