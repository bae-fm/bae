package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeStorageLocation

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingProgressTest {
    @get:Rule
    val compose = createComposeRule()

    private val invite =
        OnboardingStep.Invite(
            code = "invite-code",
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
            entry = CodeEntry.NewDevice,
        )

    @Test
    fun aJoinWaitingForApprovalNamesTheLibraryAndTheWait() {
        // The spinner animates forever; a frozen clock lets the content settle.
        compose.mainClock.autoAdvance = false
        compose.setContent {
            OnboardingProgress(OnboardingStep.Running(invite, BridgeBootstrapStatus.WaitingForApproval), onCancel = {})
        }

        compose.onNodeWithText("Household", substring = true).assertIsDisplayed()
        compose.onNodeWithText("approve", substring = true).assertIsDisplayed()
    }
}
