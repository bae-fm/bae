package fm.bae.app.data

import android.os.Looper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeLibraryPageWindow

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BrowserPageStoresTest {
    @Test
    fun oneQueryReadsAtMostThreeVisibleWindows() {
        val store = RecordingPageStore(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        store.activate("all")
        // The visible range is clamped to the list's length, which the first
        // value reports.
        store.queries.single().emit(window(0), "first")
        shadowOf(Looper.getMainLooper()).idle()
        store.reportVisibleRange(60, 239)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, store.queries.size)
        assertEquals(
            listOf(60uL, 120uL, 180uL),
            store.queries
                .single()
                .windows
                .map { it.offset },
        )
    }

    @Test
    fun aValueReadForADroppedWindowWritesNothingThere() {
        val store = RecordingPageStore(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        store.activate("all")
        store.reportVisibleRange(60, 119)
        val query = store.queries.single()
        query.emit(window(0), "evicted")
        shadowOf(Looper.getMainLooper()).idle()

        assertFalse(store.rows.containsKey(0))

        query.emit(window(60), "kept")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("kept", store.rows[60])
    }

    @Test
    fun aReplacedQueryCannotWriteIntoItsReplacement() {
        val store = RecordingPageStore(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))

        store.activate("first")
        val old = store.queries.single()
        store.activate("second")
        val replacement = store.queries.last()

        old.emit(window(0), "old")
        shadowOf(Looper.getMainLooper()).idle()
        assertFalse(store.rows[0] == "old")
        assertTrue(old.cancelled)

        replacement.emit(window(0), "new")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals("new", store.rows[0])
    }

    private fun window(offset: Int) = BridgeLibraryPageWindow(offset.toULong(), BROWSER_PAGE_SIZE.toULong())

    private class RecordingPageStore(
        scope: CoroutineScope,
    ) : WindowedBrowserPageStore<String, String>(RuntimeEnvironment.getApplication(), scope) {
        val queries = mutableListOf<RecordingQuery>()

        override fun open(parameter: String): BrowseRowsQuery<String> = RecordingQuery().also(queries::add)
    }

    private class RecordingQuery : BrowseRowsQuery<String> {
        private val values = Channel<BrowseRows<String>>(Channel.UNLIMITED)
        var windows = emptyList<BridgeLibraryPageWindow>()
            private set
        var cancelled = false
            private set

        override fun setWindows(windows: List<BridgeLibraryPageWindow>) {
            this.windows = windows
        }

        override suspend fun next(): BrowseRows<String> = values.receive()

        override suspend fun cancel() {
            cancelled = true
        }

        fun emit(
            window: BridgeLibraryPageWindow,
            row: String,
        ) {
            values.trySend(BrowseRows(listOf(window to listOf(row)), totalCount = 500))
        }
    }
}
