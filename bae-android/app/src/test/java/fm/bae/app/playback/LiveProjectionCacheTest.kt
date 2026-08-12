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
    fun acceptedParentChangeCannotBeSuppressedByLaterPageRead() =
        runBlocking {
            val observerEvents = Channel<LiveQueryEvent<Int>>(Channel.UNLIMITED)
            val pageEvents = Channel<LiveQueryEvent<Int>>(Channel.UNLIMITED)
            val sequence = AcceptedEventSequence()
            val count = AcceptedCount()
            val baselineApplied = CompletableDeferred<Unit>()
            val pageApplied = CompletableDeferred<Unit>()
            val observerPaused = CountDownLatch(1)
            val resumeObserver = CountDownLatch(1)
            val pauseObserver = AtomicBoolean()
            val pageRetired = AtomicBoolean()
            val notifications = Channel<Int>(Channel.UNLIMITED)
            lateinit var pageCache: LiveProjectionCache<String, Int>
            pageCache =
                LiveProjectionCache<String, Int>(
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
                    maximumCount = 1,
                    flow = { pageEvents.receiveAsFlow() },
                    isRetained = { true },
                    acceptedEventSequence = sequence,
                    onAcceptedRead = { _, accepted -> count.acceptRead(accepted.sequence, accepted.value) },
                    onAcceptedChange = { _, accepted ->
                        (accepted.event as? LiveQueryEvent.Value)?.let { value ->
                            count.acceptRead(accepted.sequence, value.value)
                            pageApplied.complete(Unit)
                        }
                    },
                )
            val observerCache =
                LiveProjectionCache<String, Int>(
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
                    maximumCount = 1,
                    flow = { observerEvents.receiveAsFlow() },
                    isRetained = { true },
                    acceptedEventSequence = sequence,
                    onAcceptedChange = { _, accepted ->
                        if (pauseObserver.get()) {
                            observerPaused.countDown()
                            assertTrue(resumeObserver.await(1, TimeUnit.SECONDS))
                        }
                        val notificationCount =
                            when (val event = accepted.event) {
                                is LiveQueryEvent.Value ->
                                    count.commitValue(accepted.sequence, event.value, accepted.isInitial)

                                is LiveQueryEvent.Error -> count.commitError(accepted.sequence, accepted.isInitial)
                            }
                        if (accepted.isInitial) baselineApplied.complete(Unit)
                        if (notificationCount != null) {
                            pageCache
                                .prepareRetireWhere({ true }, queryFailure())
                                .invoke()
                            pageRetired.set(true)
                            notifications.trySend(notificationCount).getOrThrow()
                        }
                    },
                )
            observerCache.ensure("parent")
            observerEvents.send(LiveQueryEvent.Value(1))
            withTimeout(1_000) { baselineApplied.await() }
            pageCache.ensure("page")
            pageEvents.send(LiveQueryEvent.Value(2))
            withTimeout(1_000) { pageApplied.await() }
            pauseObserver.set(true)

            observerEvents.send(LiveQueryEvent.Value(4))
            assertTrue(observerPaused.await(1, TimeUnit.SECONDS))
            pageEvents.send(LiveQueryEvent.Value(7))
            assertEquals(7, pageCache.value("page"))
            pauseObserver.set(false)
            resumeObserver.countDown()

            assertEquals(4, withTimeout(1_000) { notifications.receive() })
            assertTrue(pageRetired.get())
            observerEvents.send(LiveQueryEvent.Error(queryFailure()))
            assertEquals(7, withTimeout(1_000) { notifications.receive() })
            observerCache.cancelAll()
            pageCache.cancelAll()
        }

    @Test
    fun acceptedSearchChangeCannotBeSuppressedByLaterCurrentRead() =
        runBlocking {
            val events = Channel<LiveQueryEvent<Int>>(Channel.UNLIMITED)
            val count = AcceptedCount()
            val baselineApplied = CompletableDeferred<Unit>()
            val changePaused = CountDownLatch(1)
            val resumeChange = CountDownLatch(1)
            val pauseChange = AtomicBoolean()
            val notifications = Channel<Int>(Channel.UNLIMITED)
            val cache =
                LiveProjectionCache<String, Int>(
                    scope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
                    maximumCount = 1,
                    flow = { events.receiveAsFlow() },
                    isRetained = { true },
                    onAcceptedRead = { _, accepted -> count.acceptRead(accepted.sequence, accepted.value) },
                    onAcceptedChange = { _, accepted ->
                        if (pauseChange.get()) {
                            changePaused.countDown()
                            assertTrue(resumeChange.await(1, TimeUnit.SECONDS))
                        }
                        val notificationCount =
                            when (val event = accepted.event) {
                                is LiveQueryEvent.Value ->
                                    count.commitValue(accepted.sequence, event.value, accepted.isInitial)

                                is LiveQueryEvent.Error -> count.commitError(accepted.sequence, accepted.isInitial)
                            }
                        if (accepted.isInitial) baselineApplied.complete(Unit)
                        if (notificationCount != null) notifications.trySend(notificationCount).getOrThrow()
                    },
                )
            cache.ensure("query")
            events.send(LiveQueryEvent.Value(1))
            withTimeout(1_000) { baselineApplied.await() }
            pauseChange.set(true)

            events.send(LiveQueryEvent.Value(4))
            assertTrue(changePaused.await(1, TimeUnit.SECONDS))
            val currentRead = async(Dispatchers.IO) { cache.value("query") }
            assertEquals(4, currentRead.await())
            pauseChange.set(false)
            resumeChange.countDown()

            assertEquals(4, withTimeout(1_000) { notifications.receive() })
            events.send(LiveQueryEvent.Value(6))
            assertEquals(6, withTimeout(1_000) { notifications.receive() })
            events.send(LiveQueryEvent.Error(queryFailure()))
            assertEquals(6, withTimeout(1_000) { notifications.receive() })
            cache.cancelAll()
        }

    private fun queryFailure(): BridgeException =
        BridgeException.Diagnostic(
            category = BridgeErrorCategory.DATABASE,
            detail = "query failed",
        )
}
