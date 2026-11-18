package fm.bae.app.ui.albumdetail

import fm.bae.app.DisplayError
import fm.bae.app.data.LiveQueryState
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.bae_bridge.BridgeAlbumDetail
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException

/**
 * An album the read answers with nothing was deleted, so the page says it is
 * gone rather than offering a Retry that reads the same nothing again; only a
 * failed read offers Retry.
 */
class AlbumDetailPlaceholderTest {
    @Test
    fun anAnsweredReadWithNoAlbumIsNotInTheLibrary() {
        val query = LiveQueryState.Answered<BridgeAlbumDetail?>(null)

        assertEquals(AlbumDetailPlaceholder.NotInLibrary, albumDetailPlaceholder(query, loadError = null))
    }

    @Test
    fun aFailedReadOffersRetry() {
        val error = DisplayError("Couldn't read the database.")
        val query = LiveQueryState.Failed(BridgeException.Diagnostic(BridgeErrorCategory.Database, "read failed"))

        assertEquals(AlbumDetailPlaceholder.Failed(error), albumDetailPlaceholder(query, error))
    }

    @Test
    fun aReadNotYetAnsweredIsLoading() {
        assertEquals(
            AlbumDetailPlaceholder.Loading,
            albumDetailPlaceholder(LiveQueryState.Waiting, loadError = null),
        )
    }
}
