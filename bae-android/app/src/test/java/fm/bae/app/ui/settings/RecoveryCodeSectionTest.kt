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
import fm.bae.app.ui.BaeTheme
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeRestoreCodeShare

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
@OptIn(ExperimentalCoroutinesApi::class)
class RecoveryCodeSectionTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun theCodeIsMadeWhenTheSectionOpensAndStaysMaskedUntilShown() {
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RecoveryCodeSection(needsS3Key = false, location = "Dropbox", generate = { CODE })
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
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RecoveryCodeSection(needsS3Key = false, location = "Dropbox", generate = { CODE })
                }
            }
        }

        compose
            .onNodeWithText(
                "Print this code or save it in a password manager. It gets you back into your library if you lose " +
                    "all your devices. A phone you set up from this one gets it too.",
            ).assertIsDisplayed()
    }

    @Test
    fun anS3LibraryAlsoNamesTheAccessKey() {
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RecoveryCodeSection(needsS3Key = true, location = "S3", generate = { CODE })
                }
            }
        }

        compose.onNodeWithText("You’ll also need your S3 access key and secret.").assertIsDisplayed()
    }

    @Test
    fun anotherProviderNamesNoAccessKey() {
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    RecoveryCodeSection(needsS3Key = false, location = "Dropbox", generate = { CODE })
                }
            }
        }

        compose.onNodeWithText("You’ll also need your S3 access key and secret.").assertDoesNotExist()
    }

    @Test
    fun aMaskedCodeKeepsItsFirstTwoGroups() {
        assertEquals("CVR1 ABCD •••• •••• ••••", maskedRestoreCode("CVR1 ABCD EFGH IJKL MNOP"))
        assertEquals("CVR1 ABCD", maskedRestoreCode("CVR1 ABCD"))
    }

    @Test
    fun aCodeGeneratedAfterClosingStaysDropped() =
        runTest {
            val pending = CompletableDeferred<String>()
            val reveal = RecoveryCodeReveal(this, generate = { pending.await() }, errorDisplay = { null })
            reveal.open()
            advanceUntilIdle()
            assertEquals(RecoveryCodeState.Loading, reveal.state)

            reveal.close()
            pending.complete(CODE)
            advanceUntilIdle()

            assertEquals(RecoveryCodeState.Closed, reveal.state)
        }

    @Test
    fun showRevealsTheHeldCodeAndHideMasksItAgain() =
        runTest {
            val reveal = RecoveryCodeReveal(this, generate = { CODE }, errorDisplay = { null })
            reveal.open()
            advanceUntilIdle()
            assertEquals(RecoveryCodeState.Masked(CODE), reveal.state)

            reveal.show()
            assertEquals(RecoveryCodeState.Shown(CODE), reveal.state)
            reveal.hide()
            assertEquals(RecoveryCodeState.Masked(CODE), reveal.state)
        }

    @Test
    fun aNewerRequestReplacesOneStillGenerating() =
        runTest {
            val first = CompletableDeferred<String>()
            var calls = 0
            val reveal =
                RecoveryCodeReveal(
                    this,
                    generate = { if (calls++ == 0) first.await() else "CVR1 NEWER" },
                    errorDisplay = { null },
                )
            reveal.open()
            advanceUntilIdle()

            reveal.open()
            first.complete(CODE)
            advanceUntilIdle()

            assertEquals(RecoveryCodeState.Masked("CVR1 NEWER"), reveal.state)
        }

    @Test
    fun aFailureWithALineShowsIt() =
        runTest {
            val reveal =
                RecoveryCodeReveal(
                    this,
                    generate = { throw BridgeException.Diagnostic(BridgeErrorCategory.Config, "storage unreachable") },
                    errorDisplay = { DisplayError("Couldn’t reach your storage.", it.diagnosticDetail) },
                )
            reveal.open()
            advanceUntilIdle()

            assertEquals(
                RecoveryCodeState.Failed(DisplayError("Couldn’t reach your storage.", "storage unreachable")),
                reveal.state,
            )
        }

    @Test
    fun aCancellationFromCoreLeavesTheSectionClosed() =
        runTest {
            val reveal =
                RecoveryCodeReveal(this, generate = { throw BridgeException.Cancelled() }, errorDisplay = { null })
            reveal.open()
            advanceUntilIdle()

            assertEquals(RecoveryCodeState.Closed, reveal.state)
        }

    @Test
    fun addDeviceShowsTheCodeToTypeAndTheOnboardingChoice() {
        compose.setContent {
            BaeTheme {
                AddDeviceDialogContent(
                    share = BridgeRestoreCodeShare(code = CODE, qrText = CODE, qrCarriesS3Key = false),
                    added = null,
                    error = null,
                    onDismiss = {},
                )
            }
        }

        // The QR code takes the dialog's width, so what follows it is scrolled to.
        compose
            .onNodeWithText("On the new device, choose “Add device”, then scan this or type:")
            .performScrollTo()
            .assertIsDisplayed()
        compose.onNodeWithText(CODE).performScrollTo().assertIsDisplayed()
        compose
            .onNodeWithText("The QR code also carries your S3 access key, so a device that scans it won't ask for the key.")
            .assertDoesNotExist()
    }

    @Test
    fun addDeviceSaysTheQrCarriesTheS3Key() {
        compose.setContent {
            BaeTheme {
                AddDeviceDialogContent(
                    share = BridgeRestoreCodeShare(code = CODE, qrText = CODE_WITH_KEY, qrCarriesS3Key = true),
                    added = null,
                    error = null,
                    onDismiss = {},
                )
            }
        }

        compose
            .onNodeWithText("The QR code also carries your S3 access key, so a device that scans it won't ask for the key.")
            // Below the QR code and the code, past the dialog's visible height.
            .assertExists()
    }

    private companion object {
        const val CODE = "CVR1 ABCD EFGH IJKL MNOP QRST UVWX YZ23 4567"

        /** The same restore code, longer for the S3 key it holds. */
        const val CODE_WITH_KEY = "CVR1-ABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOP"
    }
}
