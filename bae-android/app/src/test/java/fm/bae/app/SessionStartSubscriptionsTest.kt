package fm.bae.app

import android.os.Looper
import fm.bae.app.data.LiveValue
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryClosing

/** An open library's stores start with its subscriptions' first values, and the session pulls the same ones. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class SessionStartSubscriptionsTest {
    @Test
    fun theSessionPullsOnFromTheFirstValues() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val first = BridgeFixtures.downloadCounts(queued = 1u)
        handle.config.deliver(BridgeFixtures.config())
        handle.downloadCounts.pulls.deliver(first)
        handle.unmeteredOnly.pulls.deliver(true)

        val started = runBlocking { SessionStartSubscriptions.start(handle) }

        assertEquals(BridgeFixtures.config(), started.config.first)
        assertEquals(first, started.downloadCounts.first)
        assertEquals(true, started.unmeteredOnly.first)

        val session = openTestLibrary(handle)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
        val next = BridgeFixtures.downloadCounts(queued = 2u)
        handle.downloadCounts.pulls.deliver(next)
        session.wireUp(scope, started)
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(LiveValue.Current(next), session.downloadStore.state.value)

        session.disposeOnMain(scope)
        scope.cancel()
    }

    @Test
    fun aFirstReadThatFailsFailsTheStart() {
        val handle = FakeAppHandle()
        val failure = BridgeException.Diagnostic(BridgeErrorCategory.Database, "disk I/O error")
        handle.config.deliver(BridgeFixtures.config())
        handle.downloadCounts.pulls.fail(failure)

        val thrown =
            assertThrows(BridgeException::class.java) {
                runBlocking { SessionStartSubscriptions.start(handle) }
            }

        assertEquals(failure, thrown)
    }
}
