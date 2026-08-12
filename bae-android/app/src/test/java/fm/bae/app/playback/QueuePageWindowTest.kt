package fm.bae.app.playback

import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgePlaybackContext
import uniffi.bae_bridge.BridgePlaybackSourceKind
import uniffi.bae_bridge.BridgeQueueEntry
import uniffi.bae_bridge.BridgeQueueUpcomingPage
import uniffi.bae_bridge.QueueUpcomingCallback

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class QueuePageWindowTest {
    @Test
    fun visiblePagesStayBoundedAndEvictedPagesCannotDeliver() {
        val source = RecordingQueuePageSource()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val player =
            BaeCorePlayer(
                applicationLooper = Looper.getMainLooper(),
                appHandle = FakeAppHandle(),
                context = RuntimeEnvironment.getApplication(),
                scope = scope,
                queuePageSource = QueuePageSource(source::subscribe),
                isAppForeground = { false },
            )
        player.onQueueValue(
            manual = emptyList(),
            context =
                BridgePlaybackContext(
                    kind = BridgePlaybackSourceKind.LIBRARY,
                    sourceTitle = null,
                    shuffled = false,
                    upcoming = emptyList(),
                    upcomingTotal = 500uL,
                ),
            hasNext = false,
            hasPrevious = false,
            revision = 1uL,
        )

        runBlocking {
            player.loadUpcomingRange(0, 60)
            player.loadUpcomingRange(60, 60)
            player.loadUpcomingRange(120, 60)
            player.loadUpcomingRange(180, 60)
        }

        assertTrue(source.maximumActive <= 3)
        assertTrue(source.cancellations.getValue(0u).cancelled)

        source.callbacks.getValue(0u).onValue(
            BridgeQueueUpcomingPage(revision = 1uL, entries = listOf(entry("evicted"))),
        )
        shadowOf(Looper.getMainLooper()).idle()
        assertNull(
            player.queue.value.context
                ?.itemAt(0),
        )
    }

    private fun entry(id: String) =
        BridgeQueueEntry(
            entryId = id,
            trackId = "track-$id",
            title = "Track Title",
            artistNames = "Artist Name",
            durationClock = null,
            albumTitle = "Album Title",
            coverImage = null,
        )

    private class RecordingQueuePageSource {
        val callbacks = mutableMapOf<UInt, QueueUpcomingCallback>()
        val cancellations = mutableMapOf<UInt, Cancellation>()
        var maximumActive = 0
            private set

        fun subscribe(
            offset: UInt,
            limit: UInt,
            callback: QueueUpcomingCallback,
        ): QueuePageSubscription {
            check(limit > 0u)
            callbacks[offset] = callback
            return Cancellation(::recordActive).also {
                cancellations[offset] = it
                recordActive()
            }
        }

        private fun recordActive() {
            maximumActive = maxOf(maximumActive, cancellations.values.count { !it.cancelled })
        }
    }

    private class Cancellation(
        private val changed: () -> Unit,
    ) : QueuePageSubscription {
        var cancelled = false
            private set

        override fun cancel() {
            cancelled = true
            changed()
        }
    }
}
