package fm.bae.app

import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryClosing

/** A switch to a library that fails to open lands back on the library still open. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class FailedSwitchTest {
    @Test
    fun aFailedSwitchLandsOnTheLibraryStillOpen() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val open = openTestLibrary(handle)
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

        assertEquals(
            AppScreen.LibraryOpen(open),
            AppScreen.openFailed("other-library", failure, DisplayError("Offline.", "offline"), offersReset = false, open),
        )
        runBlocking { open.dispose(null) }
    }
}
