package fm.bae.app.playback

import fm.bae.app.DisplayError
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

/**
 * A failed read of playback values or of the queue stands in for what they
 * last showed until the next value arrives, so the now-playing surfaces and
 * the queue never show the last value as current.
 */
@RunWith(RobolectricTestRunner::class)
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

        player.onQueueValue(manual = emptyList(), context = null, hasNext = false, hasPrevious = false, revision = 1uL)
        assertNull(player.readFailures.queue.value)
    }

    private fun player() =
        BaeCorePlayer(
            applicationLooper = android.os.Looper.getMainLooper(),
            appHandle = FakeAppHandle(),
            context = RuntimeEnvironment.getApplication(),
            scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            isAppForeground = { false },
        )
}
