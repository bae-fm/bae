package fm.bae.app.data

import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BrowserPageStoresTest {
    @Test
    fun visibleWindowBoundsSubscriptionsAndIgnoresEvictedDelivery() {
        val store =
            RecordingPageStore(
                CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            )

        store.activate("all")
        store.reportVisibleRange(60, 119)
        store.reportVisibleRange(120, 179)
        store.reportVisibleRange(180, 239)
        store.reportVisibleRange(240, 299)
        shadowOf(Looper.getMainLooper()).idle()

        assertTrue(store.maximumActive <= 3)
        assertTrue(store.cancellations.getValue(0).cancelled)

        store.emit(offset = 0, row = "evicted")
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse(store.rows.containsKey(0))
    }

    private class RecordingPageStore(
        scope: CoroutineScope,
    ) : WindowedBrowserPageStore<String, String>(RuntimeEnvironment.getApplication(), scope) {
        val cancellations = mutableMapOf<Int, Cancellation>()
        private val generations = mutableMapOf<Int, Int>()
        var maximumActive = 0
            private set

        override fun subscribe(
            parameter: String,
            offset: Int,
            generation: Int,
        ): PageSubscription {
            val cancellation = Cancellation(::recordActive)
            cancellations[offset] = cancellation
            generations[offset] = generation
            recordActive()
            deliver(offset, generation, listOf("row-$offset"), total = 500)
            return cancellation
        }

        fun emit(
            offset: Int,
            row: String,
        ) {
            deliver(offset, generations.getValue(offset), listOf(row), total = 500)
        }

        private fun recordActive() {
            maximumActive = maxOf(maximumActive, cancellations.values.count { !it.cancelled })
        }
    }

    private class Cancellation(
        private val changed: () -> Unit,
    ) : PageSubscription {
        var cancelled = false
            private set

        override fun cancel() {
            cancelled = true
            changed()
        }
    }
}
