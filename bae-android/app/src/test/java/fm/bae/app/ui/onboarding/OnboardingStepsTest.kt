package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import fm.bae.app.playback.unfaked
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BootstrapCallback
import uniffi.bae_bridge.BootstrapCancellation
import uniffi.bae_bridge.BridgeBootstrapResult
import uniffi.bae_bridge.BridgeDeviceReset
import uniffi.bae_bridge.BridgeJoinControl
import uniffi.bae_bridge.BridgeLibrary
import uniffi.bae_bridge.BridgeLibraryRemoval
import uniffi.bae_bridge.BridgeS3Credentials
import uniffi.bae_bridge.BridgeSavedRestoreCode
import uniffi.bae_bridge.BridgeSavedRestoreEntry
import uniffi.bae_bridge.BridgeUnfinishedJoin
import uniffi.bae_bridge.BridgeUnfinishedRemoval
import uniffi.bae_bridge.BridgeUnsentWork
import uniffi.bae_bridge.OpenedLibrary

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingStepsTest {
    @get:Rule
    val compose = createComposeRule()

    /** A device with no join under way; opening the code field and going back call no other core. */
    private class NoJoinsHost : EditionFakeBridgeHost() {
        override suspend fun unfinishedJoins(): List<BridgeUnfinishedJoin> = emptyList()

        override suspend fun createLibrary(name: String?): OpenedLibrary = unfaked("createLibrary")

        override suspend fun finishRemovals() {
            unfaked("finishRemovals")
        }

        override suspend fun flushDiagnostics() {
            unfaked("flushDiagnostics")
        }

        override suspend fun listLibraries(): List<BridgeLibrary> = unfaked("listLibraries")

        override suspend fun removeUnopenableLibrary(libraryId: String): BridgeLibraryRemoval = unfaked("removeUnopenableLibrary")

        override suspend fun unfinishedRemoval(): BridgeUnfinishedRemoval? = unfaked("unfinishedRemoval")

        override suspend fun unsentBeforeRemoval(libraryId: String): BridgeUnsentWork = unfaked("unsentBeforeRemoval")

        override suspend fun forgetSavedRestoreCode(libraryId: String) {
            unfaked("forgetSavedRestoreCode")
        }

        override suspend fun joinWithInvite(
            code: String,
            deviceName: String,
            callback: BootstrapCallback,
            control: BridgeJoinControl,
        ): BridgeBootstrapResult = unfaked("joinWithInvite")

        override fun prepareDeviceReset(
            libraryId: String,
            deviceName: String,
        ): BridgeDeviceReset = unfaked("prepareDeviceReset")

        override suspend fun restoreFromCode(
            code: String,
            credentials: BridgeS3Credentials?,
            deviceName: String,
            callback: BootstrapCallback,
            cancellation: BootstrapCancellation,
        ): BridgeBootstrapResult = unfaked("restoreFromCode")

        override suspend fun restoreFromKeychain(
            saved: BridgeSavedRestoreCode,
            credentials: BridgeS3Credentials?,
            deviceName: String,
            callback: BootstrapCallback,
            cancellation: BootstrapCancellation,
        ): BridgeBootstrapResult = unfaked("restoreFromKeychain")

        override suspend fun restoreReplacingDevice(
            libraryId: String,
            code: String,
            credentials: BridgeS3Credentials?,
            deviceName: String,
            callback: BootstrapCallback,
            cancellation: BootstrapCancellation,
        ): BridgeBootstrapResult = unfaked("restoreReplacingDevice")

        override suspend fun resumeJoin(
            join: BridgeUnfinishedJoin,
            callback: BootstrapCallback,
            control: BridgeJoinControl,
        ): BridgeBootstrapResult = unfaked("resumeJoin")

        override suspend fun savedRestoreCodes(): List<BridgeSavedRestoreEntry> = unfaked("savedRestoreCodes")
    }

    /**
     * Onboarding on a device with no join under way. The test screen is
     * shorter than a code screen, so what sits below the fold is scrolled to,
     * as a person would.
     */
    private fun showOnboarding(restoreOver: RestoreOver? = null) {
        compose.setContent {
            val context = LocalContext.current
            val scope = rememberCoroutineScope()
            val flow =
                remember {
                    OnboardingFlow(scope, context, NoJoinsHost(), null, restoreOver) {}
                }
            BaeTheme { OnboardingSteps(flow) }
        }
    }

    @Test
    fun aNewDeviceOpensOnScanningOrEnteringACode() {
        showOnboarding()

        compose.onNodeWithText("Scan a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Enter a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Back").assertDoesNotExist()
    }

    @Test
    fun enterACodeOpensTheFieldToTypeIt() {
        showOnboarding()

        compose.onNodeWithText("Enter a code").performScrollTo().performClick()

        compose.onNodeWithText("Recovery code or invite").assertIsDisplayed()
    }

    @Test
    fun restoringOverThisDevicesCopyStartsAtTheCodeAndBackLeavesOnboarding() {
        var left = false
        showOnboarding(
            RestoreOver(
                libraryId = "library-1",
                title = "Restore this library",
                message = "Another of your devices replaced your keys.",
                onBack = { left = true },
            ),
        )

        compose.onNodeWithText("Restore this library").assertIsDisplayed()
        compose.onNodeWithText("Scan a code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Back").performScrollTo().performClick()

        assertTrue(left)
    }
}
