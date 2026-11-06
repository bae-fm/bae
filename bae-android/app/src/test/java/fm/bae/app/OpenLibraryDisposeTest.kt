package fm.bae.app

import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeLibraryRemoval

/**
 * The switch path relies on [OpenLibrary.dispose] closing the library — which
 * saves the queue, current track, and position to the old library's DB and
 * closes the DB — before releasing the handle, so switching back later restores
 * the queue. Removal takes the opposite route: core leaves the store and
 * deletes the library, so the handle is released without closing the library —
 * there is nothing left to save into — and it is released even when leaving
 * fails.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class OpenLibraryDisposeTest {
    @Test
    fun disposeClosesTheLibraryBeforeReleasingTheHandle() {
        val (session, handle) = openLibrary()

        runBlocking { session.dispose(null) }

        assertEquals(listOf("closeLibrary", "close"), handle.calls)
    }

    /** A screen still composed could take a tap, so the handle stays open until the last of them is gone. */
    @Test
    fun disposeWaitsForTheLibrarysScreensToGoBeforeClosing() {
        val (session, handle) = openLibrary()
        session.screenShown()

        val disposing = CoroutineScope(Dispatchers.Unconfined).launch { session.dispose(null) }
        assertEquals(emptyList<String>(), handle.calls)

        session.screenGone()
        runBlocking { disposing.join() }
        assertEquals(listOf("closeLibrary", "close"), handle.calls)
    }

    @Test
    fun disposeReleasesTheHandleWhenClosingTheLibraryFails() {
        val (session, handle) = openLibrary(failClose = true)

        assertThrows(IllegalStateException::class.java) {
            runBlocking { session.dispose(null) }
        }

        assertEquals(listOf("closeLibrary", "close"), handle.calls)
    }

    @Test
    fun removalDiscardsUnsentWorkThenReleasesTheHandle() {
        val (session, handle) = openLibrary()

        val removal = runBlocking { session.removeFromDevice(null) }

        assertEquals(BridgeLibraryRemoval.Removed, removal)
        assertEquals(listOf("removeLibrary", "close"), handle.calls)
    }

    @Test
    fun aFailedRemovalStillClosesTheHandle() {
        val (session, handle) = openLibrary(failRemoval = true)

        assertThrows(BridgeException.Diagnostic::class.java) {
            runBlocking { session.removeFromDevice(null) }
        }

        assertEquals(listOf("removeLibrary", "close"), handle.calls)
    }

    private fun openLibrary(
        failClose: Boolean = false,
        failRemoval: Boolean = false,
    ): Pair<OpenLibrary, RecordingHandle> {
        val handle = RecordingHandle(failClose, failRemoval)
        return openTestLibrary(handle) to handle
    }

    private class RecordingHandle(
        private val failClose: Boolean,
        private val failRemoval: Boolean,
    ) : FakeAppHandle() {
        val calls = mutableListOf<String>()

        override fun closeLibrary(closing: BridgeLibraryClosing?) {
            calls.add("closeLibrary")
            if (failClose) {
                throw IllegalStateException("closing the library failed")
            }
        }

        override fun close() {
            calls.add("close")
        }

        override suspend fun removeLibrary(closing: BridgeLibraryClosing?): BridgeLibraryRemoval {
            calls.add("removeLibrary")
            if (failRemoval) {
                throw BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")
            }
            return BridgeLibraryRemoval.Removed
        }
    }
}
