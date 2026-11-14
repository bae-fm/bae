package fm.bae.app.playback

import fm.bae.app.HANG_GUARD
import fm.bae.app.data.ListReads
import fm.bae.screensubscription.PulledSubscription
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.bae_bridge.BridgeListCount

class ListProjectionCloseTest {
    @Test
    fun closeStopsConsumerWhenNativeCancellationFails() =
        runBlocking {
            val count = FailingCancelCount()
            val reads =
                ListReads<String>(
                    window = { error("no window is read") },
                    count = { count },
                    position = { error("no position is read") },
                    keyAt = { null },
                )
            val projection =
                ListProjection<String, String>(
                    CoroutineScope(SupervisorJob() + Dispatchers.Default),
                    reads,
                    { it },
                    onChanged = {},
                    onError = {},
                )
            count.started.await()

            assertThrows(IllegalStateException::class.java) {
                runBlocking { projection.close() }
            }

            withTimeout(HANG_GUARD) { count.stopped.await() }
        }

    private class FailingCancelCount : PulledSubscription<BridgeListCount> {
        val started = CompletableDeferred<Unit>()
        val stopped = CompletableDeferred<Unit>()

        override suspend fun next(): BridgeListCount {
            started.complete(Unit)
            try {
                awaitCancellation()
            } finally {
                stopped.complete(Unit)
            }
        }

        override suspend fun cancel(): Unit = throw IllegalStateException("native cancellation failed")
    }
}
