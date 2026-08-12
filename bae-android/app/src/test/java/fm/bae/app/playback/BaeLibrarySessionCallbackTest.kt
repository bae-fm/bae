package fm.bae.app.playback

import android.net.Uri
import android.os.Bundle
import android.os.Looper
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionError
import androidx.media3.session.SessionResult
import fm.bae.app.BridgeFixtures
import fm.bae.app.data.Library
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BaeLibrarySessionCallbackTest {
    @Test
    fun getChildrenWaitsForTheParentObservationBaseline() {
        val handle = FakeAppHandle(deliverAlbumParentObservationImmediately = false)
        val tree = tree(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val request =
            BaeLibrarySessionCallback(tree, scope)
                .getChildren(controller(), BrowseId.Albums.mediaId, page = 0, pageSize = 20, params = null)

        assertFalse(request.isDone)
        assertTrue(handle.albumPageCallbacks.isEmpty())

        handle.emitAlbumParentObservation(0uL)

        assertEquals(SessionResult.RESULT_SUCCESS, request.get(1, TimeUnit.SECONDS).resultCode)
        assertEquals(1, handle.albumPageCallbacks.size)
        tree.close()
        scope.cancel()
    }

    @Test
    fun initialParentObservationErrorAllowsThePageAndLaterValueRetiresIt() {
        val handle =
            FakeAppHandle(
                albumPages = { _, _ -> listOf(BridgeFixtures.album(id = "album-old")) },
                deliverAlbumParentObservationImmediately = false,
            )
        val notifications = mutableListOf<Pair<String, Int>>()
        val tree = tree(handle) { parentId, count -> notifications += parentId to count }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val request =
            BaeLibrarySessionCallback(tree, scope)
                .getChildren(controller(), BrowseId.Albums.mediaId, page = 0, pageSize = 20, params = null)

        assertTrue(handle.albumPageCallbacks.isEmpty())
        handle.failAlbumParentObservation()

        val initial = request.get(1, TimeUnit.SECONDS)
        assertEquals(SessionResult.RESULT_SUCCESS, initial.resultCode)
        assertEquals(BrowseId.Album("album-old").mediaId, initial.value!!.single().mediaId)
        assertTrue(notifications.isEmpty())

        handle.emitAlbumParentObservation(1uL)

        assertEquals(listOf(BrowseId.Albums.mediaId to 1), notifications)
        assertTrue(handle.albumPageSubscriptions.single().cancelled)
        tree.close()
        scope.cancel()
    }

    @Test
    fun concurrentGetChildrenRequestsShareParentReadinessAndPageProjection() {
        val handle = FakeAppHandle(deliverAlbumParentObservationImmediately = false)
        val tree = tree(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val callback = BaeLibrarySessionCallback(tree, scope)
        val browser = controller()

        val first = callback.getChildren(browser, BrowseId.Albums.mediaId, 0, 20, null)
        val second = callback.getChildren(browser, BrowseId.Albums.mediaId, 0, 20, null)

        assertEquals(1, handle.albumParentObservationCallbacks.size)
        assertTrue(handle.albumPageCallbacks.isEmpty())
        handle.emitAlbumParentObservation(0uL)

        assertEquals(SessionResult.RESULT_SUCCESS, first.get(1, TimeUnit.SECONDS).resultCode)
        assertEquals(SessionResult.RESULT_SUCCESS, second.get(1, TimeUnit.SECONDS).resultCode)
        assertEquals(1, handle.albumPageCallbacks.size)
        tree.close()
        scope.cancel()
    }

    @Test
    fun disconnectCompletesAParentReadinessWaitWithoutStartingAPage() {
        val handle = FakeAppHandle(deliverAlbumParentObservationImmediately = false)
        val tree = tree(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val browser = controller()
        val request =
            BaeLibrarySessionCallback(tree, scope)
                .getChildren(browser, BrowseId.Albums.mediaId, page = 0, pageSize = 20, params = null)

        tree.disconnect(browser)

        assertEquals(SessionError.ERROR_UNKNOWN, request.get(1, TimeUnit.SECONDS).resultCode)
        assertTrue(handle.albumPageCallbacks.isEmpty())
        assertTrue(handle.albumParentObservationSubscriptions.single().cancelled)
        tree.close()
        scope.cancel()
    }

    @Test
    fun getChildrenCreatesOneParentObservationWithoutSubscribe() {
        val handle = FakeAppHandle()
        val notifications = mutableListOf<Pair<String, Int>>()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val context = RuntimeEnvironment.getApplication()
        val player =
            BaeCorePlayer(
                applicationLooper = Looper.getMainLooper(),
                appHandle = handle,
                context = context,
                scope = scope,
                isAppForeground = { false },
            )
        lateinit var session: MediaLibrarySession
        val tree =
            LibraryBrowseTree<MediaSession.ControllerInfo>(
                library = Library(handle),
                labels = { BrowseLabels(albums = "Albums", composers = "Composers") },
                artworkUri = { Uri.parse("content://test/cover/${it.id}") },
                onChildrenChanged = { parentId, count ->
                    session.notifyChildrenChanged(parentId, count, null)
                    notifications += parentId to count
                },
            )
        val callback = BaeLibrarySessionCallback(tree, scope)
        session = MediaLibrarySession.Builder(context, player, callback).build()
        val browser = controller()

        repeat(2) {
            val result =
                callback.onGetChildren(
                    session,
                    browser,
                    BrowseId.Albums.mediaId,
                    page = 0,
                    pageSize = 20,
                    params = null,
                )
            result.get(1, TimeUnit.SECONDS)
        }
        handle.emitAlbumParentObservation(1uL)

        assertEquals(listOf(BrowseId.Albums.mediaId to 1), notifications)
        assertEquals(1, handle.albumPageCallbacks.size)
        assertTrue(handle.albumPageSubscriptions.single().cancelled)
        tree.close()
        session.release()
        player.release()
        scope.cancel()
    }

    @Test
    fun initialErrorReturnsMedia3ErrorThenTheLiveRequestRecovers() {
        val handle = FakeAppHandle(initialAlbumPageError = queryFailure())
        val tree =
            LibraryBrowseTree<MediaSession.ControllerInfo>(
                library = Library(handle),
                labels = { BrowseLabels(albums = "Albums", composers = "Composers") },
                artworkUri = { Uri.parse("content://test/cover/${it.id}") },
            )
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val callback =
            BaeLibrarySessionCallback(
                tree,
                scope,
            )
        val browser = controller()

        val failed =
            callback
                .getChildren(browser, BrowseId.Albums.mediaId, page = 0, pageSize = 20, params = null)
                .get(1, TimeUnit.SECONDS)

        assertEquals(SessionError.ERROR_UNKNOWN, failed.resultCode)
        assertTrue(handle.albumPageSubscriptions.all { !it.cancelled })

        handle.emitAlbumPage(
            subscription = 0,
            rows = listOf(BridgeFixtures.album(id = "album-recovered")),
        )
        val recovered =
            callback
                .getChildren(browser, BrowseId.Albums.mediaId, page = 0, pageSize = 20, params = null)
                .get(1, TimeUnit.SECONDS)

        assertEquals(SessionResult.RESULT_SUCCESS, recovered.resultCode)
        assertEquals(
            BrowseId.Album("album-recovered").mediaId,
            recovered.value!!.single().mediaId,
        )
        tree.close()
        scope.cancel()
    }

    private fun queryFailure(): BridgeException =
        BridgeException.Diagnostic(
            category = BridgeErrorCategory.DATABASE,
            detail = "query failed",
        )

    private fun tree(
        handle: FakeAppHandle,
        onChildrenChanged: (String, Int) -> Unit = { _, _ -> },
    ): LibraryBrowseTree<MediaSession.ControllerInfo> =
        LibraryBrowseTree(
            library = Library(handle),
            labels = { BrowseLabels(albums = "Albums", composers = "Composers") },
            artworkUri = { Uri.parse("content://test/cover/${it.id}") },
            onChildrenChanged = onChildrenChanged,
        )

    private fun controller(): MediaSession.ControllerInfo =
        MediaSession.ControllerInfo.createTestOnlyControllerInfo(
            "fm.bae.test",
            1000,
            1,
            1,
            1,
            true,
            Bundle.EMPTY,
        )
}
