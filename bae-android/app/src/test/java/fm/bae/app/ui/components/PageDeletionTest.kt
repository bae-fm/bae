package fm.bae.app.ui.components

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.diagnosticDetail
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome

/**
 * Deleting what a page shows asks first, naming what the page asked about;
 * Delete deletes it and leaves the page, Cancel deletes nothing, and a failed
 * delete stays on the page saying why.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class PageDeletionTest {
    @Test
    fun confirmingDeletesAndLeavesThePage() {
        val call = Deleting(failure = null)
        var left = 0
        val deletion = deletion(call) { left++ }

        deletion.ask("Late night")
        assertEquals("Late night", deletion.confirming)
        deletion.confirm()
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(deletion.confirming)
        assertEquals(1, call.deletes)
        assertEquals(1, left)
        assertNull(deletion.error)
    }

    @Test
    fun dismissingDeletesNothing() {
        val call = Deleting(failure = null)
        var left = 0
        val deletion = deletion(call) { left++ }

        deletion.ask("Late night")
        deletion.dismiss()
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(deletion.confirming)
        assertEquals(0, call.deletes)
        assertEquals(0, left)
    }

    @Test
    fun aFailedDeleteStaysOnThePageSayingWhy() {
        val call = Deleting(failure = BridgeException.Diagnostic(BridgeErrorCategory.Internal, "changed after planning"))
        var left = 0
        val deletion = deletion(call) { left++ }

        deletion.ask("Late night")
        deletion.confirm()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(1, call.deletes)
        assertEquals(0, left)
        assertEquals(DisplayError("couldn't delete", "changed after planning"), deletion.error)
    }

    private fun deletion(
        call: Deleting,
        onDeleted: () -> Unit,
    ) = PageDeletion<String>(
        operation = "delete playlist playlist-id",
        errors = TestErrorDisplays,
        scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
        delete = call::delete,
        onDeleted = onDeleted,
    )

    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) = DisplayError("couldn't delete", error.diagnosticDetail)

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }

    /** A delete call that counts its runs and throws [failure] when one is given. */
    private class Deleting(
        private val failure: BridgeException?,
    ) {
        var deletes = 0

        suspend fun delete() {
            deletes++
            failure?.let { throw it }
        }
    }
}
