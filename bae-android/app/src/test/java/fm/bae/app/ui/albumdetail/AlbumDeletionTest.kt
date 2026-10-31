package fm.bae.app.ui.albumdetail

import android.os.Looper
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.diagnosticDetail
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

/**
 * Deleting an album from its page asks first; Delete deletes that album and
 * leaves the page, Cancel deletes nothing, and a failed delete stays on the
 * page saying why.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AlbumDeletionTest {
    @Test
    fun confirmingDeletesTheAlbumAndLeavesThePage() {
        val handle = DeletingHandle(failure = null)
        var left = 0
        val deletion = deletion(handle) { left++ }

        deletion.ask()
        assertTrue(deletion.confirming)
        deletion.confirm()
        shadowOf(Looper.getMainLooper()).idle()

        assertFalse(deletion.confirming)
        assertEquals(listOf("album-id"), handle.deleted)
        assertEquals(1, left)
        assertNull(deletion.error)
    }

    @Test
    fun dismissingDeletesNothing() {
        val handle = DeletingHandle(failure = null)
        var left = 0
        val deletion = deletion(handle) { left++ }

        deletion.ask()
        deletion.dismiss()
        shadowOf(Looper.getMainLooper()).idle()

        assertFalse(deletion.confirming)
        assertEquals(emptyList<String>(), handle.deleted)
        assertEquals(0, left)
    }

    @Test
    fun aFailedDeleteStaysOnThePageSayingWhy() {
        val handle = DeletingHandle(failure = BridgeException.Diagnostic(BridgeErrorCategory.Internal, "changed after planning"))
        var left = 0
        val deletion = deletion(handle) { left++ }

        deletion.ask()
        deletion.confirm()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(listOf("album-id"), handle.deleted)
        assertEquals(0, left)
        assertEquals(DisplayError("couldn't delete", "changed after planning"), deletion.error)
    }

    private fun deletion(
        handle: DeletingHandle,
        onDeleted: () -> Unit,
    ) = AlbumDeletion(
        albumId = "album-id",
        appHandle = handle,
        errors = TestErrorDisplays,
        scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        onDeleted = onDeleted,
    )

    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) = DisplayError("couldn't delete", error.diagnosticDetail)

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null
    }

    private class DeletingHandle(
        private val failure: BridgeException?,
    ) : FakeAppHandle() {
        val deleted = mutableListOf<String>()

        override suspend fun deleteAlbum(albumId: String) {
            deleted.add(albumId)
            failure?.let { throw it }
        }
    }
}
