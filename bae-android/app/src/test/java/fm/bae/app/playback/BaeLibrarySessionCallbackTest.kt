package fm.bae.app.playback

import android.net.Uri
import android.os.Bundle
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
import org.robolectric.annotation.Config
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BaeLibrarySessionCallbackTest {
    @Test
    fun legacyGetChildrenEstablishesInterestBeforeRequestingAWindow() {
        val handle = FakeAppHandle(deliverAlbumBaselineImmediately = false)
        val tree = tree(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val request = BaeLibrarySessionCallback(tree, scope).getChildren(controller(), BrowseId.Albums.mediaId, 0, 20, null)

        assertFalse(request.isDone)
        assertTrue(handle.albumPageWindows.isEmpty())
        handle.albumBrowseSubscriptions.single().emitCount(0uL)

        assertEquals(SessionResult.RESULT_SUCCESS, request.get(1, TimeUnit.SECONDS).resultCode)
        assertEquals(listOf(0uL to 20uL), handle.albumPageWindows)
        tree.close()
        scope.cancel()
    }

    @Test
    fun repeatedGetChildrenCoalescesOneParentCollection() {
        val handle = FakeAppHandle()
        val tree = tree(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val callback = BaeLibrarySessionCallback(tree, scope)
        val browser = controller()

        repeat(2) {
            assertEquals(
                SessionResult.RESULT_SUCCESS,
                callback
                    .getChildren(browser, BrowseId.Albums.mediaId, 0, 20, null)
                    .get(1, TimeUnit.SECONDS)
                    .resultCode,
            )
        }

        assertEquals(1, handle.albumBrowseSubscriptions.size)
        tree.close()
        scope.cancel()
    }

    @Test
    fun collectionErrorReturnsAnExplicitMedia3Error() {
        val handle = FakeAppHandle(initialAlbumPageError = queryFailure())
        val tree = tree(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)

        val result =
            BaeLibrarySessionCallback(tree, scope)
                .getChildren(controller(), BrowseId.Albums.mediaId, 0, 20, null)
                .get(1, TimeUnit.SECONDS)

        assertEquals(SessionError.ERROR_UNKNOWN, result.resultCode)
        tree.close()
        scope.cancel()
    }

    @Test
    fun databaseSnapshotNotifiesAndTheFollowingReadUsesIt() {
        val handle = FakeAppHandle(albumPages = { _, _ -> listOf(BridgeFixtures.album(id = "album-old")) })
        val notifications = mutableListOf<Pair<String, Int>>()
        val tree = tree(handle) { parent, count -> notifications += parent to count }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
        val callback = BaeLibrarySessionCallback(tree, scope)
        val browser = controller()
        callback.getChildren(browser, BrowseId.Albums.mediaId, 0, 20, null).get(1, TimeUnit.SECONDS)

        handle.albumBrowseSubscriptions.single().emitRows(
            listOf(BridgeFixtures.album(id = "album-new")),
            1uL,
        )
        val result = callback.getChildren(browser, BrowseId.Albums.mediaId, 0, 20, null).get(1, TimeUnit.SECONDS)

        assertEquals(listOf(BrowseId.Albums.mediaId to 1), notifications)
        assertEquals(BrowseId.Album("album-new").mediaId, result.value!!.single().mediaId)
        tree.close()
        scope.cancel()
    }

    private fun tree(
        handle: FakeAppHandle,
        changed: (String, Int) -> Unit = { _, _ -> },
    ): LibraryBrowseTree<MediaSession.ControllerInfo> =
        LibraryBrowseTree(
            Library(handle),
            { BrowseLabels("Albums", "Composers") },
            { Uri.parse("content://test/cover/${it.id}") },
            onChildrenChanged = changed,
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

    private fun queryFailure() =
        uniffi.bae_bridge.BridgeException.Diagnostic(
            uniffi.bae_bridge.BridgeErrorCategory.Database,
            "query failed",
        )
}
