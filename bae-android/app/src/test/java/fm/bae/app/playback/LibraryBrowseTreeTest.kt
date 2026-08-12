package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import fm.bae.app.BridgeFixtures
import fm.bae.app.data.Library
import fm.bae.app.testCoverRef
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.supervisorScope
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
import uniffi.bae_bridge.BridgeComposerDetail
import uniffi.bae_bridge.BridgeComposerWorkGroup
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeMetadataSource
import uniffi.bae_bridge.BridgeReleaseRoleSummary
import uniffi.bae_bridge.BridgeTrack
import uniffi.bae_bridge.BridgeTrackGroup
import uniffi.bae_bridge.BridgeTrackSide
import uniffi.bae_bridge.BridgeWorkDetail
import uniffi.bae_bridge.BridgeWorkReleaseSummary

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
            artworkUri = { coverId -> Uri.parse("content://test/cover/$coverId") },
            onChildrenChanged = onChildrenChanged,
        )

    private fun track(
        id: String,
        title: String,
    ): BridgeTrack =
        BridgeTrack(
            id = id,
            title = title,
            side = 0,
            trackNumber = null,
            durationMs = 180_000L,
            durationClock = null,
            artistNames = "Artist Name",
            displayArtist = null,
            positionText = "1",
        )

    private val MediaItem.title: String?
        get() = mediaMetadata.title?.toString()

    @Test
    fun rootIsBrowsableWithTheRootId() {
        val root = tree(FakeAppHandle()).root()
        assertEquals(BrowseId.Root.mediaId, root.mediaId)
        assertTrue(root.mediaMetadata.isBrowsable == true)
        assertFalse(root.mediaMetadata.isPlayable == true)
    }

    @Test
    fun rootChildrenAreTheTwoCategories() =
        runBlocking {
            val children = tree(FakeAppHandle()).children(BrowseId.Root.mediaId, page = 0, pageSize = 10)!!
            assertEquals(listOf(BrowseId.Albums.mediaId, BrowseId.Composers.mediaId), children.map { it.mediaId })
            assertEquals(listOf("Albums", "Composers"), children.map { it.title })
            assertTrue(children.all { it.mediaMetadata.isBrowsable == true })
        }

    @Test
    fun albumsCategoryPagesAlbumsAndHonorsTheWindow() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    albumPages = { _, _ -> listOf(BridgeFixtures.album(id = "album-1", title = "First Album")) },
                )
            val children = tree(handle).children(BrowseId.Albums.mediaId, page = 2, pageSize = 20)!!

            assertEquals(listOf(BrowseId.Album("album-1").mediaId), children.map { it.mediaId })
            assertEquals(listOf("First Album"), children.map { it.title })
            assertTrue(children.single().mediaMetadata.isBrowsable == true)
            // page 2 × pageSize 20 → offset 40, limit 20, straight to the bridge.
            assertEquals(40uL to 20uL, handle.albumPageWindows.single())
            assertFalse(handle.liveSubscriptions.single().cancelled)
        }

    @Test
    fun albumParentStaysLiveAndReportsDatabaseChanges() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count ->
                notifications += parentId to count
            }

            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)
            handle.emitAlbumPage(
                subscription = 0,
                rows = listOf(BridgeFixtures.album(id = "album-new")),
            )
            handle.emitAlbumPage(
                subscription = 1,
                rows = listOf(BridgeFixtures.album(id = "album-new")),
            )

            assertEquals(BrowseId.Albums.mediaId to 1, notifications.last())
            assertEquals(
                listOf(BrowseId.Album("album-new").mediaId),
                tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)!!.map { it.mediaId },
            )
            assertEquals(2, handle.albumPageCallbacks.size)
        }

    @Test
    fun parentNotificationMakesImmediateRefetchAwaitItsPageGeneration() =
        runBlocking {
            val oldAlbum = BridgeFixtures.album(id = "album-old")
            val newAlbum = BridgeFixtures.album(id = "album-new")
            val handle = FakeAppHandle(albumPages = { _, _ -> listOf(oldAlbum) })
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)

            handle.emitAlbumPage(subscription = 0, rows = emptyList(), totalCount = 2uL)
            val error =
                supervisorScope {
                    val refetch = async { tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20) }
                    yield()

                    assertEquals(listOf(BrowseId.Albums.mediaId to 2), notifications)
                    assertFalse(refetch.isCompleted)
                    handle.failAlbumPage(subscription = 1)
                    runCatching { refetch.await() }.exceptionOrNull()
                }
            assertTrue(error is BridgeException.Diagnostic)

            handle.emitAlbumPage(subscription = 1, rows = listOf(newAlbum), totalCount = 2uL)
            assertEquals(
                BrowseId.Album("album-new").mediaId,
                tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)!!.single().mediaId,
            )
        }

    @Test
    fun pageDeliveryBeforeParentNotificationSatisfiesImmediateRefetch() =
        runBlocking {
            val oldAlbum = BridgeFixtures.album(id = "album-old")
            val newAlbum = BridgeFixtures.album(id = "album-new")
            val handle = FakeAppHandle(albumPages = { _, _ -> listOf(oldAlbum) })
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)

            handle.emitAlbumPage(subscription = 1, rows = listOf(newAlbum), totalCount = 2uL)
            handle.emitAlbumPage(subscription = 0, rows = emptyList(), totalCount = 2uL)

            assertEquals(listOf(BrowseId.Albums.mediaId to 2), notifications)
            assertEquals(
                BrowseId.Album("album-new").mediaId,
                tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)!!.single().mediaId,
            )
        }

    @Test
    fun pageSubscriptionsStayBoundedAndEvictedPagesCancel() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)

            repeat(13) { page ->
                tree.children(BrowseId.Albums.mediaId, page = page, pageSize = 20)
            }

            assertTrue(handle.liveSubscriptions.count { !it.cancelled } <= 12)
            assertTrue(handle.liveSubscriptions.first().cancelled)
        }

    @Test
    fun parentSubscriptionObservesChangesWithoutAChildrenRequest() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            val controller = Any()
            tree.subscribeParent(controller, BrowseId.Albums.mediaId)

            handle.emitAlbumPage(
                subscription = 0,
                rows = listOf(BridgeFixtures.album(id = "album-observed")),
            )

            assertEquals(BrowseId.Albums.mediaId to 1, notifications.single())
            assertEquals(1, handle.albumPageCallbacks.size)
        }

    @Test
    fun albumParentObservationUsesACountOnlyQueryAndReportsCountChanges() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)

            assertEquals(0uL to 0uL, handle.albumPageWindows.single())
            handle.emitAlbumPage(subscription = 0, rows = emptyList(), totalCount = 3uL)
            handle.emitAlbumPage(subscription = 0, rows = emptyList(), totalCount = 1uL)

            assertEquals(
                listOf(BrowseId.Albums.mediaId to 3, BrowseId.Albums.mediaId to 1),
                notifications,
            )
        }

    @Test
    fun retainedNonFirstPageImageChangeNotifiesItsParent() =
        runBlocking {
            val oldAlbum = BridgeFixtures.album(id = "album-page").copy(cover = testCoverRef("cover-old"))
            val handle =
                FakeAppHandle(
                    albumPages = { offset, _ -> if (offset == 40uL) listOf(oldAlbum) else emptyList() },
                )
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, page = 2, pageSize = 20)
            handle.emitAlbumPage(
                subscription = 1,
                rows = listOf(oldAlbum),
                totalCount = 41uL,
            )
            notifications.clear()

            handle.emitAlbumPage(
                subscription = 1,
                rows = listOf(oldAlbum.copy(cover = testCoverRef("cover-new"))),
                totalCount = 41uL,
            )

            assertEquals(listOf(BrowseId.Albums.mediaId to 41), notifications)
        }

    @Test
    fun implicitParentInterestsAreBoundedPerOwner() {
        val handle = FakeAppHandle()
        val tree = tree(handle)
        val controller = Any()

        repeat(13) { index ->
            tree.retainImplicitParent(controller, BrowseId.Album("album-$index").mediaId)
        }

        assertEquals(12, handle.albumDetailSubscriptions.count { !it.cancelled })
        assertTrue(handle.albumDetailSubscriptions.first().cancelled)
        assertFalse(handle.albumDetailSubscriptions.last().cancelled)
    }

    @Test
    fun explicitParentPromotionSurvivesImplicitPressure() {
        val handle = FakeAppHandle()
        val tree = tree(handle)
        val controller = Any()
        val promotedParent = BrowseId.Album("album-promoted").mediaId
        tree.retainImplicitParent(controller, promotedParent)
        tree.subscribeParent(controller, promotedParent)

        repeat(13) { index ->
            tree.retainImplicitParent(controller, BrowseId.Album("album-$index").mediaId)
        }

        assertFalse(handle.albumDetailSubscriptions.first().cancelled)
        tree.unsubscribeParent(controller, promotedParent)
        assertTrue(handle.albumDetailSubscriptions.first().cancelled)
    }

    @Test
    fun activeParentObservationDoesNotExemptRequestedPagesFromTheCacheBound() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)

            repeat(13) { page ->
                tree.children(BrowseId.Albums.mediaId, page = page, pageSize = 20)
            }
            handle.emitAlbumPage(
                subscription = 0,
                rows = listOf(BridgeFixtures.album(id = "album-observed")),
            )

            assertEquals(13, handle.albumPageSubscriptions.count { !it.cancelled })
            assertFalse(handle.albumPageSubscriptions.first().cancelled)
            assertTrue(handle.albumPageSubscriptions[1].cancelled)
            assertEquals(BrowseId.Albums.mediaId to 1, notifications.last())
        }

    @Test
    fun activeSearchObservationDoesNotExemptRequestedSearchesFromTheCacheBound() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle)
            val controller = Any()

            tree.subscribeSearch(controller, "observed") { count -> notifications += "observed" to count }
            repeat(9) { index ->
                tree.search("request-$index", page = 0, pageSize = 20)
            }
            handle.emitSearchResults(
                subscription = 0,
                value =
                    BridgeFixtures.searchResults(
                        albums = listOf(BridgeFixtures.albumSearchResult(id = "album-retained")),
                    ),
            )

            assertEquals(9, handle.searchSubscriptions.count { !it.cancelled })
            assertFalse(handle.searchSubscriptions.first().cancelled)
            assertTrue(handle.searchSubscriptions[1].cancelled)
            assertEquals("observed" to 1, notifications.last())
        }

    @Test
    fun searchNotificationMakesImmediateRefetchAwaitItsPageGeneration() =
        runBlocking {
            val oldResults =
                BridgeFixtures.searchResults(albums = listOf(BridgeFixtures.albumSearchResult(id = "album-old")))
            val newResults =
                BridgeFixtures.searchResults(albums = listOf(BridgeFixtures.albumSearchResult(id = "album-new")))
            val handle = FakeAppHandle(searchResults = { oldResults })
            val notifications = mutableListOf<Int>()
            val tree = tree(handle)
            tree.subscribeSearch(Any(), "query", notifications::add)
            tree.search("query", page = 0, pageSize = 20)
            notifications.clear()

            handle.emitSearchResults(subscription = 0, value = newResults)
            val error =
                supervisorScope {
                    val refetch = async { tree.search("query", page = 0, pageSize = 20) }
                    yield()

                    assertEquals(listOf(1), notifications)
                    assertFalse(refetch.isCompleted)
                    handle.failSearchResults(subscription = 1, error = queryFailure())
                    runCatching { refetch.await() }.exceptionOrNull()
                }
            assertTrue(error is BridgeException.Diagnostic)

            handle.emitSearchResults(subscription = 1, value = newResults)
            assertEquals(
                BrowseId.Album("album-new").mediaId,
                tree.search("query", page = 0, pageSize = 20).single().mediaId,
            )
        }

    @Test
    fun searchPageBeforeNotificationSatisfiesImmediateRefetch() =
        runBlocking {
            val oldResults =
                BridgeFixtures.searchResults(albums = listOf(BridgeFixtures.albumSearchResult(id = "album-old")))
            val newResults =
                BridgeFixtures.searchResults(albums = listOf(BridgeFixtures.albumSearchResult(id = "album-new")))
            val handle = FakeAppHandle(searchResults = { oldResults })
            val notifications = mutableListOf<Int>()
            val tree = tree(handle)
            tree.subscribeSearch(Any(), "query", notifications::add)
            tree.search("query", page = 0, pageSize = 20)
            notifications.clear()

            handle.emitSearchResults(subscription = 1, value = newResults)
            handle.emitSearchResults(subscription = 0, value = newResults)

            assertEquals(listOf(1), notifications)
            assertEquals(
                BrowseId.Album("album-new").mediaId,
                tree.search("query", page = 0, pageSize = 20).single().mediaId,
            )
        }

    @Test
    fun unsubscribeDoesNotCancelAnActiveChildrenRequest() =
        runBlocking {
            val handle = FakeAppHandle(deliverAlbumPagesImmediately = false)
            val tree = tree(handle)
            val controller = Any()
            tree.subscribeParent(controller, BrowseId.Albums.mediaId)
            val request = async { tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20) }
            yield()

            tree.unsubscribeParent(controller, BrowseId.Albums.mediaId)
            handle.emitAlbumPage(1, listOf(BridgeFixtures.album(id = "album-request")))

            assertEquals(BrowseId.Album("album-request").mediaId, request.await()!!.single().mediaId)
            assertTrue(handle.albumPageSubscriptions[0].cancelled)
        }

    @Test
    fun disconnectDoesNotCancelActiveSearchOrSpokenRequests() =
        runBlocking {
            val handle = FakeAppHandle(deliverSearchResultsImmediately = false)
            val tree = tree(handle)
            val controller = Any()
            val listener = async { tree.subscribeSearch(controller, "listener") {} }
            yield()
            handle.emitSearchResults(0, BridgeFixtures.searchResults())
            listener.await()
            val results = async { tree.search("results", page = 0, pageSize = 20) }
            val spoken = async { tree.searchTopPlayable("spoken") }
            yield()

            tree.disconnect(controller)
            handle.emitSearchResults(
                1,
                BridgeFixtures.searchResults(albums = listOf(BridgeFixtures.albumSearchResult(id = "album-result"))),
            )
            handle.emitSearchResults(2, BridgeFixtures.searchResults())

            assertEquals(BrowseId.Album("album-result").mediaId, results.await().single().mediaId)
            assertNull(spoken.await())
            assertTrue(handle.searchSubscriptions[0].cancelled)
        }

    @Test
    fun disconnectWaitsForAnInitialSearchListenerDeliveryBeforeCancelling() =
        runBlocking {
            val handle = FakeAppHandle(deliverSearchResultsImmediately = false)
            val tree = tree(handle)
            val controller = Any()
            val listener = async { tree.subscribeSearch(controller, "query") {} }
            yield()

            tree.disconnect(controller)
            assertFalse(handle.searchSubscriptions.single().cancelled)
            handle.emitSearchResults(0, BridgeFixtures.searchResults())
            listener.await()

            assertTrue(handle.searchSubscriptions.single().cancelled)
        }

    @Test
    fun aNewSearchListenerKeepsTheObservationWhileAnOldListenerRequestFinishes() =
        runBlocking {
            val handle = FakeAppHandle(deliverSearchResultsImmediately = false)
            val tree = tree(handle)
            val firstController = Any()
            val secondController = Any()
            val first = async { tree.subscribeSearch(firstController, "query") {} }
            yield()
            tree.disconnect(firstController)
            val second = async { tree.subscribeSearch(secondController, "query") {} }
            yield()

            handle.emitSearchResults(0, BridgeFixtures.searchResults())
            first.await()
            second.await()

            assertFalse(handle.searchSubscriptions.single().cancelled)
            tree.disconnect(secondController)
            assertTrue(handle.searchSubscriptions.single().cancelled)
        }

    @Test
    fun eachOwnerRetainsOnlyItsCurrentSearch() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)
            val controller = Any()

            repeat(12) { index -> tree.subscribeSearch(controller, "query-$index") {} }

            assertEquals(1, handle.searchSubscriptions.count { !it.cancelled })
            assertFalse(handle.searchSubscriptions.last().cancelled)
        }

    @Test
    fun repeatedSearchForTheSameOwnerAndQueryCoalesces() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)
            val controller = Any()

            repeat(2) { tree.subscribeSearch(controller, "query") {} }

            assertEquals(1, handle.searchSubscriptions.size)
            assertFalse(handle.searchSubscriptions.single().cancelled)
        }

    @Test
    fun replacedSearchRejectsItsLateNotification() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)
            val controller = Any()
            val notifications = mutableListOf<Pair<String, Int>>()
            tree.subscribeSearch(controller, "query-a") { notifications += "query-a" to it }
            tree.subscribeSearch(controller, "query-b") { notifications += "query-b" to it }
            notifications.clear()

            handle.emitSearchResults(
                0,
                BridgeFixtures.searchResults(
                    albums = listOf(BridgeFixtures.albumSearchResult(id = "album-stale")),
                ),
            )

            assertTrue(notifications.isEmpty())
            assertTrue(handle.searchSubscriptions[0].cancelled)
            assertFalse(handle.searchSubscriptions[1].cancelled)
        }

    @Test
    fun replacingSearchWaitsForTheOldInitialRequestBeforeCancelling() =
        runBlocking {
            val handle = FakeAppHandle(deliverSearchResultsImmediately = false)
            val tree = tree(handle)
            val controller = Any()
            val oldSearch = async { tree.subscribeSearch(controller, "query-a") {} }
            yield()
            val currentSearch = async { tree.subscribeSearch(controller, "query-b") {} }
            yield()

            assertFalse(handle.searchSubscriptions[0].cancelled)
            handle.emitSearchResults(0, BridgeFixtures.searchResults())
            oldSearch.await()
            assertTrue(handle.searchSubscriptions[0].cancelled)

            handle.emitSearchResults(1, BridgeFixtures.searchResults())
            currentSearch.await()
            assertFalse(handle.searchSubscriptions[1].cancelled)
            tree.disconnect(controller)
            assertTrue(handle.searchSubscriptions[1].cancelled)
        }

    @Test
    fun replacingSearchDoesNotCancelAnActiveOldResultPageRequest() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)
            val controller = Any()
            tree.subscribeSearch(controller, "query-a") {}
            handle.deliverSearchResultsImmediately = false
            val oldResults = async { tree.search("query-a", page = 0, pageSize = 20) }
            yield()
            val replacement = async { tree.subscribeSearch(controller, "query-b") {} }
            yield()

            assertFalse(handle.searchSubscriptions[1].cancelled)
            handle.emitSearchResults(
                1,
                BridgeFixtures.searchResults(
                    albums = listOf(BridgeFixtures.albumSearchResult(id = "album-request")),
                ),
            )
            assertEquals(BrowseId.Album("album-request").mediaId, oldResults.await().single().mediaId)

            handle.emitSearchResults(2, BridgeFixtures.searchResults())
            replacement.await()
            assertFalse(handle.searchSubscriptions[2].cancelled)
        }

    @Test
    fun unobservedSearchCacheStaysBounded() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)

            repeat(9) { index ->
                tree.search("query-$index", page = 0, pageSize = 20)
            }

            assertTrue(handle.liveSubscriptions.count { !it.cancelled } <= 8)
            assertTrue(handle.liveSubscriptions.first().cancelled)
        }

    @Test
    fun initialErrorCompletesTheRequestAndLaterValueRecovers() =
        runBlocking {
            val handle = FakeAppHandle(initialAlbumPageError = queryFailure())
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)

            val error =
                runCatching {
                    withTimeout(1_000) {
                        tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)
                    }
                }.exceptionOrNull()

            assertTrue(error is BridgeException.Diagnostic)
            assertTrue(handle.albumPageSubscriptions.all { !it.cancelled })

            handle.emitAlbumPage(
                subscription = 0,
                rows = listOf(BridgeFixtures.album(id = "album-recovered")),
            )
            handle.emitAlbumPage(
                subscription = 1,
                rows = listOf(BridgeFixtures.album(id = "album-recovered")),
            )
            assertEquals(BrowseId.Albums.mediaId to 1, notifications.single())
            assertEquals(
                BrowseId.Album("album-recovered").mediaId,
                tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)!!.single().mediaId,
            )
        }

    @Test
    fun replacedProjectionRejectsQueuedValuesErrorsAndNotifications() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val errors = mutableListOf<BridgeException>()
            val tree =
                LibraryBrowseTree<Any>(
                    library = Library(handle),
                    labels = { labels },
                    artworkUri = { image -> Uri.parse("content://test/cover/${image.id}") },
                    onChildrenChanged = { parentId, count -> notifications += parentId to count },
                    onQueryError = errors::add,
                )
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)
            repeat(12) { page ->
                tree.children(BrowseId.Albums.mediaId, page = page + 1, pageSize = 20)
            }
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)

            handle.failAlbumPage(subscription = 0)
            handle.emitAlbumPage(
                subscription = 0,
                rows = listOf(BridgeFixtures.album(id = "album-stale")),
            )

            assertTrue(errors.isEmpty())
            assertTrue(notifications.isEmpty())
            assertTrue(
                tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)!!.isEmpty(),
            )

            handle.emitAlbumPage(
                subscription = 13,
                rows =
                    listOf(
                        BridgeFixtures.album(id = "album-current").copy(
                            cover = testCoverRef("cover-current"),
                        ),
                    ),
            )
            notifications.clear()
            handle.emitAlbumPage(
                subscription = 13,
                rows =
                    listOf(
                        BridgeFixtures.album(id = "album-current").copy(
                            cover = testCoverRef("cover-updated"),
                        ),
                    ),
            )
            assertEquals(listOf(BrowseId.Albums.mediaId to 1), notifications)
            assertEquals(
                BrowseId.Album("album-current").mediaId,
                tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)!!.single().mediaId,
            )
        }

    @Test
    fun closingTreeCancelsRetainedQueries() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)

            tree.close()

            assertTrue(handle.liveSubscriptions.single().cancelled)
        }

    @Test
    fun searchStaysLiveAndReportsLaterResults() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = {
                        BridgeFixtures.searchResults(
                            albums = listOf(BridgeFixtures.albumSearchResult(id = "album-new")),
                        )
                    },
                )
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree =
                LibraryBrowseTree<Any>(
                    library = Library(handle),
                    labels = { labels },
                    artworkUri = { image -> Uri.parse("content://test/cover/${image.id}") },
                )

            tree.subscribeSearch(Any(), "query") { count -> notifications += "query" to count }
            handle.emitSearchResults(
                subscription = 0,
                value =
                    BridgeFixtures.searchResults(
                        albums = listOf(BridgeFixtures.albumSearchResult(id = "album-new")),
                    ),
            )

            assertEquals("query" to 1, notifications.last())
            assertEquals(
                BrowseId.Album("album-new").mediaId,
                tree.search("query", page = 0, pageSize = 10).single().mediaId,
            )
            assertEquals(2, handle.searchCallbacks.size)
        }

    @Test
    fun browseQueryRecoversAfterNonterminalError() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parentId, count -> notifications += parentId to count }

            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)
            handle.failAlbumPage(subscription = 1)
            handle.emitAlbumPage(
                subscription = 0,
                rows = listOf(BridgeFixtures.album(id = "album-recovered")),
            )
            handle.emitAlbumPage(
                subscription = 1,
                rows = listOf(BridgeFixtures.album(id = "album-recovered")),
            )

            assertEquals(BrowseId.Albums.mediaId to 1, notifications.last())
            assertEquals(
                BrowseId.Album("album-recovered").mediaId,
                tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)!!.single().mediaId,
            )
        }

    @Test
    fun albumDrillsToItsPrimaryReleaseTracksWithFlatIndices() =
        runBlocking {
            val album = BridgeFixtures.album(id = "album-1", primaryReleaseId = "rel-1")
            val release =
                BridgeFixtures.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups =
                        listOf(
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Sided("A"),
                                headerKey = "core.track.side",
                                tracks = listOf(track("t0", "Opener"), track("t1", "Second")),
                            ),
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Sided("B"),
                                headerKey = "core.track.side",
                                tracks = listOf(track("t2", "Third")),
                            ),
                        ),
                )
            val handle = FakeAppHandle(albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(album, listOf(release))))

            val children = tree(handle).children(BrowseId.Album("album-1").mediaId, page = 0, pageSize = 10)!!

            assertEquals(
                listOf(
                    BrowseId.Track("rel-1", 0).mediaId,
                    BrowseId.Track("rel-1", 1).mediaId,
                    BrowseId.Track("rel-1", 2).mediaId,
                ),
                children.map { it.mediaId },
            )
            assertEquals(listOf("Opener", "Second", "Third"), children.map { it.title })
            assertTrue(children.all { it.mediaMetadata.isPlayable == true })
            assertTrue(children.none { it.mediaMetadata.isBrowsable == true })
        }

    @Test
    fun trackChildPageIsSliced() =
        runBlocking {
            val album = BridgeFixtures.album(id = "album-1", primaryReleaseId = "rel-1")
            val release =
                BridgeFixtures.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups =
                        listOf(
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Flat,
                                headerKey = null,
                                tracks = listOf(track("t0", "A"), track("t1", "B"), track("t2", "C")),
                            ),
                        ),
                )
            val handle = FakeAppHandle(albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(album, listOf(release))))

            val children = tree(handle).children(BrowseId.Album("album-1").mediaId, page = 1, pageSize = 2)!!

            // page 1 × 2 → skip 2, take 2 → only the third track survives.
            assertEquals(listOf(BrowseId.Track("rel-1", 2).mediaId), children.map { it.mediaId })
        }

    @Test
    fun composersCategoryPagesComposers() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    composerPages = { _, _ -> listOf(BridgeFixtures.composerSummary(artistId = "artist-1", name = "A Composer")) },
                )
            val children = tree(handle).children(BrowseId.Composers.mediaId, page = 0, pageSize = 50)!!

            assertEquals(listOf(BrowseId.Composer("artist-1").mediaId), children.map { it.mediaId })
            assertEquals(listOf("A Composer"), children.map { it.title })
            assertEquals(0uL to 50uL, handle.composerPageWindows.single())
        }

    @Test
    fun composerDrillsToWorksThenCreditedAlbums() =
        runBlocking {
            val detail =
                BridgeComposerDetail(
                    composer = BridgeFixtures.composerSummary(artistId = "artist-1"),
                    workGroups =
                        listOf(
                            BridgeComposerWorkGroup(
                                id = "group-1",
                                parent = null,
                                works = listOf(BridgeFixtures.workSummary(workId = "work-1", title = "A Work")),
                            ),
                        ),
                    unlinkedReleaseRoles =
                        listOf(
                            BridgeReleaseRoleSummary(
                                releaseId = "rel-9",
                                albumId = "album-9",
                                albumTitle = "Credited Album",
                                source = BridgeMetadataSource.MUSIC_BRAINZ,
                                sourceCredit = null,
                            ),
                        ),
                    unlinkedTrackRoles = emptyList(),
                    defaultWorkId = null,
                )
            val handle = FakeAppHandle(composerDetails = mapOf("artist-1" to detail))

            val children = tree(handle).children(BrowseId.Composer("artist-1").mediaId, page = 0, pageSize = 10)!!

            assertEquals(
                listOf(BrowseId.Work("work-1").mediaId, BrowseId.Album("album-9").mediaId),
                children.map { it.mediaId },
            )
            assertEquals(listOf("A Work", "Credited Album"), children.map { it.title })
        }

    @Test
    fun workDrillsToChildWorksThenReleases() =
        runBlocking {
            val detail =
                BridgeWorkDetail(
                    work = BridgeFixtures.workSummary(workId = "work-1"),
                    childWorks = listOf(BridgeFixtures.workSummary(workId = "work-2", title = "Child Work")),
                    releases =
                        listOf(
                            BridgeWorkReleaseSummary(
                                releaseId = "rel-3",
                                albumId = "album-3",
                                albumTitle = "Work Album",
                                displayName = "Release",
                                format = null,
                                cover = null,
                            ),
                        ),
                    tracks = emptyList(),
                )
            val handle = FakeAppHandle(workDetails = mapOf("work-1" to detail))

            val children = tree(handle).children(BrowseId.Work("work-1").mediaId, page = 0, pageSize = 10)!!

            assertEquals(
                listOf(BrowseId.Work("work-2").mediaId, BrowseId.Album("album-3").mediaId),
                children.map { it.mediaId },
            )
        }

    @Test
    fun unknownParentReturnsNull() =
        runBlocking {
            assertNull(tree(FakeAppHandle()).children("nonsense", page = 0, pageSize = 10))
        }

    @Test
    fun searchReturnsBrowsableAlbumResults() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = {
                        BridgeFixtures.searchResults(
                            albums = listOf(BridgeFixtures.albumSearchResult(id = "album-7", title = "Found Album")),
                        )
                    },
                )
            val results = tree(handle).search("query", page = 0, pageSize = 10)

            assertEquals(listOf(BrowseId.Album("album-7").mediaId), results.map { it.mediaId })
            assertEquals(listOf("Found Album"), results.map { it.title })
            assertTrue(results.single().mediaMetadata.isBrowsable == true)
        }

    @Test
    fun spokenQueryResolvesToATrackInItsPrimaryRelease() =
        runBlocking {
            val album = BridgeFixtures.album(id = "album-1", primaryReleaseId = "rel-1")
            val release =
                BridgeFixtures.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups =
                        listOf(
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Flat,
                                headerKey = null,
                                tracks = listOf(track("t0", "A"), track("t1", "B")),
                            ),
                        ),
                )
            val handle =
                FakeAppHandle(
                    albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(album, listOf(release))),
                    searchResults = {
                        BridgeFixtures.searchResults(
                            tracks = listOf(BridgeFixtures.trackSearchResult(id = "t1", albumId = "album-1")),
                        )
                    },
                )

            assertEquals(BrowseId.Track("rel-1", 1), tree(handle).searchTopPlayable("query"))
        }

    @Test
    fun spokenQueryWithNoResultsResolvesToNull() =
        runBlocking {
            assertNull(tree(FakeAppHandle()).searchTopPlayable("query"))
        }

    @Test
    fun itemResolvesATrackByReleaseAndIndex() =
        runBlocking {
            val release =
                BridgeFixtures.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups =
                        listOf(
                            BridgeTrackGroup(
                                side = BridgeTrackSide.Flat,
                                headerKey = null,
                                tracks = listOf(track("t0", "A"), track("t1", "B")),
                            ),
                        ),
                )
            val handle = FakeAppHandle(releaseDetails = mapOf("rel-1" to release))

            val item = tree(handle).item(BrowseId.Track("rel-1", 1).mediaId)!!

            assertEquals(BrowseId.Track("rel-1", 1).mediaId, item.mediaId)
            assertEquals("B", item.title)
            assertTrue(item.mediaMetadata.isPlayable == true)
        }

    private fun queryFailure(): BridgeException =
        BridgeException.Diagnostic(
            category = BridgeErrorCategory.DATABASE,
            detail = "query failed",
        )
}
