package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.FakeDetailRead
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class LibraryLiveFlowTest {
    /** A detail pane's values reach it on the main thread, which here runs each at once. */
    @Before
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun an_items_flow_reads_it_until_collection_stops() =
        runBlocking {
            val detail = BridgeFixtures.albumDetail(BridgeFixtures.album("album-1"))
            val handle = FakeAppHandle(albumDetails = mapOf("album-1" to detail))

            val event = Library(handle).albumDetails("album-1").first()

            assertEquals(detail, (event as LiveQueryEvent.Value).value)
            val read = handle.albumDetailSubscriptions.single().read
            assertEquals(listOf("album-1"), read.requestedIds)
            assertTrue("the read ends with the collection", read.cancelled)
        }

    @Test
    fun a_detail_pane_moves_its_one_read_while_it_shows_and_closes_it_when_it_goes() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    albumDetails =
                        listOf("album-1", "album-2", "album-3").associateWith {
                            BridgeFixtures.albumDetail(BridgeFixtures.album(it))
                        },
                )
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val store = LibraryQueryStores(Library(handle), scope).album

            store.activate("album-1")
            store.activate("album-2")
            assertEquals(
                "album-2",
                store.state.value.value
                    ?.album
                    ?.id,
            )
            val first: FakeDetailRead<*> = handle.albumDetailSubscriptions.single().read
            assertEquals(listOf("album-1", "album-2"), first.requestedIds)

            store.deactivate("album-1")
            assertTrue("an album no longer shown leaves the read alone", !first.cancelled)
            store.deactivate("album-2")
            assertTrue("the pane's read ends when the pane goes", first.cancelled)

            store.activate("album-3")
            assertEquals(2, handle.albumDetailSubscriptions.size)
            val second: FakeDetailRead<*> = handle.albumDetailSubscriptions.last().read
            assertEquals(listOf("album-3"), second.requestedIds)
            assertEquals(
                "album-3",
                store.state.value.value
                    ?.album
                    ?.id,
            )
            scope.cancel()
        }
}
