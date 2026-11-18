package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetailRequest
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

@OptIn(ExperimentalCoroutinesApi::class)
class LibraryLiveFlowTest {
    /** A detail pane's values reach it on the main thread, which here runs each at once. */
    @Before
    fun setUp() = Dispatchers.setMain(Dispatchers.Unconfined)

    @After
    fun tearDown() = Dispatchers.resetMain()

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

            store.activate(request("album-1"))
            store.activate(request("album-2"))
            assertEquals(
                "album-2",
                store.state.value.answer
                    ?.album
                    ?.id,
            )
            val first = handle.albumDetailSubscriptions.single().read
            assertEquals(listOf(request("album-1"), request("album-2")), first.requestedIds)

            store.deactivate(request("album-1"))
            assertTrue("an album no longer shown leaves the read alone", !first.cancelled)
            store.deactivate(request("album-2"))
            assertTrue("the pane's read ends when the pane goes", first.cancelled)

            store.activate(request("album-3"))
            assertEquals(2, handle.albumDetailSubscriptions.size)
            val second = handle.albumDetailSubscriptions.last().read
            assertEquals(listOf(request("album-3")), second.requestedIds)
            assertEquals(
                "album-3",
                store.state.value.answer
                    ?.album
                    ?.id,
            )
            scope.cancel()
        }

    /** Choosing another release of the album shown keeps the album drawn until the new release's value arrives. */
    @Test
    fun another_release_of_the_album_shown_keeps_its_value_while_the_read_moves() =
        runBlocking {
            val handle =
                FakeAppHandle(albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(BridgeFixtures.album("album-1"))))
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val store = LibraryQueryStores(Library(handle), scope).album
            store.activate(request("album-1"))
            val shown = mutableListOf<String?>()
            val watching = scope.launch { store.state.collect { shown += it.answer?.album?.id } }

            store.activate(request("album-1", chosenReleaseId = "rel-2"))
            watching.cancel()
            assertEquals("the page never empties between releases", listOf("album-1"), shown.distinct())
            store.deactivate(request("album-1"))
            assertTrue(
                "the album's read ends whichever release it shows",
                handle.albumDetailSubscriptions
                    .single()
                    .read.cancelled,
            )
            scope.cancel()
        }

    /** A failed read replaces the album the pane shows, and the read's next value replaces the failure. */
    @Test
    fun a_failed_read_replaces_the_album_until_the_read_answers_again() =
        runBlocking {
            val handle =
                FakeAppHandle(albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(BridgeFixtures.album("album-1"))))
            val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
            val store = LibraryQueryStores(Library(handle), scope).album
            store.activate(request("album-1"))
            val read = handle.albumDetailSubscriptions.single().read
            val failure = BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed")

            read.fail(failure)
            assertEquals(LiveQueryState.Failed(failure), store.state.value)

            read.setId(request("album-1"))
            assertEquals(
                "album-1",
                store.state.value.answer
                    ?.album
                    ?.id,
            )
            scope.cancel()
        }

    private fun request(
        albumId: String,
        chosenReleaseId: String? = null,
    ) = BridgeAlbumDetailRequest(albumId = albumId, chosenReleaseId = chosenReleaseId)
}
