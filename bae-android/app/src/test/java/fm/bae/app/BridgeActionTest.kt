package fm.bae.app

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackErrorReason

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BridgeActionTest {
    @Test
    fun bridgeFailureIsReportedInsteadOfEscapingTheActionCoroutine() {
        val shown = mutableListOf<DisplayError?>()

        runBlocking {
            performBridgeAction(
                logger = BaeLogger("bae.BridgeActionTest"),
                operation = "test operation",
                errors = TestErrorDisplays,
                showError = { shown.add(it) },
            ) {
                throw BridgeException.Diagnostic(BridgeErrorCategory.Internal, "diagnostic detail")
            }
        }

        assertEquals(listOf(DisplayError("localized bridge error", "diagnostic detail")), shown)
    }

    @Test
    fun unexpectedFailureIsReportedInsteadOfEscapingTheActionCoroutine() {
        val shown = mutableListOf<DisplayError?>()

        runBlocking {
            performBridgeAction(
                logger = BaeLogger("bae.BridgeActionTest"),
                operation = "test operation",
                errors = TestErrorDisplays,
                showError = { shown.add(it) },
            ) {
                error("unexpected failure")
            }
        }

        assertEquals(listOf(DisplayError("java.lang.IllegalStateException: unexpected failure")), shown)
    }

    @Test
    fun coroutineCancellationStillPropagates() {
        assertThrows(CancellationException::class.java) {
            runBlocking {
                performBridgeAction(
                    logger = BaeLogger("bae.BridgeActionTest"),
                    operation = "test operation",
                    errors = TestErrorDisplays,
                    showError = {},
                ) {
                    throw CancellationException("cancelled")
                }
            }
        }
    }

    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason) = DisplayError("localized playback error", reason.diagnosticDetail)

        override fun display(error: BridgeException) = DisplayError("localized bridge error", error.diagnosticDetail)
    }
}
