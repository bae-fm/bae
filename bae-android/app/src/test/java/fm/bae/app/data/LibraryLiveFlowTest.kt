package fm.bae.app.data

import fm.bae.app.BridgeFixtures
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.bae_bridge.AlbumDetailCallback
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.NoHandle

class LibraryLiveFlowTest {
    @Test
    fun album_detail_error_does_not_end_later_value_delivery() =
        runBlocking {
            val detail = BridgeFixtures.albumDetail(BridgeFixtures.album("album-1"))
            val library =
                Library(
                    object : AppHandle(NoHandle) {
                        override fun subscribeAlbumDetail(
                            albumId: String,
                            callback: AlbumDetailCallback,
                        ): LiveSubscription {
                            callback.onError(
                                BridgeException.Diagnostic(
                                    category = BridgeErrorCategory.DATABASE,
                                    detail = "query failed",
                                ),
                            )
                            callback.onValue(detail)
                            return TestLiveSubscription()
                        }
                    },
                )

            val events = library.albumDetails("album-1").take(2).toList()

            assertTrue(events[0] is LiveQueryEvent.Error)
            assertEquals(detail, (events[1] as LiveQueryEvent.Value).value)
        }

    private class TestLiveSubscription : LiveSubscription(NoHandle) {
        override fun cancel() {}
    }
}
