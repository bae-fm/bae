package fm.bae.app.playback

import android.net.Uri
import fm.bae.app.BridgeFixtures
import fm.bae.app.data.Library
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LibraryBrowseTreeParentErrorTest {
    @Test
    fun pageCountIsUsedWhenParentObservationErrorsAfterItsInitialError() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    albumPages = { _, _ ->
                        listOf(
                            BridgeFixtures.album(id = "album-1"),
                            BridgeFixtures.album(id = "album-2"),
                            BridgeFixtures.album(id = "album-3"),
                        )
                    },
                    deliverAlbumParentObservationImmediately = false,
                )
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)
            val readiness = async { tree.subscribeParent(Any(), BrowseId.Albums.mediaId) }
            yield()

            handle.failAlbumParentObservation()
            assertTrue(readiness.await())
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)
            handle.failAlbumParentObservation()

            assertEquals(listOf(BrowseId.Albums.mediaId to 3), notifications)
            assertTrue(handle.albumPageSubscriptions.single().cancelled)

            handle.emitAlbumParentObservation(4uL)

            assertEquals(
                listOf(BrowseId.Albums.mediaId to 3, BrowseId.Albums.mediaId to 4),
                notifications,
            )
        }

    @Test
    fun parentObservationErrorsWithoutAKnownCountDoNotNotify() =
        runBlocking {
            val handle = FakeAppHandle(deliverAlbumParentObservationImmediately = false)
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            val readiness = async { tree.subscribeParent(Any(), BrowseId.Albums.mediaId) }
            yield()

            handle.failAlbumParentObservation()
            assertTrue(readiness.await())
            handle.failAlbumParentObservation()

            assertTrue(notifications.isEmpty())

            handle.emitAlbumParentObservation(2uL)
            assertEquals(listOf(BrowseId.Albums.mediaId to 2), notifications)
        }

    private fun tree(
        handle: FakeAppHandle,
        onChildrenChanged: (String, Int) -> Unit,
    ): LibraryBrowseTree<Any> =
        LibraryBrowseTree(
            library = Library(handle),
            labels = { BrowseLabels(albums = "Albums", composers = "Composers") },
            artworkUri = { Uri.parse("content://test/cover/${it.id}") },
            onChildrenChanged = onChildrenChanged,
        )
}
