package fm.bae.app.data

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.NoErrorDisplays
import fm.bae.app.diagnosticDetail
import fm.bae.app.playback.FakeAlbumList
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumAppearance
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeListRows
import uniffi.bae_bridge.BridgeListWindow
import uniffi.bae_bridge.BridgeListWindowValue
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class ChainBrowserPageStoreTest {
    private val sort = BridgeSortCriterion(BridgeSortField.TITLE, BridgeSortDirection.ASCENDING)

    private fun albums(ids: List<String>) = ids.map { BridgeFixtures.album(id = it) }

    private fun ids(range: IntRange) = range.map { "a%04d".format(it) }

    private class Fixture(
        val list: FakeAlbumList,
        val store: AlbumPageStore,
    ) {
        fun idle() = shadowOf(Looper.getMainLooper()).idle()

        fun ids(range: IntRange): List<String?> = range.map { store.rowAt(it)?.album?.id }
    }

    private fun open(
        albums: List<BridgeAlbum>,
        failure: BridgeException? = null,
        errors: ErrorDisplays = NoErrorDisplays,
    ): Fixture {
        val list = FakeAlbumList(albums, windowFailure = failure)
        val handle = FakeAppHandle(albumList = list)
        val store =
            AlbumPageStore(
                Library(handle),
                RuntimeEnvironment.getApplication(),
                errors,
                CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            )
        store.activate(sort)
        return Fixture(list, store).also { it.idle() }
    }

    private val readFailed = BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed")

    @Test
    fun aSplitOnTheKeyBeyondAWindowKeepsEveryRowOnce() {
        val fixture = open(albums(ids(0..199)))
        fixture.store.reportVisibleRange(0, 10)
        fixture.idle()
        assertEquals(ids(0..59), fixture.ids(0..59))

        // Rows land inside the first window's range, so it holds more than its limit.
        val added = listOf("a0010x1", "a0010x2", "a0010x3", "a0010x4", "a0010x5")
        fixture.list.albums = albums((ids(0..199) + added).sorted())
        fixture.list.changed()
        fixture.idle()
        fixture.store.reportVisibleRange(0, 10)
        fixture.idle()

        assertEquals(
            fixture.list.albums
                .take(125)
                .map { it.id },
            fixture.ids(0..124),
        )
    }

    @Test
    fun aWindowThatGoesEmptyIsDroppedIntoItsNeighbour() {
        val fixture = open(albums(ids(0..129)))
        fixture.store.reportVisibleRange(0, 119)
        fixture.idle()
        fixture.store.reportVisibleRange(0, 119)
        fixture.idle()
        assertEquals(ids(0..129), fixture.ids(0..129))

        // Every row of the middle window goes.
        fixture.list.albums = albums(ids(0..59) + ids(120..129))
        fixture.list.changed()
        fixture.idle()

        assertEquals(fixture.list.albums.map { it.id }, fixture.ids(0..69))
        assertNull(fixture.store.rowAt(70))
    }

    @Test
    fun aJumpStartsTheChainAtTheKeyAtThatPosition() {
        val fixture = open(albums(ids(0..999)))

        fixture.store.reportVisibleRange(900, 910)
        fixture.idle()

        assertEquals(ids(900..960), fixture.ids(900..960))
        assertNull(fixture.store.rowAt(0))
        assertEquals(
            BridgeListWindow.Forward(fixture.list.keyOf("a0900"), null, BROWSER_PAGE_SIZE.toULong()),
            fixture.list.windows
                .filter { !it.cancelled }
                .map { it.requested.first() }
                .first { it is BridgeListWindow.Forward },
        )
    }

    @Test
    fun aValueForAReplacedWindowDrawsNothing() {
        val fixture = open(albums(ids(0..199)))
        val window = fixture.list.windows.first()
        // The window opened with a request its first delivery then moved.
        val replaced = window.requested.first()
        assertEquals(2, window.requested.size)

        window.deliver(
            BridgeListWindowValue(
                replaced,
                BridgeListRows.Albums(listOf(BridgeAlbumAppearance(BridgeFixtures.album(id = "stale"), null))),
                fixture.list.keyOf("stale"),
                null,
            ),
        )
        fixture.idle()

        assertEquals(ids(0..59), fixture.ids(0..59))
    }

    @Test
    fun theRowsOfAWindowAreItsLastValueOnly() {
        val fixture = open(albums(ids(0..29)))
        assertEquals(ids(0..29), fixture.ids(0..29))

        fixture.list.albums = albums(ids(0..9))
        fixture.list.changed()
        fixture.idle()

        assertEquals(ids(0..9), fixture.ids(0..9))
        assertNull(fixture.store.rowAt(10))
        assertEquals(10, fixture.store.totalCount)
    }

    @Test
    fun aReplacedListsReadsCannotWriteIntoItsReplacement() {
        val fixture = open(albums(ids(0..29)))
        val old = fixture.list.windows.first()

        fixture.store.activate(BridgeSortCriterion(BridgeSortField.TITLE, BridgeSortDirection.DESCENDING))
        fixture.idle()
        old.deliver(
            BridgeListWindowValue(
                old.requested.last(),
                BridgeListRows.Albums(listOf(BridgeAlbumAppearance(BridgeFixtures.album(id = "stale"), null))),
                fixture.list.keyOf("stale"),
                null,
            ),
        )
        fixture.idle()

        assertTrue(old.cancelled)
        assertEquals(ids(0..29), fixture.ids(0..29))
    }

    @Test
    fun aLaterReadThatFailsNamesWhatTheListHolds() {
        val fixture = open(albums(ids(0..29)))

        fixture.list.windowFailure = readFailed
        fixture.list.changed()
        fixture.idle()

        assertEquals(
            "Could not load more albums.",
            fixture.store.error
                ?.error
                ?.line,
        )
    }

    @Test
    fun aFirstReadThatFailsSaysTheLibraryDidntLoad() {
        val fixture = open(albums(ids(0..29)), failure = readFailed)

        assertEquals(
            "Could not load your library.",
            fixture.store.error
                ?.error
                ?.line,
        )
    }

    /** A read core says reading again can't get past, such as a damaged database, offers no Retry. */
    @Test
    fun aFailedReadNoRetryGetsPastOffersNone() {
        val noRetry =
            object : ErrorDisplays by NoErrorDisplays {
                override fun display(error: BridgeException) = DisplayError("failed", error.diagnosticDetail, false)
            }
        val fixture =
            open(
                albums(ids(0..29)),
                failure = BridgeException.Diagnostic(BridgeErrorCategory.DatabaseDamaged, "damaged"),
                errors = noRetry,
            )

        assertEquals(
            false,
            fixture.store.error
                ?.error
                ?.offersRetry,
        )
    }
}
