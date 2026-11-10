package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.BaeTheme
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeStorageLocation

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingRestoreTextTest {
    @get:Rule
    val compose = createComposeRule()

    private val metadata =
        BridgeCodeMetadata(
            libraryId = "library-1",
            memberId = "member-1",
            provider = BridgeCloudProvider.ONE_DRIVE,
            location =
                BridgeStorageLocation.OneDrive(
                    driveId = "drive-1",
                    folderId = "folder-1",
                    accountLabel = "person@example.com",
                    folderLabel = null,
                ),
            oauthAccess = null,
            carriesS3Key = false,
        )

    private val restoreOver =
        RestoreOver(
            libraryId = "library-1",
            title = "Restore this library",
            message = "Warning",
            onBack = {},
            start = RestoreOver.Start.CODE,
        )

    private fun restore(entry: CodeEntry) = OnboardingStep.Restore("restore-code", metadata, entry)

    @Test
    fun aRestoreCodeOnANewDeviceAddsThisDevice() {
        compose.setContent { BaeTheme { RestoreForm(restore(CodeEntry.NewDevice), null, {}, {}) } }

        compose.onNodeWithText("Add device").assertIsDisplayed()
        compose.onNodeWithText("Add").assertIsDisplayed()
        compose.onNodeWithText("Restore").assertDoesNotExist()
    }

    @Test
    fun restoringOverThisDevicesCopyStillRestores() {
        compose.setContent { BaeTheme { RestoreForm(restore(restoreOver), null, {}, {}) } }

        compose.onNodeWithText("Restore your library").assertIsDisplayed()
        compose.onNodeWithText("Restore").assertIsDisplayed()
        compose.onNodeWithText("Add").assertDoesNotExist()
    }

    @Test
    fun addingThisDeviceHasNoHeadingUnderTheBar() {
        // The spinner animates forever; a frozen clock lets the content settle.
        compose.mainClock.autoAdvance = false
        compose.setContent {
            BaeTheme {
                OnboardingProgress(
                    OnboardingStep.Running(
                        restore(CodeEntry.NewDevice),
                        BridgeBootstrapState(BridgeBootstrapStatus.Connecting, cancellable = true),
                        LatestStream.bootstrapSteps().only,
                    ),
                    onCancel = {},
                    onCheckNow = {},
                )
            }
        }

        compose.onNodeWithText("Adding this device").assertDoesNotExist()
    }

    @Test
    fun restoringOverThisDevicesCopySaysRestoringWhileItRuns() {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            BaeTheme {
                OnboardingProgress(
                    OnboardingStep.Running(
                        restore(restoreOver),
                        BridgeBootstrapState(BridgeBootstrapStatus.Connecting, cancellable = true),
                        LatestStream.bootstrapSteps().only,
                    ),
                    onCancel = {},
                    onCheckNow = {},
                )
            }
        }

        compose.onNodeWithText("Restoring your library").assertIsDisplayed()
    }
}
