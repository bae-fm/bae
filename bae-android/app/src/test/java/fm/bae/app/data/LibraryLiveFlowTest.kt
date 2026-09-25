package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.FakeDetailRead
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LibraryLiveFlowTest {
    @Test
    fun an_items_flow_reads_it_until_collection_stops() =
        runBlocking {
            val detail = BridgeFixtures.albumDetail(BridgeFixtures.album("album-1"))
            val handle = FakeAppHandle(albumDetails = mapOf("album-1" to detail))

            val event = Library(handle).albumDetails("album-1").first()

            assertEquals(detail, (event as LiveQueryEvent.Value).value)
            val read = handle.albumDetailSubscriptions.single().read
            assertEquals(listOf<String?>("album-1"), read.requestedIds)
            assertTrue("the read ends with the collection", read.cancelled)
        }

    @Test
    fun a_detail_pane_moves_its_one_read_between_albums() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    albumDetails =
                        mapOf(
                            "album-1" to BridgeFixtures.albumDetail(BridgeFixtures.album("album-1")),
                            "album-2" to BridgeFixtures.albumDetail(BridgeFixtures.album("album-2")),
                        ),
                )
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val store = LibraryQueryStores(Library(handle), scope).album

            store.activate("album-1")
            assertEquals(
                "album-1",
                store.state.value.value
                    ?.album
                    ?.id,
            )
            store.deactivate("album-1")
            store.activate("album-2")

            assertEquals(
                "album-2",
                store.state.value.value
                    ?.album
                    ?.id,
            )
            val read: FakeDetailRead<*> = handle.albumDetailSubscriptions.single().read
            assertEquals(listOf("album-1", null, "album-2"), read.requestedIds)
            assertTrue("the pane keeps its read open between albums", !read.cancelled)
            scope.cancel()
        }
}
