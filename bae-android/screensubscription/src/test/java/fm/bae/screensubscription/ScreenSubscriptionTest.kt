package fm.bae.screensubscription

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

@OptIn(ExperimentalCoroutinesApi::class)
class ScreenSubscriptionTest {
    private val main = StandardTestDispatcher()

    @Before
    fun setUp() = Dispatchers.setMain(main)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun aMappedSubscriptionReadsEachValueThroughItsTransformAndCancelsTheSource() =
        runTest(main) {
            val source = Pulls<Int>()
            val mapped = source.map { "value $it" }

            source.deliver(1)
            assertEquals("value 1", mapped.next())
            mapped.cancel()

            assertTrue("cancelling the mapped pulls cancels the source", source.cancelled)
        }

    @Test
    fun aFirstValueReadPullsOnceAndThenCancels() =
        runTest(main) {
            val source = Pulls<Int>()
            source.deliver(1)
            source.deliver(2)

            assertEquals(1, source.firstValue())
            assertEquals("one pull", 1, source.pullCount)
            assertTrue("the subscription ends after its first value", source.cancelled)
        }

    @Test
    fun aFirstValueReadThatFailsCancelsToo() =
        runTest(main) {
            val source = Pulls<Int>()
            source.fail(readFailure())

            val failed = runCatching { source.firstValue() }

            assertTrue(failed.exceptionOrNull() is BridgeException.Diagnostic)
            assertTrue("a failed first read still ends the subscription", source.cancelled)
        }

    @Test
    fun aScreenSubscriptionKeepsPullingAfterAFailedRead() =
        runTest(main) {
            val source = Pulls<String>()
            val values = mutableListOf<String>()
            val errors = mutableListOf<BridgeException>()
            val failure = readFailure()
            ScreenSubscription(source, screenScope(), values::add, errors::add)

            source.deliver("first")
            source.fail(failure)
            source.deliver("second")
            runCurrent()

            assertEquals(listOf("first", "second"), values)
            assertEquals(listOf<BridgeException>(failure), errors)
        }

    @Test
    fun aCancelledPullEndsTheSubscriptionWithoutAnError() =
        runTest(main) {
            val source = Pulls<String>()
            val errors = mutableListOf<BridgeException>()
            ScreenSubscription(source, screenScope(), {}, errors::add)
            runCurrent()

            source.cancel()
            runCurrent()

            assertTrue("a cancelled pull is not an error", errors.isEmpty())
            assertEquals("no pull after the cancelled one", 1, source.pullCount)
        }

    @Test
    fun aPullAnsweringTheRuntimeIsShuttingDownEndsTheSubscriptionWithoutAnError() =
        runTest(main) {
            val source = Pulls<String>()
            val values = mutableListOf<String>()
            val errors = mutableListOf<BridgeException>()
            ScreenSubscription(source, screenScope(), values::add, errors::add)

            source.deliver("first")
            source.fail(BridgeException.ShuttingDown())
            source.deliver("second")
            runCurrent()

            assertEquals(listOf("first"), values)
            assertTrue("a shutting-down pull is not an error", errors.isEmpty())
            assertEquals("no pull after the shutting-down one", 2, source.pullCount)
        }

    @Test
    fun stopCancelsTheSourceAndNoValueFollows() =
        runTest(main) {
            val source = Pulls<String>()
            val values = mutableListOf<String>()
            val subscription = ScreenSubscription(source, screenScope(), values::add) {}
            runCurrent()

            subscription.stop()
            runCurrent()
            source.deliver("after stop")
            runCurrent()

            assertTrue(source.cancelled)
            assertTrue("no value reaches the screen after stop", values.isEmpty())
        }

    private fun screenScope() = CoroutineScope(SupervisorJob() + main)

    private fun readFailure() = BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed")

    /** Pulls a test answers: each value or failure answers the next pull, and cancelling ends them. */
    private class Pulls<V> : PulledSubscription<V> {
        private val answers = Channel<Result<V>>(Channel.UNLIMITED)

        var cancelled = false
            private set
        var pullCount = 0
            private set

        fun deliver(value: V) {
            answers.trySend(Result.success(value))
        }

        fun fail(error: BridgeException) {
            answers.trySend(Result.failure(error))
        }

        override suspend fun next(): V {
            pullCount++
            return answers.receive().getOrThrow()
        }

        override suspend fun cancel() {
            cancelled = true
            answers.close(BridgeException.Cancelled())
        }
    }
}
