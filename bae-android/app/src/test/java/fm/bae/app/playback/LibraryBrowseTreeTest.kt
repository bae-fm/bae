package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import fm.bae.app.BridgeFixtures
import fm.bae.app.data.Library
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLiveQueryCause
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LibraryBrowseTreeTest {
    private val labels = BrowseLabels(albums = "Albums", composers = "Composers")

    private fun tree(
        handle: FakeAppHandle,
        onChildrenChanged: (String, Int) -> Unit = { _, _ -> },
    ): LibraryBrowseTree<Any> =
        LibraryBrowseTree(
            library = Library(handle),
            labels = { labels },
            artworkUri = { Uri.parse("content://test/cover/${it.id}") },
            onChildrenChanged = onChildrenChanged,
        )

    private val MediaItem.title: String?
        get() = mediaMetadata.title?.toString()

    @Test
    fun rootAndCategoriesAreBrowsable() =
        runBlocking {
            val tree = tree(FakeAppHandle())
            assertTrue(tree.root.mediaMetadata.isBrowsable == true)
            val children = tree.children(BrowseId.Root.mediaId, 0, 10)!!
            assertEquals(listOf(BrowseId.Albums.mediaId, BrowseId.Composers.mediaId), children.map { it.mediaId })
            assertEquals(listOf("Albums", "Composers"), children.map { it.title })
            tree.close()
        }

    @Test
    fun albumWindowIsAnAbsoluteRequestOnOneSubscription() =
        runBlocking {
            val handle = FakeAppHandle(albumPages = { _, _ -> listOf(BridgeFixtures.album(id = "album-page")) })
            val children = tree(handle).children(BrowseId.Albums.mediaId, 2, 20)!!

            assertEquals(BrowseId.Album("album-page").mediaId, children.single().mediaId)
            assertEquals(listOf(40uL to 20uL), handle.albumPageWindows)
            assertEquals(1, handle.albumBrowseSubscriptions.size)
        }

    @Test
    fun requestChangesDoNotNotifyButDatabaseSnapshotsDo() =
        runBlocking {
            val handle = FakeAppHandle(albumPages = { _, _ -> listOf(BridgeFixtures.album(id = "album-old")) })
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)

            tree.children(BrowseId.Albums.mediaId, 0, 20)
            assertTrue(notifications.isEmpty())

            handle.albumBrowseSubscriptions.single().emitRows(
                listOf(BridgeFixtures.album(id = "album-new")),
                totalCount = 4uL,
            )
            assertEquals(listOf(BrowseId.Albums.mediaId to 4), notifications)
            assertEquals(
                BrowseId.Album("album-new").mediaId,
                tree.children(BrowseId.Albums.mediaId, 0, 20)!!.single().mediaId,
            )
        }

    @Test
    fun aRequestCoalescedWithADatabaseCommitNotifiesExactlyOnce() =
        runBlocking {
            val handle = FakeAppHandle(albumPages = { _, _ -> listOf(BridgeFixtures.album(id = "album-row")) })
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)

            tree.children(BrowseId.Albums.mediaId, 0, 20)
            assertTrue(notifications.isEmpty())

            handle.albumBrowseSubscriptions.single().emitRows(
                rows = listOf(BridgeFixtures.album(id = "album-changed")),
                totalCount = 3uL,
                cause = BridgeLiveQueryCause.REQUEST_AND_DATABASE_CHANGED,
            )

            assertEquals(listOf(BrowseId.Albums.mediaId to 3), notifications)
            tree.close()
        }

    @Test
    fun parentInterestWithNoRequestedWindowNotifiesOnDatabaseSnapshot() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }

            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            assertTrue(
                handle.albumBrowseSubscriptions
                    .single()
                    .requestedWindows
                    .isEmpty(),
            )
            assertTrue(notifications.isEmpty())

            handle.albumBrowseSubscriptions.single().emitCount(7uL)

            assertEquals(listOf(BrowseId.Albums.mediaId to 7), notifications)
            tree.close()
        }

    @Test
    fun requestedWindowsStayBoundedInsideOneLiveQuery() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)

            repeat(13) { tree.children(BrowseId.Albums.mediaId, it, 20) }

            assertEquals(1, handle.albumBrowseSubscriptions.size)
            assertEquals(
                12,
                handle.albumBrowseSubscriptions
                    .single()
                    .requestedWindows.size,
            )
            assertEquals(
                20uL,
                handle.albumBrowseSubscriptions
                    .single()
                    .requestedWindows
                    .first()
                    .offset,
            )
        }

    @Test
    fun aWindowRequestWaitsForTheSnapshotContainingThatWindow() =
        runBlocking {
            val handle = FakeAppHandle(deliverAlbumPagesImmediately = false)
            val tree = tree(handle)
            val request = async { tree.children(BrowseId.Albums.mediaId, 1, 20) }
            yield()
            assertFalse(request.isCompleted)

            handle.albumBrowseSubscriptions.single().emitRows(listOf(BridgeFixtures.album(id = "album-fresh")))

            assertEquals(BrowseId.Album("album-fresh").mediaId, request.await()!!.single().mediaId)
        }

    @Test
    fun nonterminalErrorCompletesTheRequestAndLaterValueRecovers() =
        runBlocking {
            val handle = FakeAppHandle(initialAlbumPageError = queryFailure())
            val tree = tree(handle)

            val error = runCatching { tree.children(BrowseId.Albums.mediaId, 0, 20) }.exceptionOrNull()
            assertTrue(error is BridgeException)

            handle.albumBrowseSubscriptions.single().emitRows(listOf(BridgeFixtures.album(id = "album-recovered")))
            assertEquals(
                BrowseId.Album("album-recovered").mediaId,
                tree.children(BrowseId.Albums.mediaId, 0, 20)!!.single().mediaId,
            )
        }

    @Test
    fun closeCancelsTheCollectionAndCompletesAWaiter() =
        runBlocking {
            val handle = FakeAppHandle(deliverAlbumPagesImmediately = false)
            val tree = tree(handle)
            val request = async { runCatching { tree.children(BrowseId.Albums.mediaId, 0, 20) }.exceptionOrNull() }
            yield()

            tree.close()

            assertTrue(handle.albumBrowseSubscriptions.single().cancelled)
            assertTrue(withTimeout(1_000) { request.await() } is BridgeException)
        }

    @Test
    fun closeWaitsForCommittedNotificationAndPreventsLaterCallbacks() =
        runBlocking {
            val entered = CountDownLatch(1)
            val release = CountDownLatch(1)
            var notifications = 0
            val handle = FakeAppHandle()
            val tree =
                tree(handle) { _, _ ->
                    notifications++
                    entered.countDown()
                    release.await()
                }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, 0, 20)
            val subscription = handle.albumBrowseSubscriptions.single()
            val delivery = async(Dispatchers.IO) { subscription.emitCount(1uL) }
            assertTrue(entered.await(1, TimeUnit.SECONDS))

            val close = async(Dispatchers.IO) { tree.close() }
            yield()
            assertFalse(close.isCompleted)

            release.countDown()
            withTimeout(1_000) { delivery.await() }
            withTimeout(1_000) { close.await() }
            subscription.emitCount(2uL)
            assertEquals(1, notifications)
        }

    @Test
    fun searchSharesTheCurrentDeliveredValue() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = {
                        BridgeFixtures.searchResults(
                            albums = listOf(BridgeFixtures.albumSearchResult(id = "album-result")),
                        )
                    },
                )
            val tree = tree(handle)
            tree.subscribeSearch(Any(), "query") {}

            assertEquals(
                BrowseId.Album("album-result").mediaId,
                tree.search("query", 0, 20).single().mediaId,
            )
        }

    @Test
    fun replacingSearchRejectsOldDeliveriesAndClearsItsKnownCount() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = { query ->
                        BridgeFixtures.searchResults(
                            albums =
                                List(if (query == "first") 2 else 3) { index ->
                                    BridgeFixtures.albumSearchResult(id = "$query-$index")
                                },
                        )
                    },
                    initialSearchError = { query -> if (query == "second") queryFailure() else null },
                )
            val tree = tree(handle)
            val owner = Any()
            val notifications = mutableListOf<Int>()
            tree.subscribeSearch(owner, "first", notifications::add)

            assertTrue(runCatching { tree.subscribeSearch(owner, "second", notifications::add) }.isFailure)
            handle.failSearchResults(1, queryFailure())
            handle.emitSearchResults(
                0,
                BridgeFixtures.searchResults(
                    albums = List(9) { BridgeFixtures.albumSearchResult(id = "stale-$it") },
                ),
            )
            assertEquals(listOf(2), notifications)

            handle.emitSearchResults(
                1,
                BridgeFixtures.searchResults(albums = listOf(BridgeFixtures.albumSearchResult(id = "recovered"))),
            )
            assertEquals(listOf(2, 1), notifications)
            tree.close()
        }

    @Test
    fun unknownParentReturnsNull() =
        runBlocking {
            assertNull(tree(FakeAppHandle()).children("unknown", 0, 20))
        }

    private fun queryFailure(): BridgeException = BridgeException.Diagnostic(BridgeErrorCategory.Database, "query failed")
}
