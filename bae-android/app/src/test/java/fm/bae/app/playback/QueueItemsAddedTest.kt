package fm.bae.app.playback

import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * The queue-add confirmation flow ([BaeCorePlayer.onQueueItemsAdded] →
 * [BaeCorePlayer.queueItemsAdded]). It carries a one-shot count for the in-app
 * root to surface as a snackbar. The contract that matters is that it is a
 * transient event, not state: a collector that subscribes after an add must not
 * see it, so a recomposition or screen change never re-shows a stale "+N added".
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class QueueItemsAddedTest {
    @Test
    fun deliversCountToALiveSubscriber() {
        val player = player()
        val received = mutableListOf<Int>()
        val scope = CoroutineScope(Dispatchers.Unconfined)
        val job = scope.launch { player.queueItemsAdded.collect { received.add(it) } }

        player.onQueueItemsAdded(3)

        assertEquals(listOf(3), received)
        job.cancel()
    }

    @Test
    fun doesNotReplayToASubscriberThatArrivesAfterTheAdd() {
        val player = player()

        // Emitted before anyone is listening: with no replay the confirmation is
        // gone, not buffered for the next collector.
        player.onQueueItemsAdded(3)

        val received = mutableListOf<Int>()
        val scope = CoroutineScope(Dispatchers.Unconfined)
        val job = scope.launch { player.queueItemsAdded.collect { received.add(it) } }

        assertEquals(emptyList<Int>(), received)
        job.cancel()
    }

    private fun player(): BaeCorePlayer {
        val context = RuntimeEnvironment.getApplication()
        return BaeCorePlayer(
            applicationLooper = Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            context = context,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )
    }
}
