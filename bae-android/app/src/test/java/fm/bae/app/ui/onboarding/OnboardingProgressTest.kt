package fm.bae.app.ui.onboarding

import android.app.Application
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.shownTexts
import fm.bae.app.ui.tellStep
import fm.bae.fastvalues.LatestStream
import fm.bae.fastvalues.only
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeBootstrapState
import uniffi.bae_bridge.BridgeBootstrapStatus
import uniffi.bae_bridge.BridgeBootstrapStep
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeCodeMetadata
import uniffi.bae_bridge.BridgeInviteInfo
import uniffi.bae_bridge.BridgeLoadProgress
import uniffi.bae_bridge.BridgeLoadStep
import uniffi.bae_bridge.BridgeStorageLocation

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class OnboardingProgressTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val invite =
        OnboardingStep.Invite(
            start = JoinStart.Code("invite-code", "Dan"),
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
                                    folderLabel = null,
                                ),
                            oauthAccess = null,
                            carriesS3Key = false,
                        ),
                    libraryName = "Household",
                    inviter = "Ana",
                ),
            entry = CodeEntry.NewDevice,
        )

    /** A waiting join tells the code to the inviter, with no countdown, Check now, date or Cancel. */
    @Test
    fun aWaitingJoinSaysWhoToTellTheNumberAndWhoItWaitsForButNoCountdownCheckNowOrCancel() {
        val waiting =
            OnboardingStep.Running(
                invite,
                BridgeBootstrapState(
                    BridgeBootstrapStatus.WaitingForApproval(confirmation = "041 778"),
                    cancellable = false,
                ),
                LatestStream.bootstrapSteps().only,
            )
        compose.mainClock.autoAdvance = false
        compose.setContent {
            BaeTheme { OnboardingProgress(waiting, onCancel = null, onBack = {}) }
        }

        compose.onNodeWithText("Join library").assertIsDisplayed()
        compose.onNodeWithText("Tell Ana this code").assertIsDisplayed()
        compose.onNodeWithText("041 778").assertIsDisplayed()
        compose.onNodeWithText("Waiting for Ana to let you in…").assertIsDisplayed()
        compose.onNodeWithText("Next check", substring = true).assertDoesNotExist()
        compose.onNodeWithText("Check now").assertDoesNotExist()
        compose.onNodeWithText("Waiting for approval until", substring = true).assertDoesNotExist()
        compose.onNodeWithText("Cancel").assertDoesNotExist()
    }

    /** A join that has not reached approval is headed by the library's name, with Cancel while it can. */
    @Test
    fun aJoinBeforeApprovalIsHeadedByTheLibraryNameWithCancel() {
        val connecting =
            OnboardingStep.Running(
                invite,
                BridgeBootstrapState(BridgeBootstrapStatus.Connecting, cancellable = true),
                LatestStream.bootstrapSteps().only,
            )
        compose.mainClock.autoAdvance = false
        compose.setContent {
            BaeTheme { OnboardingProgress(connecting, onCancel = {}, onBack = {}) }
        }

        compose.onNodeWithText("Household").assertIsDisplayed()
        compose.onNodeWithText("Connecting…").assertIsDisplayed()
        compose.onNodeWithText("Cancel").assertIsDisplayed()
        compose.onNodeWithText("Tell Ana this code").assertDoesNotExist()
    }

    /** Registering this device after loading names the step, uncounted. */
    @Test
    fun registeringNamesItsStep() {
        val status = BridgeBootstrapStatus.Registering
        compose.setContent { StatusLines(status) }
        compose.onNodeWithText("Registering this device…").assertIsDisplayed()
    }

    /** The last work before the library opens names the step, uncounted. */
    @Test
    fun committingNamesItsStep() {
        val status = BridgeBootstrapStatus.Committing
        compose.setContent { StatusLines(status) }
        compose.onNodeWithText("Finishing…").assertIsDisplayed()
    }

    /**
     * Loading the library shows its step's line, drawn by a View from the call's steps, in place of the status lines. The
     * step's catalog key comes in the progress the bridge hands the screen.
     */
    @Test
    fun loadingTheLibraryNamesItsStep() {
        val loading =
            BridgeBootstrapStep.Loading(
                BridgeLoadProgress(BridgeLoadStep.READING, "core.library.load_step.reading", done = 2uL, total = 8uL),
            )
        val steps = LatestStream.written<BridgeBootstrapStep>()
        compose.setContent {
            BaeTheme {
                BootstrapStepLine(steps = steps.source(STEP_KEY)) { StatusLines(BridgeBootstrapStatus.Loading) }
            }
        }
        // The rule hands out its activity only after running the main looper until idle, which never comes while a
        // step is held, so the root is taken first.
        val root = compose.activity.window.decorView

        steps.tellStep(STEP_KEY, loading)

        val shown = root.shownTexts()
        steps.tellStep(STEP_KEY, null)
        assertTrue(shown.toString(), shown.any { it.contains("Downloading recent changes") })
    }

    private companion object {
        const val STEP_KEY = "restore"
    }
}
