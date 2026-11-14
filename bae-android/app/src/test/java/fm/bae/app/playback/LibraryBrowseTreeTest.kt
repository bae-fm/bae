package fm.bae.app.playback

import android.net.Uri
import androidx.media3.common.MediaItem
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.HANG_GUARD
import fm.bae.app.HANG_GUARD_MS
import fm.bae.app.MissingMainRelease
import fm.bae.app.data.Library
import fm.bae.app.plainCreditText
import fm.bae.app.ui.PreviewData
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
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
import uniffi.bae_bridge.BridgeListWindow
import uniffi.bae_bridge.BridgeReleaseRoleSummary
import uniffi.bae_bridge.BridgeTermLabel
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class LibraryBrowseTreeTest {
    private val labels = BrowseLabels(library = "Library", albums = "Albums", composers = "Composers")

    private val bugs = mutableListOf<Pair<String, Throwable>>()

    private fun tree(
        handle: FakeAppHandle,
        onChildrenChanged: (String, Int) -> Unit = { _, _ -> },
    ): LibraryBrowseTree<Any> =
        LibraryBrowseTree(
            library = Library(handle),
            labels = { labels },
            nodes = BrowseNodeFactory({ Uri.parse("content://test/cover/${it.id}") }, ::plainCreditText),
            onChildrenChanged = onChildrenChanged,
            reportBug = { operation, error -> bugs += operation to error },
        )

    /** An album's children are its main release's tracks, never another release's. */
    @Test
    fun anAlbumsChildrenAreItsMainReleasesTracks() =
        runBlocking {
            val album = BridgeFixtures.album(id = "album-1", primaryReleaseId = "rel-main", releaseIds = listOf("rel-other", "rel-main"))
            val other =
                PreviewData.release(
                    id = "rel-other",
                    albumId = "album-1",
                    trackGroups = listOf(PreviewData.trackGroup(listOf(PreviewData.track("trk-other")))),
                )
            val main =
                PreviewData.release(
                    id = "rel-main",
                    albumId = "album-1",
                    trackGroups = listOf(PreviewData.trackGroup(listOf(PreviewData.track("trk-main")))),
                )
            val tree = tree(FakeAppHandle(albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(album, listOf(other, main)))))

            val children = tree.children(BrowseId.Album("album-1").mediaId, 0, 20)!!

            assertEquals(listOf(BrowseId.Track("rel-main", "trk-main").mediaId), children.map { it.mediaId })
            tree.close()
        }

    /** An album naming a main release it doesn't hold is a bug: reported, and an error, not the first release. */
    @Test
    fun anAlbumWhoseMainReleaseIsMissingIsReportedAndAnError() =
        runBlocking {
            val album = BridgeFixtures.album(id = "album-1", primaryReleaseId = "rel-gone", releaseIds = listOf("rel-other"))
            val other = PreviewData.release(id = "rel-other", albumId = "album-1")
            val tree = tree(FakeAppHandle(albumDetails = mapOf("album-1" to BridgeFixtures.albumDetail(album, listOf(other)))))

            assertThrows(BridgeException::class.java) {
                runBlocking { tree.children(BrowseId.Album("album-1").mediaId, 0, 20) }
            }
            assertTrue(bugs.single().second is MissingMainRelease)
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

    /** A page is named by offset: the key of the album there is read, and the page is a window from that key. */
    @Test
    fun aPageIsAWindowFromTheKeyAtItsOffset() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(100)))
            val children = tree(handle).children(BrowseId.Albums.mediaId, 2, 20)!!

            assertEquals(List(20) { BrowseId.Album("album-%04d".format(40 + it)).mediaId }, children.map { it.mediaId })
            assertEquals(
                listOf(BridgeListWindow.Forward(handle.albumList.key(40), null, 20uL)),
                handle.albumList.windows
                    .single()
                    .requested,
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
                BridgeListWindow.Forward(handle.albumList.key(800), null, 800uL),
                handle.albumList.windows
                    .single()
                    .window,
            )
        }

    /** A page past the list's end is empty, and reads no window. */
    @Test
    fun aPagePastTheEndIsEmpty() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(30)))

            assertEquals(emptyList<MediaItem>(), tree(handle).children(BrowseId.Albums.mediaId, 2, 20))
            assertTrue(handle.albumList.windows.isEmpty())
        }

    /** A request that names no page is answered as a bad value, not read as page 0 or one item. */
    @Test
    fun aRequestNamingNoPageIsRefused() =
        runBlocking {
            val handle = FakeAppHandle()
            val tree = tree(handle)

            assertNull(tree.children(BrowseId.Albums.mediaId, -1, 20))
            assertNull(tree.children(BrowseId.Albums.mediaId, 0, 0))
            assertNull(tree.search("query", 0, 0))
            assertTrue(handle.albumList.windows.isEmpty())
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

    /** A write that moves the count drops the pages read before it, and tells the car once. */
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
    fun parentInterestWithNoRequestedWindowNotifiesOnACountChange() =
        runBlocking {
            val handle = FakeAppHandle()
            val notifications = mutableListOf<Pair<String, Int>>()
            val tree = tree(handle) { parent, count -> notifications += parent to count }

            tree.subscribeParent(Any(), BrowseId.Albums.mediaId)
            assertTrue(handle.albumList.windows.isEmpty())
            assertTrue(notifications.isEmpty())

            handle.albumList.albums = albums(7)
            handle.albumList.emitCount()

            assertEquals(listOf(BrowseId.Albums.mediaId to 7), notifications)
            tree.close()
        }

    @Test
    fun liveWindowsStayBounded() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(400)))
            val tree = tree(handle)

            repeat(13) { tree.children(BrowseId.Albums.mediaId, it, 20) }

            assertEquals(1, handle.albumList.counts.size)
            assertEquals(13, handle.albumList.windows.size)
            assertEquals(12, handle.albumList.windows.count { !it.cancelled })
            assertTrue(
                handle.albumList.windows
                    .first()
                    .cancelled,
            )
            tree.close()
        }

    @Test
    fun aPageRequestWaitsForTheWindowThatHoldsIt() =
        runBlocking {
            val handle = FakeAppHandle(albumList = FakeAlbumList(albums(40), deliverImmediately = false))
            val tree = tree(handle)
            val request = async { tree.children(BrowseId.Albums.mediaId, 1, 20) }
            withTimeout(HANG_GUARD) { handle.albumList.firstCount.await() }
            assertTrue(handle.albumList.windows.isEmpty())
            handle.albumList.emitCount()
            val window = withTimeout(HANG_GUARD) { handle.albumList.firstWindow.await() }
            assertFalse(request.isCompleted)

            window.deliver()

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
            withTimeout(HANG_GUARD) { handle.albumList.firstWindow.await() }

            tree.close()

            assertTrue(
                handle.albumList.windows
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
                tree.search("query", 0, 20)!!.single().mediaId,
            )
        }

    @Test
    fun aControllersNextSearchMovesItsOneQueryRejectingOldDeliveriesAndItsKnownCount() =
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
            assertEquals("a controller keeps one search across its phrases", 1, handle.searchSubscriptions.size)
            handle.failSearchResults(0, queryFailure())
            handle.emitSearchResults(
                0,
                BridgeFixtures.searchResults(
                    albums = List(9) { BridgeFixtures.albumSearchResult(id = "stale-$it") },
                ),
                revision = 1uL,
            )
            assertEquals(listOf(2), notifications)

            handle.emitSearchResults(
                0,
                BridgeFixtures.searchResults(albums = listOf(BridgeFixtures.albumSearchResult(id = "recovered"))),
            )
            assertEquals(listOf(2, 1), notifications)
            assertEquals(
                BrowseId.Album("recovered").mediaId,
                tree.search("second", 0, 20)!!.single().mediaId,
            )
            tree.close()
            assertTrue(handle.searchSubscriptions.single().closed)
        }

    @Test
    fun unknownParentReturnsNull() =
        runBlocking {
            assertNull(tree(FakeAppHandle()).children("unknown", 0, 20))
        }

    private fun queryFailure(): BridgeException = BridgeException.Diagnostic(BridgeErrorCategory.Database, "query failed")

    private fun albums(count: Int) = List(count) { BridgeFixtures.album(id = "album-%04d".format(it)) }
}
