package fm.bae.app.ui.settings

import fm.bae.app.DisplayError
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeAccessKeyOutcome
import uniffi.bae_bridge.BridgeCancellation
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.NoHandle

/**
 * Replacing the S3 key from the Sync page: the dialog shows each way the call ends, Cancel stops the check, and a
 * save the person cancelled too late still names the old key.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class S3KeyReplacementTest {
    @Test
    fun aSavedKeyClosesTheDialogAndNamesTheOldKey() =
        runTest {
            val call = HeldKeyCall()
            val replacement = call.replacement(this)
            replacement.open()
            replacement.save("NEW", "secret")
            assertEquals(S3KeyReplacement.Sheet.Checking, replacement.sheet)

            call.end(BridgeAccessKeyOutcome.Saved("OLD"))
            advanceUntilIdle()
            assertNull(replacement.sheet)
            assertEquals(S3KeyReplacement.Saved("OLD"), replacement.saved)
            assertEquals(listOf("NEW"), call.received)

            replacement.dismissSaved()
            assertNull(replacement.saved)
        }

    @Test
    fun aKeyTheLibraryRecordsIsSavedWithNoOldKey() =
        runTest {
            val call = HeldKeyCall()
            val replacement = call.replacement(this)
            replacement.open()
            replacement.save("NEW", "secret")
            call.end(BridgeAccessKeyOutcome.Saved(null))
            advanceUntilIdle()
            assertEquals(S3KeyReplacement.Saved(null), replacement.saved)
        }

    @Test
    fun aRefusedKeyShowsTheFieldsAgainWithWhy() =
        runTest {
            val call = HeldKeyCall()
            val replacement = call.replacement(this)
            replacement.open()
            replacement.save("NEW", "secret")
            call.end(BridgeAccessKeyOutcome.Refused)
            advanceUntilIdle()
            assertEquals(S3KeyReplacement.Sheet.Fields(S3KeyReplacement.Failure.Refused), replacement.sheet)
            assertNull(replacement.saved)
        }

    @Test
    fun aFailedKeyShowsTheFieldsAgainWithItsLine() =
        runTest {
            val call = HeldKeyCall()
            val replacement = call.replacement(this)
            replacement.open()
            replacement.save("NEW", "secret")
            call.end(BridgeAccessKeyOutcome.Failed(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")))
            advanceUntilIdle()
            assertEquals(
                S3KeyReplacement.Sheet.Fields(S3KeyReplacement.Failure.Failed(DisplayError("Network", "offline"))),
                replacement.sheet,
            )
        }

    @Test
    fun cancelWhileCheckingClosesTheDialogAndStopsTheCheck() =
        runTest {
            val call = HeldKeyCall()
            val replacement = call.replacement(this)
            replacement.open()
            replacement.save("NEW", "secret")
            advanceUntilIdle()

            replacement.cancel()
            assertNull(replacement.sheet)
            assertTrue(call.cancellation.cancelled)

            call.end(BridgeAccessKeyOutcome.Cancelled)
            advanceUntilIdle()
            assertNull(replacement.sheet)
            assertNull(replacement.saved)
        }

    @Test
    fun aSaveWhoseCheckPassedBeforeTheCancelStillNamesTheOldKey() =
        runTest {
            val call = HeldKeyCall()
            val replacement = call.replacement(this)
            replacement.open()
            replacement.save("NEW", "secret")
            replacement.cancel()

            call.end(BridgeAccessKeyOutcome.Saved("OLD"))
            advanceUntilIdle()
            assertNull(replacement.sheet)
            assertEquals(S3KeyReplacement.Saved("OLD"), replacement.saved)
        }
}

/** A cancel that records whether it was sent, with no Rust object behind it. */
private class RecordedCancellation : BridgeCancellation(NoHandle) {
    var cancelled = false

    override fun cancel() {
        cancelled = true
    }
}

/** A key call that waits until the test ends it with an outcome. */
private class HeldKeyCall {
    val received = mutableListOf<String>()
    val cancellation = RecordedCancellation()
    private val outcome = CompletableDeferred<BridgeAccessKeyOutcome>()

    fun replacement(scope: TestScope) =
        S3KeyReplacement(
            scope = scope,
            useAccessKey = { accessKeyId, _, _ ->
                received += accessKeyId
                outcome.await()
            },
            display = { error -> DisplayError("Network", (error as? BridgeException.Diagnostic)?.detail) },
            makeCancellation = { cancellation },
        )

    fun end(ended: BridgeAccessKeyOutcome) {
        outcome.complete(ended)
    }
}
