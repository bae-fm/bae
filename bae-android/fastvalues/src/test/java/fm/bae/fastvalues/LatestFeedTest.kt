package fm.bae.fastvalues

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
import uniffi.bae_bridge.BridgeFastValue
import uniffi.bae_bridge.KeyedValueStream
import uniffi.bae_bridge.NoHandle

@OptIn(ExperimentalCoroutinesApi::class)
class LatestFeedTest {
    /** The main thread, where a fed stream's key changes are told; the test's clock runs it. */
    private val main = StandardTestDispatcher()

    @Before
    fun setUp() = Dispatchers.setMain(main)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun aPulledKeyChangeTellsThatKeysWatchersAndCancellingTheFeedCancelsTheStream() =
        runTest(main) {
            val stream = LatestStream.downloads()
            val pulls = PulledKeys()
            val told = mutableListOf<String>()
            stream.activity.watch("release-1") { told += "release-1" }
            stream.activity.watch("release-3") { told += "release-3" }
            val feed = stream.fed(feedScope(), { error -> throw AssertionError("no read failed", error) }) { pulls }

            pulls.change("release-1", "release-2")
            runCurrent()

            assertEquals("only the changed key's watcher is told", listOf("release-1"), told)
            feed.cancel()
            runCurrent()
            assertTrue("cancelling the feed cancels the stream", pulls.cancelled)
        }

    @Test
    fun aPulledFailureReachesOnErrorAndThePullsGoOn() =
        runTest(main) {
            val stream = LatestStream.uploadTotal()
            val pulls = PulledKeys()
            val errors = mutableListOf<BridgeException>()
            var told = 0
            stream.activity.watch("") { told++ }
            stream.fed(feedScope(), errors::add) { pulls }
            val failure = BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed")

            pulls.fail(failure)
            pulls.change("")
            runCurrent()

            assertEquals(listOf<BridgeException>(failure), errors)
            assertEquals("the key change after the failure is still told", 1, told)
        }

    private fun feedScope() = CoroutineScope(SupervisorJob() + main)

    /** A stream whose key changes and failures a test hands the pulls; it holds no value. */
    private class PulledKeys : KeyedValueStream(NoHandle) {
        private val answers = Channel<Result<List<ByteArray>>>(Channel.UNLIMITED)

        var cancelled = false
            private set

        fun change(vararg keys: String) {
            answers.trySend(Result.success(keys.map(String::encodeToByteArray)))
        }

        fun fail(error: BridgeException) {
            answers.trySend(Result.failure(error))
        }

        override suspend fun next(): List<ByteArray> = answers.receive().getOrThrow()

        override fun latest(key: ByteArray): BridgeFastValue? = null

        override fun moving(key: ByteArray): Boolean = false

        override fun cancel() {
            cancelled = true
            answers.close(BridgeException.Cancelled())
        }
    }
}
