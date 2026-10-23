package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeInviteDeliveryKind
import uniffi.bae_bridge.BridgeInviteInfo
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
                        ),
                    libraryName = "Household",
                    invitee = "Dan",
                    delivery = BridgeInviteDeliveryKind.SENT,
                ),
            entry = CodeEntry.NewDevice,
        )

    @Test
    fun aSentInvitesJoinShowsTheLibraryItsNumberAndCheckNowButNoCancel() {
        val waiting =
            OnboardingStep.Running(
                invite,
                BridgeBootstrapStatus.WaitingForApproval(
                    confirmation = "041 778",
                    nextCheck = BridgeNextCheck.At(Instant.now().plusSeconds(300)),
                ),
            )
        assertFalse(waiting.cancellable())
        compose.mainClock.autoAdvance = false
        compose.setContent {
            OnboardingProgress(waiting, onCancel = null, onCheckNow = {})
        }

        compose.onNodeWithText("Household", substring = true).assertIsDisplayed()
        compose.onNodeWithText("041 778").assertIsDisplayed()
        compose.onNodeWithText("Check now").assertIsDisplayed()
        compose.onNodeWithText("Cancel").assertDoesNotExist()
    }

    @Test
    fun aJoinStillConnectingCanBeCancelled() {
        assertTrue(OnboardingStep.Running(invite, BridgeBootstrapStatus.Connecting).cancellable())
    }
}
