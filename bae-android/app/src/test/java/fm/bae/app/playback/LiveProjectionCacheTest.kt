package fm.bae.app.playback

import fm.bae.app.data.LiveQueryEvent
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

class LiveProjectionCacheTest {
    @Test
    fun cancelAllCompletesAnActiveWaiterWithTheCloseError() =
        runBlocking {
            val events = Channel<LiveQueryEvent<Int>>(Channel.UNLIMITED)
            val subscribed = CompletableDeferred<Unit>()
            val cache =
                LiveProjectionCache<String, Int>(
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
                    maximumCount = 1,
                    flow = {
                        flow {
                            subscribed.complete(Unit)
                            emitAll(events.receiveAsFlow())
                        }
                    },
                    onEvent = { _, _, _ -> },
                )
            val waiter = async(Dispatchers.IO) { runCatching { cache.value("projection") }.exceptionOrNull() }
            withTimeout(1_000) { subscribed.await() }

            val error = queryFailure()
            cache.cancelAll(error)

            assertEquals(error, withTimeout(1_000) { waiter.await() })
        }

    @Test
    fun evictedProjectionRejectsItsQueuedEvent() =
        runBlocking {
            val first = Channel<LiveQueryEvent<Int>>(Channel.UNLIMITED)
            val second = Channel<LiveQueryEvent<Int>>(Channel.UNLIMITED)
            val accepted = mutableListOf<Int>()
            val cache =
                LiveProjectionCache<String, Int>(
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined),
                    maximumCount = 1,
                    flow = { key -> if (key == "first") first.receiveAsFlow() else second.receiveAsFlow() },
                    onEvent = { _, event, _ ->
                        (event as? LiveQueryEvent.Value)?.let { accepted += it.value }
                    },
                )
            first.send(LiveQueryEvent.Value(1))
            assertEquals(1, cache.value("first"))
            second.send(LiveQueryEvent.Value(2))
            assertEquals(2, cache.value("second"))

            first.send(LiveQueryEvent.Value(99))

            assertTrue(99 !in accepted)
        }

    private fun queryFailure(): BridgeException = BridgeException.Diagnostic(BridgeErrorCategory.DATABASE, "query failed")
}
