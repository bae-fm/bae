package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeInviteDeliveryKind
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeLoadProgress
import uniffi.bae_bridge.BridgeLoadStep
import uniffi.bae_bridge.BridgeNextCheck
import uniffi.bae_bridge.BridgeStorageLocation
import java.time.Instant

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingProgressTest {
    @get:Rule
    val compose = createComposeRule()

    private val invite =
        OnboardingStep.Invite(
            start = JoinStart.Code("invite-code"),
            invite =
                BridgeInviteInfo(
                    metadata =
                        BridgeCodeMetadata(
                            libraryId = "library-1",
                            memberId = "member-1",
                            provider = BridgeCloudProvider.ONE_DRIVE,
                            location =
                                BridgeStorageLocation.OneDrive(
                                    driveId = "drive-1",
                                    folderId = "folder-1",
                                    accountLabel = "person@example.com",
                                ),
                            oauthAccess = null,
                            carriesS3Key = false,
                        ),
                    libraryName = "Household",
                    invitee = "Dan",
                    delivery = BridgeInviteDeliveryKind.SENT,
                    expiresAt = Instant.parse("2026-11-03T16:15:00Z"),
                ),
            entry = CodeEntry.NewDevice,
        )

    /** A sent invite's join, waiting, shows until when it waits: the invite's expiry. */
    @Test
    fun aSentInvitesJoinShowsTheLibraryUntilWhenItWaitsItsNumberAndCheckNowButNoCancel() {
        val waiting =
            OnboardingStep.Running(
                invite,
                BridgeBootstrapState(
                    BridgeBootstrapStatus.WaitingForApproval(
                        confirmation = "041 778",
                        nextCheck = BridgeNextCheck.At(Instant.now().plusSeconds(300)),
                    ),
                    cancellable = false,
                ),
            )
        compose.mainClock.autoAdvance = false
        compose.setContent {
            OnboardingProgress(waiting, onCancel = null, onCheckNow = {})
        }

        compose.onNodeWithText("Household", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Waiting for approval until", substring = true).assertIsDisplayed()
        compose.onNodeWithText("2026", substring = true).assertIsDisplayed()
        compose.onNodeWithText("041 778").assertIsDisplayed()
        compose.onNodeWithText("Check now").assertIsDisplayed()
        compose.onNodeWithText("Cancel").assertDoesNotExist()
    }

    /** Registering this device after loading names the step, uncounted. */
    @Test
    fun registeringNamesItsStep() {
        val status = BridgeBootstrapStatus.Registering
        assertFalse(status.hasStepBar())
        compose.setContent { StatusLines(status) }
        compose.onNodeWithText("Registering this device…").assertIsDisplayed()
    }

    /** The last work before the library opens names the step, uncounted. */
    @Test
    fun committingNamesItsStep() {
        val status = BridgeBootstrapStatus.Committing
        assertFalse(status.hasStepBar())
        compose.setContent { StatusLines(status) }
        compose.onNodeWithText("Finishing…").assertIsDisplayed()
    }

    /**
     * Loading the library shows its step's line, not a bare spinner. The step's catalog key comes from the bridge, which a
     * unit test can't load, so the test names it.
     */
    @Test
    fun loadingTheLibraryNamesItsStep() {
        val status = BridgeBootstrapStatus.Loading(BridgeLoadProgress(BridgeLoadStep.READING, done = 2uL, total = 8uL))
        assertTrue(status.hasStepBar())
        compose.setContent {
            StatusLines(status, loadStepKey = { "core.library.load_step.reading" })
        }
        compose.onNodeWithText("Downloading recent changes", substring = true).assertIsDisplayed()
    }
}
