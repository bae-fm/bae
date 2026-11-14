package fm.bae.app.data

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.NoErrorDisplays
import fm.bae.app.playback.FakeAlbumList
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeAlbum
import uniffi.bae_bridge.BridgeAlbumAppearance
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

    private fun open(albums: List<BridgeAlbum>): Fixture {
        val list = FakeAlbumList(albums)
        val handle = FakeAppHandle(albumList = list)
        val store =
            AlbumPageStore(
                Library(handle),
                RuntimeEnvironment.getApplication(),
                NoErrorDisplays,
                CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
            )
        store.activate(sort)
        return Fixture(list, store).also { it.idle() }
    }

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
}
