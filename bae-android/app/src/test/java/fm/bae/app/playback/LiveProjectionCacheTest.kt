package fm.bae.app.playback

import fm.bae.app.data.LiveQueryEvent
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class LiveProjectionCacheTest {
    @Test
    fun delayedAcceptedReadCannotRegressNewerValueBookkeeping() =
        runBlocking {
            val events = Channel<LiveQueryEvent<Int>>(Channel.UNLIMITED)
            val count = AcceptedCount()
            val initialApplied = CompletableDeferred<Unit>()
            val newerApplied = CompletableDeferred<Unit>()
            val errorCount = CompletableDeferred<Int?>()
            val oldReadPaused = CountDownLatch(1)
            val resumeOldRead = CountDownLatch(1)
            val pauseOldRead = AtomicBoolean()
            val cache =
                LiveProjectionCache<String, Int>(
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
                    maximumCount = 1,
                    flow = { events.receiveAsFlow() },
                    isRetained = { true },
                    onAcceptedValue = { _, accepted ->
                        if (pauseOldRead.get() && accepted.value == 1) {
                            oldReadPaused.countDown()
                            assertTrue(resumeOldRead.await(1, TimeUnit.SECONDS))
                        }
                        count.accept(accepted.sequence, accepted.value)
                        if (accepted.value == 1) initialApplied.complete(Unit)
                        if (accepted.value == 2) newerApplied.complete(Unit)
                    },
                    onError = { _, error -> errorCount.complete(count.acceptError(error.sequence)) },
                )
            cache.ensure("projection")
            events.send(LiveQueryEvent.Value(1))
            withTimeout(1_000) { initialApplied.await() }
            pauseOldRead.set(true)

            val oldRead = async(Dispatchers.IO) { cache.value("projection") }
            assertTrue(oldReadPaused.await(1, TimeUnit.SECONDS))
            events.send(LiveQueryEvent.Value(2))
            withTimeout(1_000) { newerApplied.await() }
            resumeOldRead.countDown()
            assertEquals(1, oldRead.await())

            events.send(LiveQueryEvent.Error(queryFailure()))
            assertEquals(2, withTimeout(1_000) { errorCount.await() })
            cache.cancelAll()
        }

    private fun queryFailure(): BridgeException =
        BridgeException.Diagnostic(
            category = BridgeErrorCategory.DATABASE,
            detail = "query failed",
        )
}
