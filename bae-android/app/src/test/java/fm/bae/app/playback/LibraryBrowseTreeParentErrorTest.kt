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
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

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

    @Test
    fun retiredPageCallbackCannotReplaceTheCurrentKnownCount() =
        runBlocking {
            val handle = FakeAppHandle(deliverAlbumPagesImmediately = false)
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            val retiredRequest =
                async {
                    runCatching {
                        tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)
                    }.exceptionOrNull()
                }
            yield()

            handle.emitAlbumParentObservation(1uL)
            assertTrue(retiredRequest.await() is BridgeException.Diagnostic)

            val currentRequest = async { tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20) }
            yield()
            handle.emitAlbumPage(
                subscription = 1,
                rows = listOf(BridgeFixtures.album(id = "album-current")),
                totalCount = 5uL,
            )
            assertEquals(BrowseId.Album("album-current").mediaId, currentRequest.await()!!.single().mediaId)
            notifications.clear()

            handle.emitAlbumPage(
                subscription = 0,
                rows = listOf(BridgeFixtures.album(id = "album-retired")),
                totalCount = 99uL,
            )
            handle.failAlbumParentObservation()

            assertEquals(listOf(BrowseId.Albums.mediaId to 5), notifications)
        }

    @Test
    fun searchErrorsNotifyOnlyAfterASuccessfulCurrentValue() =
        runBlocking {
            val handle = FakeAppHandle(deliverSearchResultsImmediately = false)
            val notifications = mutableListOf<Int>()
            val tree = tree(handle)
            val subscription =
                async {
                    runCatching {
                        tree.subscribeSearch(Any(), "query") { count -> notifications += count }
                    }.exceptionOrNull()
                }
            yield()

            handle.failSearchResults(0, queryFailure())
            assertTrue(subscription.await() is BridgeException.Diagnostic)
            handle.failSearchResults(0, queryFailure())
            assertTrue(notifications.isEmpty())

            handle.emitSearchResults(
                subscription = 0,
                value =
                    BridgeFixtures.searchResults(
                        albums =
                            listOf(
                                BridgeFixtures.albumSearchResult(id = "album-1"),
                                BridgeFixtures.albumSearchResult(id = "album-2"),
                                BridgeFixtures.albumSearchResult(id = "album-3"),
                            ),
                    ),
            )
            assertEquals(listOf(3), notifications)

            handle.failSearchResults(0, queryFailure())
            assertEquals(listOf(3, 3), notifications)
        }

    @Test
    fun replacedSearchCountDoesNotLeakToTheCurrentQuery() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = {
                        BridgeFixtures.searchResults(
                            albums =
                                listOf(
                                    BridgeFixtures.albumSearchResult(id = "album-a-1"),
                                    BridgeFixtures.albumSearchResult(id = "album-a-2"),
                                ),
                        )
                    },
                )
            val firstNotifications = mutableListOf<Int>()
            val secondNotifications = mutableListOf<Int>()
            val tree = tree(handle)
            val owner = Any()
            tree.subscribeSearch(owner, "query-a") { count -> firstNotifications += count }
            handle.deliverSearchResultsImmediately = false
            val replacement =
                async {
                    runCatching {
                        tree.subscribeSearch(owner, "query-b") { count -> secondNotifications += count }
                    }.exceptionOrNull()
                }
            yield()

            handle.emitSearchResults(
                subscription = 0,
                value =
                    BridgeFixtures.searchResults(
                        albums = List(5) { index -> BridgeFixtures.albumSearchResult(id = "album-retired-$index") },
                    ),
            )
            handle.failSearchResults(1, queryFailure())
            assertTrue(replacement.await() is BridgeException.Diagnostic)
            handle.failSearchResults(1, queryFailure())

            assertEquals(listOf(2), firstNotifications)
            assertTrue(secondNotifications.isEmpty())
        }

    private fun tree(
        handle: FakeAppHandle,
        onChildrenChanged: (String, Int) -> Unit = { _, _ -> },
    ): LibraryBrowseTree<Any> =
        LibraryBrowseTree(
            library = Library(handle),
            labels = { BrowseLabels(albums = "Albums", composers = "Composers") },
            artworkUri = { Uri.parse("content://test/cover/${it.id}") },
            onChildrenChanged = onChildrenChanged,
        )

    private fun queryFailure(): BridgeException =
        BridgeException.Diagnostic(
            category = BridgeErrorCategory.DATABASE,
            detail = "query failed",
        )
}
