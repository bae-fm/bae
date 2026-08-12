package fm.bae.app.playback

import android.net.Uri
import fm.bae.app.data.Library
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LibraryBrowseTreeNotificationRaceTest {
    @Test
    fun replacingSearchAfterQueuedActionSuppressesTheOldListener() =
        runBlocking {
            val tree = tree(FakeAppHandle())
            val firstOwner = Any()
            val replacedOwner = Any()
            val queued = CountDownLatch(1)
            val releaseFirst = CountDownLatch(1)
            val replacedNotifications = AtomicInteger()
            val currentNotifications = AtomicInteger()
            val firstSubscription =
                async(Dispatchers.IO) {
                    tree.subscribeSearch(firstOwner, "query") {
                        runBlocking {
                            tree.subscribeSearch(replacedOwner, "query") {
                                replacedNotifications.incrementAndGet()
                            }
                        }
                        queued.countDown()
                        assertTrue(releaseFirst.await(1, TimeUnit.SECONDS))
                    }
                }
            assertTrue(queued.await(1, TimeUnit.SECONDS))

            tree.subscribeSearch(replacedOwner, "replacement") {
                currentNotifications.incrementAndGet()
            }
            releaseFirst.countDown()
            firstSubscription.await()

            assertEquals(0, replacedNotifications.get())
            assertEquals(1, currentNotifications.get())
        }

    @Test
    fun parentDisconnectWaitsForACommittedInvocation() =
        runBlocking {
            val invocationStarted = CountDownLatch(1)
            val releaseInvocation = CountDownLatch(1)
            val handle = FakeAppHandle()
            val notifications = AtomicInteger()
            val disconnectStarted = CountDownLatch(1)
            val disconnectReturned = CountDownLatch(1)
            val tree =
                tree(handle) { _, _ ->
                    notifications.incrementAndGet()
                    invocationStarted.countDown()
                    assertTrue(releaseInvocation.await(1, TimeUnit.SECONDS))
                }
            val owner = Any()
            tree.subscribeParent(owner, BrowseId.Albums.mediaId)
            tree.children(BrowseId.Albums.mediaId, page = 0, pageSize = 20)
            val update = async(Dispatchers.IO) { handle.emitAlbumParentObservation(1uL) }
            assertTrue(invocationStarted.await(1, TimeUnit.SECONDS))

            val disconnect =
                async(Dispatchers.IO) {
                    disconnectStarted.countDown()
                    tree.disconnect(owner)
                    disconnectReturned.countDown()
                }
            assertTrue(disconnectStarted.await(1, TimeUnit.SECONDS))
            assertFalse(disconnectReturned.await(100, TimeUnit.MILLISECONDS))
            releaseInvocation.countDown()
            update.await()
            disconnect.await()

            assertEquals(1, notifications.get())
        }

    @Test
    fun searchReplacementWaitsForACommittedInvocation() =
        runBlocking {
            val tree = tree(FakeAppHandle())
            val owner = Any()
            val oldInvocationStarted = CountDownLatch(1)
            val releaseOldInvocation = CountDownLatch(1)
            val replacementStarted = CountDownLatch(1)
            val replacementReturned = CountDownLatch(1)
            val oldNotifications = AtomicInteger()
            val currentNotifications = AtomicInteger()
            val initial =
                async(Dispatchers.IO) {
                    tree.subscribeSearch(owner, "old") {
                        oldNotifications.incrementAndGet()
                        oldInvocationStarted.countDown()
                        assertTrue(releaseOldInvocation.await(1, TimeUnit.SECONDS))
                    }
                }
            assertTrue(oldInvocationStarted.await(1, TimeUnit.SECONDS))

            val replacement =
                async(Dispatchers.IO) {
                    replacementStarted.countDown()
                    tree.subscribeSearch(owner, "current") {
                        currentNotifications.incrementAndGet()
                    }
                    replacementReturned.countDown()
                }
            assertTrue(replacementStarted.await(1, TimeUnit.SECONDS))
            assertFalse(replacementReturned.await(100, TimeUnit.MILLISECONDS))

            releaseOldInvocation.countDown()
            initial.await()
            replacement.await()
            assertEquals(1, oldNotifications.get())
            assertEquals(1, currentNotifications.get())
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
}
