package fm.bae.fastvalues

import android.app.Activity
import android.os.Looper
import android.view.View
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.awaitCancellation
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowChoreographer
import uniffi.bae_bridge.BridgeFastValue
import uniffi.bae_bridge.BridgeTrackPosition
import uniffi.bae_bridge.KeyedValueStream
import uniffi.bae_bridge.NoHandle
import java.time.Duration

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class FrameFollowingTest {
    /** A stream that holds one position under every key and counts how often a key is read. */
    private class CountedReads : KeyedValueStream(NoHandle) {
        var reads = 0

        override suspend fun next(): List<ByteArray> = awaitCancellation()

        override fun latest(key: ByteArray): BridgeFastValue? {
            reads += 1
            return BridgeFastValue.TrackPosition(BridgeTrackPosition(1, 10uL, 0.1, 0uL))
        }

        override fun cancel() = Unit
    }

    @Test
    fun aFrameReadsTheKeyOnce() {
        // Frames come one per interval, as a display paces them.
        ShadowChoreographer.setPaused(true)
        ShadowChoreographer.setFrameDelay(FRAME)
        val counted = CountedReads()
        val positions = LatestStream.trackPositions()
        positions.fed(CoroutineScope(Dispatchers.Unconfined), onError = {}) { counted }
        val activity = Robolectric.buildActivity(Activity::class.java).setup().get()
        val view = View(activity)
        activity.setContentView(view)
        val following = FrameFollowing<BridgeTrackPosition>(view) {}
        following.follow(positions.source("track"))

        val looper = shadowOf(Looper.getMainLooper())
        looper.idleFor(FRAME)
        val before = counted.reads
        looper.idleFor(FRAME)
        looper.idleFor(FRAME)
        looper.idleFor(FRAME)

        assertEquals(3, counted.reads - before)
    }

    private companion object {
        val FRAME: Duration = Duration.ofMillis(16)
    }
}
