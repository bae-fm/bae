package fm.bae.app.playback

import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.plainCreditText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeQueueSnapshot

/**
 * When a read fails, the screen shows the failure instead of the last value
 * until that read's next value arrives. A queue failure can come from the
 * queue's value or from the session's up-next list.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class PlaybackReadFailureTest {
    private val failure = DisplayError("Couldn't read playback.")

    @Test
    fun aFailedValuesReadHoldsUntilTheNextValue() {
        val player = player()
        player.applyPlaybackState(playingState())

        player.readFailures.failValues(failure)
        assertEquals(failure, player.readFailures.values.value)

        player.applyPlaybackState(playingState())
        assertNull(player.readFailures.values.value)
    }

    @Test
    fun aFailedQueueReadHoldsUntilTheNextQueueValue() {
        val player = player()

        player.readFailures.failQueue(failure)
        assertEquals(failure, player.readFailures.queue.value)

        player.onQueueValue(emptyQueue())
        assertNull(player.readFailures.queue.value)
    }

    @Test
    fun aFailedSessionUpNextReadShowsOnTheQueueUntilItsOwnNextValue() {
        val player = player()

        player.readFailures.failSessionUpNext(failure)
        assertEquals(failure, player.readFailures.queue.value)

        player.onQueueValue(emptyQueue())
        assertEquals("a new queue value does not clear the session up-next read's failure", failure, player.readFailures.queue.value)

        player.onSessionUpNext(emptyList())
        assertNull(player.readFailures.queue.value)
    }

    private fun emptyQueue() =
        BridgeQueueSnapshot(
            manual = emptyList(),
            context = null,
            shuffled = false,
            hasNext = false,
            hasPrevious = false,
        )

    private fun player() =
        BaeCorePlayer(
            applicationLooper = android.os.Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            system = PlaybackSystem(RuntimeEnvironment.getApplication(), AudioFocus(RuntimeEnvironment.getApplication()), { false }),
            creditLineText = ::plainCreditText,
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        )
}
