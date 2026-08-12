package fm.bae.app.playback

import android.net.Uri
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
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class BaeLibrarySessionCallbackTest {
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

        val failed =
            callback
                .getChildren(BrowseId.Albums.mediaId, page = 0, pageSize = 20, params = null)
                .get(1, TimeUnit.SECONDS)

        assertEquals(SessionError.ERROR_UNKNOWN, failed.resultCode)
        assertFalse(handle.liveSubscriptions.single().cancelled)

        handle.emitAlbumPage(
            subscription = 0,
            rows = listOf(BridgeFixtures.album(id = "album-recovered")),
        )
        val recovered =
            callback
                .getChildren(BrowseId.Albums.mediaId, page = 0, pageSize = 20, params = null)
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
}
