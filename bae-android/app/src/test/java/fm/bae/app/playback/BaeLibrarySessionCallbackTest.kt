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
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BaeLibrarySessionCallbackTest {
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
            callback
                .onGetChildren(
                    session,
                    browser,
                    BrowseId.Albums.mediaId,
                    page = 0,
                    pageSize = 20,
                    params = null,
                )
                .get(1, TimeUnit.SECONDS)
        }
        handle.emitAlbumPage(
            subscription = 0,
            rows = listOf(BridgeFixtures.album(id = "album-updated")),
        )

        assertEquals(listOf(BrowseId.Albums.mediaId to 1), notifications)
        assertEquals(2, handle.albumPageCallbacks.size)
        assertTrue(handle.albumPageSubscriptions.all { !it.cancelled })
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
        handle.emitAlbumPage(
            subscription = 1,
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
