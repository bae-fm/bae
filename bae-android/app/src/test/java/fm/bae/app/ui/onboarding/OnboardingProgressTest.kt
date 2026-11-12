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
import java.time.Instant

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
                    expiresAt = Instant.parse("2026-11-03T16:15:00Z"),
                ),
            entry = CodeEntry.NewDevice,
        )

    /** A join, waiting, shows until when it waits: the invite's expiry. */
    @Test
    fun aWaitingJoinShowsTheLibraryUntilWhenItWaitsAndItsNumberButNoCountdownCheckNowOrCancel() {
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
            OnboardingProgress(waiting, onCancel = null)
        }

        compose.onNodeWithText("Household", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Waiting for approval until", substring = true).assertIsDisplayed()
        compose.onNodeWithText("2026", substring = true).assertIsDisplayed()
        compose.onNodeWithText("041 778").assertIsDisplayed()
        compose.onNodeWithText("Next check", substring = true).assertDoesNotExist()
        compose.onNodeWithText("Check now").assertDoesNotExist()
        compose.onNodeWithText("Cancel").assertDoesNotExist()
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
