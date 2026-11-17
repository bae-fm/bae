package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.HANG_GUARD
import fm.bae.app.HANG_GUARD_MS
import fm.bae.app.plainCreditText
import fm.bae.app.ui.PreviewData
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeComposerDetailItem
import uniffi.bae_bridge.BridgeComposerDetailRow
import uniffi.bae_bridge.BridgeComposerHeading
import uniffi.bae_bridge.BridgeCreditRole
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeReleaseRoleSummary
import uniffi.bae_bridge.BridgeTermLabel
import uniffi.bae_bridge.BridgeWorkChild
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class LibraryBrowseTreeTest {
    private val labels = BrowseLabels(library = "Library", albums = "Albums", composers = "Composers")

    private fun tree(
        handle: FakeAppHandle,
        onChildrenChanged: (String, Int) -> Unit = { _, _ -> },
    ): LibraryBrowseTree<Any> =
        LibraryBrowseTree(
            handle = handle,
            labels = { labels },
            nodes = BrowseNodeFactory({ Uri.parse("content://test/cover/${it.id}") }, ::plainCreditText),
            onChildrenChanged = onChildrenChanged,
        )

    /** An album's children on each page are the tracks of a view with the page on screen, and none past the end. */
    @Test
    fun anAlbumsChildrenAreTheWindowOfThePageAsked() =
        runBlocking {
            val tracks = List(3) { BridgeFixtures.browseTrack(id = "trk-$it", releaseId = "rel-main") }
            val handle = FakeAppHandle(albumTracks = mapOf("album-1" to tracks))
            val tree = tree(handle)
            val album = BrowseId.Album("album-1").mediaId

            assertEquals(
                listOf(BrowseId.Track("rel-main", "trk-0").mediaId, BrowseId.Track("rel-main", "trk-1").mediaId),
                tree.children(album, 0, 2)!!.map { it.mediaId },
            )
            assertEquals(listOf(BrowseId.Track("rel-main", "trk-2").mediaId), tree.children(album, 1, 2)!!.map { it.mediaId })
            assertTrue(tree.children(album, 2, 2)!!.isEmpty())
            assertEquals(listOf(0..1, 2..3), handle.albumTrackList("album-1").views.map { it.reported.first() })
            tree.close()
        }

    /**
     * A change inside a page the controller asked for tells the controller the album's children changed, with
     * their number unknown, and the controller's next request for the page reads it again.
     */
    @Test
    fun aChangeInsideAnAskedPageTellsTheControllerAndTheNextRequestReadsIt() =
        runBlocking {
            val handle =
                FakeAppHandle(albumTracks = mapOf("album-1" to listOf(BridgeFixtures.browseTrack(id = "trk-0", title = "Old"))))
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }
            val album = BrowseId.Album("album-1").mediaId
            tree.subscribeParent(Any(), album)
            assertEquals(
                "Old",
                tree
                    .children(album, 0, 10)!!
                    .single()
                    .mediaMetadata.title,
            )

            val list = handle.albumTrackList("album-1")
            list.rows = listOf(BridgeFixtures.browseTrack(id = "trk-0", title = "New"))
            list.views.single().deliver()

            assertEquals(listOf(album to UNKNOWN_CHILD_COUNT), notifications)
            assertEquals(
                "New",
                tree
                    .children(album, 0, 10)!!
                    .single()
                    .mediaMetadata.title,
            )
            assertTrue(list.views.first().cancelled)
            tree.close()
            assertTrue(list.views.last().cancelled)
        }

    /** The tree tells the controller each time an album's live track count changes, but not when the read delivers its first count. */
    @Test
    fun anAlbumsTrackCountTellsTheControllerEachChange() =
        runBlocking {
            val handle = FakeAppHandle(albumTracks = mapOf("album-1" to listOf(BridgeFixtures.browseTrack())))
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }
            val album = BrowseId.Album("album-1").mediaId

            tree.subscribeParent(Any(), album)
            assertTrue(notifications.isEmpty())
            val list = handle.albumTrackList("album-1")
            list.rows = List(2) { BridgeFixtures.browseTrack(id = "trk-$it") }
            list.emitCount()

            assertEquals(listOf(album to 2), notifications)
            tree.close()
            assertTrue(list.counts.single().cancelled)
        }

    /** A work's children on each page are its parts, then its releases as their albums, as the page's window reads them. */
    @Test
    fun aWorksChildrenAreTheWindowOfThePageAsked() =
        runBlocking {
            val children =
                listOf(
                    BridgeWorkChild.Part(BridgeFixtures.workSummary(workId = "child-1")),
                    BridgeWorkChild.Release(BridgeFixtures.workRelease(releaseId = "child-2", albumId = "album-1")),
                    BridgeWorkChild.Release(BridgeFixtures.workRelease(releaseId = "child-3", albumId = "album-2")),
                )
            val tree = tree(FakeAppHandle(workChildren = mapOf("work-1" to children)))
            val work = BrowseId.Work("work-1").mediaId

            assertEquals(
                listOf(BrowseId.Work("child-1").mediaId, BrowseId.Album("album-1").mediaId),
                tree.children(work, 0, 2)!!.map { it.mediaId },
            )
            assertEquals(listOf(BrowseId.Album("album-2").mediaId), tree.children(work, 1, 2)!!.map { it.mediaId })
            assertTrue(tree.children(work, 2, 2)!!.isEmpty())
            tree.close()
        }

    /** A composer's children are its page's rows, the window the car asks for: works, then credits as their albums. */
    @Test
    fun aComposersChildrenAreItsPageRowsInTheWindowAsked() =
        runBlocking {
            val rows =
                listOf(
                    BridgeComposerDetailRow(
                        0L,
                        BridgeComposerHeading.WORKS,
                        BridgeComposerDetailItem.Work(PreviewData.workSummary(workId = "work-head"), false),
                    ),
                    BridgeComposerDetailRow(
                        1L,
                        null,
                        BridgeComposerDetailItem.Work(PreviewData.workSummary(workId = "work-part"), true),
                    ),
                    BridgeComposerDetailRow(
                        2L,
                        BridgeComposerHeading.CREDITS,
                        BridgeComposerDetailItem.ReleaseCredit(
                            BridgeReleaseRoleSummary(
                                "rel-1",
                                "album-credited",
                                "Album Title",
                                BridgeCreditRole(BridgeTermLabel.Verbatim("Written-By"), emptyList()),
                            ),
                        ),
                    ),
                )
            val tree = tree(FakeAppHandle(composerDetailRows = mapOf("artist-1" to rows)))

            val first = tree.children(BrowseId.Composer("artist-1").mediaId, 0, 2)!!
            val second = tree.children(BrowseId.Composer("artist-1").mediaId, 1, 2)!!

            assertEquals(listOf(BrowseId.Work("work-head").mediaId, BrowseId.Work("work-part").mediaId), first.map { it.mediaId })
            assertEquals(listOf(BrowseId.Album("album-credited").mediaId), second.map { it.mediaId })
            tree.close()
        }

    private val MediaItem.title: String?
        get() = mediaMetadata.title?.toString()

    @Test
    fun rootAndCategoriesAreBrowsable() =
        runBlocking {
            val tree = tree(FakeAppHandle())
            assertTrue(tree.root.mediaMetadata.isBrowsable == true)
            assertEquals("Library", tree.root.title)
            val children = tree.children(BrowseId.Root.mediaId, 0, 10)!!
            assertEquals(listOf(BrowseId.Albums.mediaId, BrowseId.Composers.mediaId), children.map { it.mediaId })
            assertEquals(listOf("Albums", "Composers"), children.map { it.title })
            tree.close()
        }

    /** The car asks for a page by offset. The tree opens a view of the albums with that page's rows on screen. */
    @Test
    fun aPageIsAViewWithItsRowsOnScreen() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(100)))
            val children = tree(handle).children(BrowseId.Albums.mediaId, 2, 20)!!

            assertEquals(List(20) { BrowseId.Album("album-%04d".format(40 + it)).mediaId }, children.map { it.mediaId })
            assertEquals(
                listOf(40..59),
                handle.albumList.views
                    .single()
                    .reported,
            )
            assertEquals(1, handle.albumList.counts.size)
        }

    /** The car gets exactly the page it asks for, however large, never one cut down to a cap. */
    @Test
    fun aLargePageIsReadAsAsked() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(1000)))
            val children = tree(handle).children(BrowseId.Albums.mediaId, 1, 800)!!

            assertEquals(200, children.size)
            assertEquals(
                listOf(800..1599),
                handle.albumList.views
                    .single()
                    .reported,
            )
        }

    /** A page past the list's end is empty, and opens no view. */
    @Test
    fun aPagePastTheEndIsEmpty() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(30)))

            assertEquals(emptyList<MediaItem>(), tree(handle).children(BrowseId.Albums.mediaId, 2, 20))
            assertTrue(handle.albumList.views.isEmpty())
        }

    /** A request that names no page is answered as a bad value, not read as page 0 or one item. */
    @Test
    fun aRequestNamingNoPageIsRefused() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)

            assertNull(tree.children(BrowseId.Albums.mediaId, -1, 20))
            assertNull(tree.children(BrowseId.Albums.mediaId, 0, 0))
            assertNull(tree.search(Any(), "query", 0, 0))
            assertTrue(handle.albumList.views.isEmpty())
            tree.close()
        }

    @Test
    fun askingForAPageDoesNotNotifyButAChangeToItsRowsDoes() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(listOf(BridgeFixtures.album(id = "album-a"))))
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)

            tree.children(BrowseId.Albums.mediaId, 0, 20)
            assertTrue(notifications.isEmpty())

            handle.albumList.albums = listOf(BridgeFixtures.album(id = "album-new"))
            handle.albumList.changed()
            assertEquals(listOf(BrowseId.Albums.mediaId to 1), notifications)
            assertEquals(
                BrowseId.Album("album-new").mediaId,
                tree.children(BrowseId.Albums.mediaId, 0, 20)!!.single().mediaId,
            )
        }

    /** When a write changes the album count, the tree drops the pages it read before and tells the car once. */
    @Test
    fun aWriteThatChangesTheCountNotifiesExactlyOnce() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(3)))
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, 0, 20)
            assertTrue(notifications.isEmpty())

            handle.albumList.albums = albums(4)
            handle.albumList.changed()

            assertEquals(listOf(BrowseId.Albums.mediaId to 4), notifications)
            assertEquals(4, tree.children(BrowseId.Albums.mediaId, 0, 20)!!.size)
            tree.close()
        }

    @Test
    fun parentInterestWithNoRequestedPageNotifiesOnACountChange() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }

            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            assertTrue(handle.albumList.views.isEmpty())
            assertTrue(notifications.isEmpty())

            handle.albumList.albums = albums(7)
            handle.albumList.emitCount()

            assertEquals(listOf(BrowseId.Albums.mediaId to 7), notifications)
            tree.close()
        }

    @Test
    fun livePagesStayBounded() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(400)))
            val tree = tree(handle)

            repeat(13) { tree.children(BrowseId.Albums.mediaId, it, 20) }

            assertEquals(1, handle.albumList.counts.size)
            assertEquals(13, handle.albumList.views.size)
            assertEquals(12, handle.albumList.views.count { !it.cancelled })
            assertTrue(
                handle.albumList.views
                    .first()
                    .cancelled,
            )
            tree.close()
        }

    @Test
    fun aPageRequestWaitsForItsView() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(40), deliverImmediately = false))
            val tree = tree(handle)
            val request = async { tree.children(BrowseId.Albums.mediaId, 1, 20) }
            withTimeout(HANG_GUARD) { handle.albumList.firstCount.await() }
            assertTrue(handle.albumList.views.isEmpty())
            handle.albumList.emitCount()
            val view = withTimeout(HANG_GUARD) { handle.albumList.firstView.await() }
            assertFalse(request.isCompleted)

            view.deliver()

            assertEquals(List(20) { BrowseId.Album("album-%04d".format(20 + it)).mediaId }, request.await()!!.map { it.mediaId })
        }

    @Test
    fun nonterminalErrorCompletesTheRequestAndALaterReadRecovers() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(5), windowFailure = queryFailure()))
            val tree = tree(handle)

            val error = runCatching { tree.children(BrowseId.Albums.mediaId, 0, 20) }.exceptionOrNull()
            assertTrue(error is BridgeException)

            handle.albumList.windowFailure = null
            assertEquals(5, tree.children(BrowseId.Albums.mediaId, 0, 20)!!.size)
        }

    @Test
    fun closeCancelsTheReadsAndCompletesAWaiter() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(5), deliverImmediately = false))
            val tree = tree(handle)
            val request = async { runCatching { tree.children(BrowseId.Albums.mediaId, 0, 20) }.exceptionOrNull() }
            withTimeout(HANG_GUARD) { handle.albumList.firstCount.await() }
            handle.albumList.emitCount()
            withTimeout(HANG_GUARD) { handle.albumList.firstView.await() }

            tree.close()

            assertTrue(
                handle.albumList.views
                    .single()
                    .cancelled,
            )
            assertTrue(
                handle.albumList.counts
                    .single()
                    .cancelled,
            )
            assertTrue(withTimeout(HANG_GUARD) { request.await() } is BrowseUnavailable.LibraryClosed)
        }

    @Test
    fun closeWaitsForCommittedNotificationAndPreventsLaterCallbacks() =
        runBlocking {
            val entered = CountDownLatch(1)
            val release = CountDownLatch(1)
            var notifications = 0
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(1)))
            val tree =
                tree(handle) { _, _ ->
                    notifications++
                    entered.countDown()
                    release.await()
                }
            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, 0, 20)
            val count = handle.albumList.counts.single()
            handle.albumList.albums = albums(2)
            val delivery = async(Dispatchers.IO) { count.deliver() }
            assertTrue(entered.await(HANG_GUARD_MS, TimeUnit.MILLISECONDS))

            val close = async(Dispatchers.IO) { tree.close() }
            // Closing cancels the count, then waits for the notification in flight.
            withTimeout(HANG_GUARD) { count.cancelledSignal.await() }
            assertFalse(close.isCompleted)

            release.countDown()
            withTimeout(HANG_GUARD) { delivery.await() }
            withTimeout(HANG_GUARD) { close.await() }
            handle.albumList.albums = albums(3)
            count.deliver()
            assertEquals(1, notifications)
        }

    /**
     * A search page is the window of the controller's search that holds the page asked for. A request for a page past
     * the end returns no albums.
     */
    @Test
    fun aSearchPageIsTheWindowOfThePageAsked() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = {
                        BridgeFixtures.searchResults(
                            albums = List(5) { BridgeFixtures.albumSearchResult(id = "album-$it") },
                        )
                    },
                )
            val tree = tree(handle)
            val owner = Any()
            tree.subscribeSearch(owner, "query") {}

            assertEquals(
                listOf(BrowseId.Album("album-2").mediaId, BrowseId.Album("album-3").mediaId),
                tree.search(owner, "query", 1, 2)!!.map { it.mediaId },
            )
            assertTrue(tree.search(owner, "query", 3, 2)!!.isEmpty())
            assertEquals(
                listOf(2..3),
                handle.searchAlbumList("query").views.map { it.reported.first() },
            )
            tree.close()
        }

    /** The tree refuses a request for the results of a phrase the controller is not searching for, and reads nothing. */
    @Test
    fun aSearchPageOfAnotherPhraseIsRefused() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)
            val owner = Any()

            assertNull(tree.search(owner, "query", 0, 10))
            tree.subscribeSearch(owner, "query") {}
            assertNull(tree.search(owner, "other", 0, 10))
            assertNull(tree.search(Any(), "query", 0, 10))
            assertTrue(handle.searchAlbumList("query").views.isEmpty())
            tree.close()
        }

    /**
     * A change inside a search page the controller asked for tells the controller its search's results changed,
     * with their number unknown, and the controller's next request for the page reads the renamed album.
     */
    @Test
    fun aChangeInsideASearchPageTellsTheControllerAndTheNextRequestReadsIt() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = {
                        BridgeFixtures.searchResults(albums = listOf(BridgeFixtures.albumSearchResult(id = "album-0", title = "Old")))
                    },
                )
            val tree = tree(handle)
            val owner = Any()
            val notifications = mutableListOf<Int>()
            tree.subscribeSearch(owner, "query", notifications::add)
            assertEquals(
                "Old",
                tree
                    .search(owner, "query", 0, 10)!!
                    .single()
                    .mediaMetadata.title,
            )

            val list = handle.searchAlbumList("query")
            list.rows = listOf(BridgeFixtures.albumSearchResult(id = "album-0", title = "New"))
            list.views.single().deliver()

            assertEquals(listOf(1, UNKNOWN_CHILD_COUNT), notifications)
            assertEquals(
                "New",
                tree
                    .search(owner, "query", 0, 10)!!
                    .single()
                    .mediaMetadata.title,
            )
            assertTrue(list.views.first().cancelled)
            tree.close()
            assertTrue(list.views.last().cancelled)
        }

    /** A track item is the one track the database read returns, found by its release and track ids. */
    @Test
    fun aTrackItemIsTheTrackTheReadReturns() =
        runBlocking {
            val release =
                PreviewData.release(
                    id = "rel-1",
                    albumId = "album-1",
                    trackGroups = listOf(PreviewData.trackGroup(listOf(PreviewData.track("trk-a"), PreviewData.track("trk-b")))),
                )
            val tree = tree(FakeAppHandle(releaseDetails = mapOf("rel-1" to release)))

            assertEquals(
                BrowseId.Track("rel-1", "trk-b").mediaId,
                tree.item(BrowseId.Track("rel-1", "trk-b").mediaId)!!.mediaId,
            )
            assertNull(tree.item(BrowseId.Track("rel-1", "trk-missing").mediaId))
            tree.close()
        }

    /** A search's album count is a live read of its own, and each count it delivers tells the controller. */
    @Test
    fun aSearchTellsTheControllerEachCountOfItsAlbums() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = {
                        BridgeFixtures.searchResults(albums = List(2) { BridgeFixtures.albumSearchResult(id = "album-$it") })
                    },
                )
            val tree = tree(handle)
            val notifications = mutableListOf<Int>()

            tree.subscribeSearch(Any(), "query", notifications::add)
            assertEquals(listOf(2), notifications)

            val list = handle.searchAlbumList("query")
            list.rows = List(5) { BridgeFixtures.albumSearchResult(id = "album-$it") }
            list.emitCount()
            assertEquals(listOf(2, 5), notifications)
            tree.close()
            assertTrue(list.counts.single().cancelled)
        }

    /** A controller's next phrase replaces its search's reads, and a failed first count fails that search. */
    @Test
    fun aControllersNextSearchReplacesItsReads() =
        runBlocking {
            val handle =
                FakeAppHandle(
                    searchResults = { query ->
                        BridgeFixtures.searchResults(
                            albums = if (query == "first") listOf(BridgeFixtures.albumSearchResult(id = "album-0")) else emptyList(),
                        )
                    },
                    initialSearchError = { query -> if (query == "second") queryFailure() else null },
                )
            val tree = tree(handle)
            val owner = Any()
            val notifications = mutableListOf<Int>()
            tree.subscribeSearch(owner, "first", notifications::add)
            tree.search(owner, "first", 0, 10)

            assertTrue(runCatching { tree.subscribeSearch(owner, "second", notifications::add) }.exceptionOrNull() is BridgeException)
            val first = handle.searchAlbumList("first")
            assertTrue("the next phrase ends the last one's count", first.counts.single().cancelled)
            assertTrue("the next phrase ends the last one's pages", first.views.single().cancelled)
            val second = handle.searchAlbumList("second")
            second.countFailure = null
            second.rows = List(2) { BridgeFixtures.albumSearchResult(id = "album-$it") }
            second.emitCount()
            assertEquals(listOf(1, 2), notifications)
            tree.close()
            assertTrue(second.counts.single().cancelled)
        }

    @Test
    fun unknownParentReturnsNull() =
        runBlocking {
            assertNull(tree(FakeAppHandle()).children("unknown", 0, 20))
        }

    private fun queryFailure(): BridgeException = BridgeException.Diagnostic(BridgeErrorCategory.Database, "query failed")

    private fun albums(count: Int) = List(count) { BridgeFixtures.album(id = "album-%04d".format(it)) }
}
