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
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
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
import uniffi.bae_bridge.BridgeListView
import uniffi.bae_bridge.BridgeSortCriterion
import uniffi.bae_bridge.BridgeSortDirection
import uniffi.bae_bridge.BridgeSortField

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class ListBrowserPageStoreTest {
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
        deliverImmediately: Boolean = true,
    ): Fixture {
        val list = FakeAlbumList(albums, deliverImmediately = deliverImmediately, windowFailure = failure)
        val handle = FakeAppHandle(albumList = list)
        val store =
            AlbumPageStore(
                Library(handle),
                RuntimeEnvironment.getApplication(),
                errors,
                CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            )
        store.screenRows = 30
        store.activate(sort)
        return Fixture(list, store).also { it.idle() }
    }

    private val readFailed = BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed")

    /** The failure the page draws in its rows' place, while it draws one. */
    private val AlbumPageStore.failure: PageError? get() = (state as? ListPageState.Failed)?.error

    /** The page waits for its count, then draws from the count alone, with the rows the view has not read yet unread. */
    @Test
    fun thePageWaitsForItsCountThenDrawsFromIt() {
        val fixture = open(albums(ids(0..29)), deliverImmediately = false)
        assertEquals(ListPageState.Waiting, fixture.store.state)

        fixture.list.emitCount()
        fixture.idle()

        assertEquals(ListPageState.Counted(30), fixture.store.state)
        assertNull(fixture.store.rowAt(0))
    }

    /** A list whose count answers none is empty, whatever its view holds. */
    @Test
    fun aListWhoseCountIsNoneIsEmpty() {
        val fixture = open(emptyList())

        assertEquals(ListPageState.Counted(0), fixture.store.state)
    }

    /** The store opens the list's view with as many rows on screen as the screen fits, before any row arrives. */
    @Test
    fun theViewOpensWithTheRowsTheScreenFits() {
        val fixture = open(albums(ids(0..199)))

        assertEquals(
            listOf(0..29),
            fixture.list.views
                .single()
                .reported,
        )
        assertEquals(ids(0..89), fixture.ids(0..89))
        assertNull(fixture.store.rowAt(90))
    }

    /** The store tells core the rows on screen once for each change, and draws each row at the window's start plus its place. */
    @Test
    fun theStoreReportsEachChangeOfTheRowsOnScreenOnce() {
        val fixture = open(albums(ids(0..999)))

        fixture.store.reportVisibleRange(900, 909)
        fixture.store.reportVisibleRange(900, 909)
        fixture.idle()

        assertEquals(
            listOf(0..29, 900..909),
            fixture.list.views
                .single()
                .reported,
        )
        assertEquals(ids(890..919), fixture.ids(890..919))
        assertNull(fixture.store.rowAt(0))
    }

    /** Rows inserted above the rows on screen move the window's start: each row the screen shows sits one place lower. */
    @Test
    fun anInsertAboveTheRowsOnScreenKeepsTheirRowsOnePlaceLower() {
        val fixture = open(albums(ids(0..199)))
        fixture.store.reportVisibleRange(100, 109)
        fixture.idle()
        assertEquals(ids(100..109), fixture.ids(100..109))

        fixture.list.albums = albums((ids(0..199) + "a0000a").sorted())
        fixture.list.changed()
        fixture.idle()

        assertEquals(ids(100..109), fixture.ids(101..110))
    }

    @Test
    fun theRowsOfTheViewAreItsLastValueOnly() {
        val fixture = open(albums(ids(0..29)))
        assertEquals(ids(0..29), fixture.ids(0..29))

        fixture.list.albums = albums(ids(0..9))
        fixture.list.changed()
        fixture.idle()

        assertEquals(ids(0..9), fixture.ids(0..9))
        assertNull(fixture.store.rowAt(10))
        assertEquals(ListPageState.Counted(10), fixture.store.state)
    }

    /** A new sort keeps the rows drawn until the new sort's reads answer, then draws theirs and ends the old reads. */
    @Test
    fun aNewSortKeepsTheRowsDrawnUntilItsReadsAnswer() {
        val fixture = open(albums(ids(0..29)), deliverImmediately = false)
        fixture.list.changed()
        fixture.idle()
        val old = fixture.list.views.single()

        fixture.store.activate(BridgeSortCriterion(BridgeSortField.TITLE, BridgeSortDirection.DESCENDING))
        fixture.idle()
        assertEquals(ListPageState.Counted(30), fixture.store.state)
        assertEquals(ids(0..29), fixture.ids(0..29))
        assertFalse(old.cancelled)

        fixture.list.albums = albums(ids(0..9))
        fixture.list.views
            .last()
            .deliver()
        fixture.list.counts
            .last()
            .deliver()
        fixture.idle()

        assertEquals(ListPageState.Counted(10), fixture.store.state)
        assertEquals(ids(0..9), fixture.ids(0..9))
        assertTrue(old.cancelled)
    }

    @Test
    fun aReplacedListsReadsCannotWriteIntoItsReplacement() {
        val fixture = open(albums(ids(0..29)))
        val old = fixture.list.views.first()

        fixture.store.activate(BridgeSortCriterion(BridgeSortField.TITLE, BridgeSortDirection.DESCENDING))
        fixture.idle()
        old.deliver(
            BridgeListView(
                0uL,
                BridgeListRows.Albums(listOf(BridgeAlbumAppearance(BridgeFixtures.album(id = "stale"), null))),
            ),
        )
        fixture.idle()

        assertTrue(old.cancelled)
        assertEquals(ids(0..29), fixture.ids(0..29))
    }

    /** A read that fails after rows arrived replaces them with the failure, and the read's next value replaces it. */
    @Test
    fun aLaterFailedReadReplacesTheRowsUntilTheReadAnswersAgain() {
        val fixture = open(albums(ids(0..29)))

        fixture.list.windowFailure = readFailed
        fixture.list.changed()
        fixture.idle()

        assertEquals(
            "Could not load your library.",
            fixture.store.failure
                ?.error
                ?.line,
        )
        assertEquals(List(30) { null }, fixture.ids(0..29))

        fixture.list.windowFailure = null
        fixture.list.changed()
        fixture.idle()

        assertNull(fixture.store.failure)
        assertEquals(ids(0..29), fixture.ids(0..29))
    }

    /** The view answering leaves a failed count's failure in place: the failure goes once the count answers. */
    @Test
    fun aFailedCountStandsUntilTheCountAnswers() {
        val fixture = open(albums(ids(0..29)))

        fixture.list.countFailure = readFailed
        fixture.list.changed()
        fixture.idle()
        assertNotNull(fixture.store.failure)

        fixture.list.countFailure = null
        fixture.list.emitCount()
        fixture.idle()
        assertNull(fixture.store.failure)
    }

    @Test
    fun aFirstReadThatFailsSaysTheLibraryDidntLoad() {
        val fixture = open(albums(ids(0..29)), failure = readFailed)

        assertEquals(
            "Could not load your library.",
            fixture.store.failure
                ?.error
                ?.line,
        )
    }

    /** When core says that reading again cannot fix the failure, such as a damaged database, the store offers no Retry. */
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
            fixture.store.failure
                ?.error
                ?.offersRetry,
        )
    }
}
