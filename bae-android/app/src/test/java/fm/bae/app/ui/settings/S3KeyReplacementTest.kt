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
 * call the person cancelled too late still tells the page how it ended.
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
            assertEquals(S3KeyReplacement.Line.Saved("OLD"), replacement.line)
            assertEquals(listOf("NEW"), call.received)

            replacement.dismissLine()
            assertNull(replacement.line)
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
            assertEquals(S3KeyReplacement.Line.Saved(null), replacement.line)
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
            assertNull(replacement.line)
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
            assertNull(replacement.line)
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
            assertEquals(S3KeyReplacement.Line.Saved("OLD"), replacement.line)
        }

    @Test
    fun aFailureAfterTheDialogClosedShowsOnThePage() =
        runTest {
            val call = HeldKeyCall()
            val replacement = call.replacement(this)
            replacement.open()
            replacement.save("NEW", "secret")
            replacement.cancel()

            call.end(BridgeAccessKeyOutcome.Failed(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")))
            advanceUntilIdle()
            assertNull(replacement.sheet)
            assertEquals(S3KeyReplacement.Line.Failed(DisplayError("Network", "offline")), replacement.line)
        }

    @Test
    fun aReplacedCallsFailureShowsOnThePageNotInTheDialog() =
        runTest {
            val first = HeldKeyCall()
            val second = HeldKeyCall()
            val replacement =
                S3KeyReplacement(
                    scope = this,
                    useAccessKey = { accessKeyId, _, _ ->
                        if (accessKeyId == "NEW") first.use(accessKeyId) else second.use(accessKeyId)
                    },
                    display = { error -> DisplayError("Network", (error as? BridgeException.Diagnostic)?.detail) },
                    makeCancellation = { RecordedCancellation() },
                )
            replacement.open()
            replacement.save("NEW", "secret")
            replacement.cancel()
            replacement.open()
            replacement.save("NEWER", "secret")

            first.end(BridgeAccessKeyOutcome.Failed(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")))
            advanceUntilIdle()
            assertEquals(S3KeyReplacement.Sheet.Checking, replacement.sheet)
            assertEquals(S3KeyReplacement.Line.Failed(DisplayError("Network", "offline")), replacement.line)

            second.end(BridgeAccessKeyOutcome.Cancelled)
            advanceUntilIdle()
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

    /** Take a call: record its key and wait for the test's outcome. */
    suspend fun use(accessKeyId: String): BridgeAccessKeyOutcome {
        received += accessKeyId
        return outcome.await()
    }

    fun replacement(scope: TestScope) =
        S3KeyReplacement(
            scope = scope,
            useAccessKey = { accessKeyId, _, _ -> use(accessKeyId) },
            display = { error -> DisplayError("Network", (error as? BridgeException.Diagnostic)?.detail) },
            makeCancellation = { cancellation },
        )

    fun end(ended: BridgeAccessKeyOutcome) {
        outcome.complete(ended)
    }
}
