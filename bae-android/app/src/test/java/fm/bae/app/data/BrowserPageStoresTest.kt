package fm.bae.app.data

import android.os.Looper
import androidx.compose.runtime.snapshots.Snapshot
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.NoErrorDisplays
import fm.bae.app.R
import fm.bae.app.diagnosticDetail
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.Channel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryBrowseSection
import uniffi.bae_bridge.BridgeLibraryPageWindow

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class BrowserPageStoresTest {
    @Test
    fun artistSectionsArriveWithPagesAndLeaveWithTheQuery() {
        val store = RecordingPageStore(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val sections =
            listOf(
                BridgeLibraryBrowseSection("artist-a", "Artist A", null, BridgeLibraryPageWindow(0uL, 200uL)),
                BridgeLibraryBrowseSection("artist-b", "Artist B", null, BridgeLibraryPageWindow(200uL, 300uL)),
            )
        store.activate("grouped")
        store.queries.single().emit(window(0), "album", sections)
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(sections, store.beside)
        assertEquals("album", store.rows[0])
        // A delivery without sections leaves the last ones standing, beside its rows.
        store.queries.single().emit(window(60), "next")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(sections, store.beside)
        store.deactivate()
        assertTrue(store.beside.isEmpty())
    }

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
    fun aValueEqualToTheRowsHeldWritesNoRow() {
        val store = RecordingPageStore(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        val rowWrites = mutableListOf<Set<Any>>()
        val observer = Snapshot.registerApplyObserver { changed, _ -> if (store.rows in changed) rowWrites.add(changed) }
        try {
            store.activate("all")
            val query = store.queries.single()
            query.emit(window(0), "same")
            shadowOf(Looper.getMainLooper()).idle()
            Snapshot.sendApplyNotifications()
            rowWrites.clear()

            query.emit(window(0), "same")
            shadowOf(Looper.getMainLooper()).idle()
            Snapshot.sendApplyNotifications()
            assertTrue(rowWrites.isEmpty())

            query.emit(window(0), "changed")
            shadowOf(Looper.getMainLooper()).idle()
            Snapshot.sendApplyNotifications()
            assertEquals("changed", store.rows[0])
            assertFalse(rowWrites.isEmpty())
        } finally {
            observer.dispose()
        }
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

    @Test
    fun aLaterPageThatFailsNamesWhatTheListHolds() {
        val store =
            RecordingPageStore(
                CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
                loadMoreFailed = R.string.library_load_more_artists_failed,
            )
        store.activate("all")
        val query = store.queries.single()
        query.emit(window(0), "first")
        shadowOf(Looper.getMainLooper()).idle()

        query.fail(BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed"))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals("Couldn’t load more artists.", store.error?.error?.line)
    }

    @Test
    fun aFirstPageThatFailsSaysTheLibraryDidntLoad() {
        val store = RecordingPageStore(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
        store.activate("all")

        store.queries.single().fail(BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed"))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals("Couldn’t load your library.", store.error?.error?.line)
    }

    /** A read core says reading again can't get past, such as a damaged database, offers no Retry. */
    @Test
    fun aFailedReadNoRetryGetsPastOffersNone() {
        val noRetry =
            object : ErrorDisplays by NoErrorDisplays {
                override fun display(error: BridgeException) = DisplayError("failed", error.diagnosticDetail, false)
            }
        val store = RecordingPageStore(CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate), errors = noRetry)
        store.activate("all")

        store.queries.single().fail(BridgeException.Diagnostic(BridgeErrorCategory.DatabaseDamaged, "damaged"))
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(false, store.error?.error?.offersRetry)
    }

    private fun window(offset: Int) = BridgeLibraryPageWindow(offset.toULong(), BROWSER_PAGE_SIZE.toULong())

    private class RecordingPageStore(
        scope: CoroutineScope,
        loadMoreFailed: Int = R.string.library_load_more_albums_failed,
        errors: ErrorDisplays = NoErrorDisplays,
    ) : WindowedBrowserPageStore<String, String, AlbumGridSections>(
            RuntimeEnvironment.getApplication(),
            errors,
            scope,
            emptyList(),
            loadMoreFailed,
        ) {
        val queries = mutableListOf<RecordingQuery>()

        override fun open(parameter: String): BrowseRowsQuery<String, AlbumGridSections> = RecordingQuery().also(queries::add)
    }

    private class RecordingQuery : BrowseRowsQuery<String, AlbumGridSections> {
        private val values = Channel<Result<BrowseRows<String, AlbumGridSections>>>(Channel.UNLIMITED)
        var windows = emptyList<BridgeLibraryPageWindow>()
            private set
        var cancelled = false
            private set

        override fun setWindows(windows: List<BridgeLibraryPageWindow>) {
            this.windows = windows
        }

        override suspend fun next(): BrowseRows<String, AlbumGridSections> = values.receive().getOrThrow()

        override suspend fun cancel() {
            cancelled = true
        }

        fun emit(
            window: BridgeLibraryPageWindow,
            row: String,
            sections: AlbumGridSections? = null,
        ) {
            values.trySend(Result.success(BrowseRows(listOf(window to listOf(row)), totalCount = 500, beside = sections)))
        }

        fun fail(error: BridgeException) {
            values.trySend(Result.failure(error))
        }
    }
}
