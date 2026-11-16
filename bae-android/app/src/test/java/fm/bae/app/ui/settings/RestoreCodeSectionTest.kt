package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.DisplayError
import fm.bae.app.LocalBugReports
import fm.bae.app.diagnosticDetail
import fm.bae.app.ui.TestTheme
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAddDevice
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRestoreCodeShare

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
@OptIn(ExperimentalCoroutinesApi::class)
class RestoreCodeSectionTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun theCodeIsMadeWhenTheSectionOpensAndStaysMaskedUntilShown() {
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RestoreCodeSection(needsS3Key = false, location = "Dropbox", generate = { SHARE })
                }
            }
        }
        compose.onNodeWithText(CODE).assertDoesNotExist()
        compose.onNodeWithContentDescription("Restore code, partly hidden").assertIsDisplayed()
        compose.onNodeWithText("Copy").assertIsDisplayed()
        compose.onNodeWithText("Print").assertIsDisplayed()

        compose.onNodeWithText("Show code").performClick()
        compose.onNodeWithText(CODE).assertIsDisplayed()

        compose.onNodeWithText("Hide").performClick()
        compose.onNodeWithText(CODE).assertDoesNotExist()
        compose.onNodeWithText("Show code").assertIsDisplayed()
    }

    @Test
    fun theCaptionSaysWhereElseTheCodeIs() {
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RestoreCodeSection(needsS3Key = false, location = "Dropbox", generate = { SHARE })
                }
            }
        }

        compose
            .onNodeWithText(
                "Print this code or save it in a password manager. It gets you back into your library if you lose " +
                    "all your devices.",
            ).assertIsDisplayed()
    }

    @Test
    fun anS3LibraryAlsoNamesTheAccessKey() {
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RestoreCodeSection(needsS3Key = true, location = "S3", generate = { SHARE })
                }
            }
        }

        compose.onNodeWithText("You will also need your S3 access key and secret.").assertIsDisplayed()
    }

    @Test
    fun anotherProviderNamesNoAccessKey() {
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RestoreCodeSection(needsS3Key = false, location = "Dropbox", generate = { SHARE })
                }
            }
        }

        compose.onNodeWithText("You will also need your S3 access key and secret.").assertDoesNotExist()
    }

    @Test
    fun theMaskedCodeShowsTheGroupsCoreMasked() {
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RestoreCodeSection(needsS3Key = false, location = "Dropbox", generate = { SHARE })
                }
            }
        }

        compose.onNodeWithText("CVR1 ABCD •••• •••• •••• •••• •••• •••• 4567", useUnmergedTree = true).assertExists()
    }

    @Test
    fun aCodeGeneratedAfterClosingStaysDropped() =
        runTest {
            val pending = CompletableDeferred<BridgeRestoreCodeShare>()
            val reveal = RestoreCodeReveal(this, generate = { pending.await() }, errorDisplay = { null })
            reveal.open()
            advanceUntilIdle()
            assertEquals(RestoreCodeState.Loading, reveal.state)

            reveal.close()
            pending.complete(SHARE)
            advanceUntilIdle()

            assertEquals(RestoreCodeState.Closed, reveal.state)
        }

    @Test
    fun showRevealsTheHeldCodeAndHideMasksItAgain() =
        runTest {
            val reveal = RestoreCodeReveal(this, generate = { SHARE }, errorDisplay = { null })
            reveal.open()
            advanceUntilIdle()
            assertEquals(RestoreCodeState.Masked(SHARE), reveal.state)

            reveal.show()
            assertEquals(RestoreCodeState.Shown(SHARE), reveal.state)
            reveal.hide()
            assertEquals(RestoreCodeState.Masked(SHARE), reveal.state)
        }

    @Test
    fun aNewerRequestReplacesOneStillGenerating() =
        runTest {
            val first = CompletableDeferred<BridgeRestoreCodeShare>()
            var calls = 0
            val reveal =
                RestoreCodeReveal(
                    this,
                    generate = { if (calls++ == 0) first.await() else NEWER },
                    errorDisplay = { null },
                )
            reveal.open()
            advanceUntilIdle()

            reveal.open()
            first.complete(SHARE)
            advanceUntilIdle()

            assertEquals(RestoreCodeState.Masked(NEWER), reveal.state)
        }

    @Test
    fun aFailureWithALineShowsIt() =
        runTest {
            val reveal =
                RestoreCodeReveal(
                    this,
                    generate = { throw BridgeException.Diagnostic(BridgeErrorCategory.Config, "storage unreachable") },
                    errorDisplay = { DisplayError("Could not reach the cloud.", it.diagnosticDetail) },
                )
            reveal.open()
            advanceUntilIdle()

            assertEquals(
                RestoreCodeState.Failed(DisplayError("Could not reach the cloud.", "storage unreachable")),
                reveal.state,
            )
        }

    @Test
    fun aCancellationFromCoreLeavesTheSectionClosed() =
        runTest {
            val reveal =
                RestoreCodeReveal(this, generate = { throw BridgeException.Cancelled() }, errorDisplay = { null })
            reveal.open()
            advanceUntilIdle()

            assertEquals(RestoreCodeState.Closed, reveal.state)
        }

    @Test
    fun addDeviceShowsTheInstructionAndTheQrCodeButNotTheCodesText() {
        compose.setContent {
            TestTheme {
                AddDeviceDialogContent(
                    adding = BridgeAddDevice(id = "pairing", code = CODE, qrText = CODE),
                    face = AddDeviceFace.Code,
                    error = null,
                    onApprove = {},
                    onDecline = {},
                    onDismiss = {},
                )
            }
        }

        // The QR code takes the dialog's width, so what follows it is scrolled to.
        compose.onNodeWithText("Scan or enter the code on the new device.").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Copy code").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Share").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(CODE).assertDoesNotExist()
    }

    private companion object {
        const val CODE = "CVR1 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567"
        val SHARE =
            BridgeRestoreCodeShare(
                code = CODE,
                maskedGroups = listOf("CVR1", "ABCD", "••••", "••••", "••••", "••••", "••••", "••••", "4567"),
                qrText = CODE,
            )
        val NEWER =
            BridgeRestoreCodeShare(code = "CVR1 NEWE R", maskedGroups = listOf("CVR1", "NEWE", "R"), qrText = "CVR1-NEWER")
    }
}
