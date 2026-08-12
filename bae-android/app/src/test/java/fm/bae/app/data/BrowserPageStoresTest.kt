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

    @Test
    fun oldSameOffsetSubscriptionCannotMutateReplacement() {
        val store =
            RecordingPageStore(
                CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            )

        store.activate("all")
        for (offset in listOf(60, 120, 180, 240, 0)) {
            store.reportVisibleRange(offset, offset + 59)
        }
        shadowOf(Looper.getMainLooper()).idle()

        store.emit(offset = 0, subscription = 0, row = "old")
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse(store.rows[0] == "old")

        store.emit(offset = 0, subscription = 1, row = "new")
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(store.rows[0] == "new")

        store.fail(offset = 0, subscription = 0)
        shadowOf(Looper.getMainLooper()).idle()
        assertTrue(store.error == null)
    }

    private class RecordingPageStore(
        scope: CoroutineScope,
    ) : WindowedBrowserPageStore<String, String>(RuntimeEnvironment.getApplication(), scope) {
        val cancellations = mutableMapOf<Int, Cancellation>()
        private val emitters = mutableMapOf<Int, MutableList<(String) -> Unit>>()
        private val failures = mutableMapOf<Int, MutableList<() -> Unit>>()
        var maximumActive = 0
            private set

        override fun subscribe(
            parameter: String,
            offset: Int,
            generation: Int,
            identity: Long,
        ): PageSubscription {
            val cancellation = Cancellation(::recordActive)
            cancellations[offset] = cancellation
            emitters.getOrPut(offset, ::mutableListOf).add { row ->
                deliver(offset, generation, identity, listOf(row), total = 500)
            }
            failures.getOrPut(offset, ::mutableListOf).add {
                fail(
                    offset,
                    generation,
                    identity,
                    uniffi.bae_bridge.BridgeException.Diagnostic(
                        uniffi.bae_bridge.BridgeErrorCategory.INTERNAL,
                        "old failure",
                    ),
                )
            }
            recordActive()
            deliver(
                offset,
                generation,
                identity,
                listOf("row-$offset"),
                total = 500,
            )
            return cancellation
        }

        fun emit(
            offset: Int,
            subscription: Int = 0,
            row: String,
        ) {
            emitters.getValue(offset)[subscription](row)
        }

        fun fail(
            offset: Int,
            subscription: Int,
        ) {
            failures.getValue(offset)[subscription]()
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
