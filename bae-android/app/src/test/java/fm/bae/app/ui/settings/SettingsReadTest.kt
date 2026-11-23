package fm.bae.app.ui.settings

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.FakeLibraryNameSubscription
import fm.bae.screensubscription.pulled
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.LibraryNameSubscription

/**
 * A failed read of the library's name stays a failure Settings shows with
 * Retry, rather than an absent name that silently hides the name row. A cancelled read
 * shows nothing and leaves the read as it was.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class SettingsReadTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /**
     * The model's writes post Compose's snapshot apply to the main looper, and
     * Robolectric drops whatever is still queued there when the test ends. That
     * leaves Compose's JVM-wide main dispatcher waiting on a message that never
     * runs, so every later Compose test in the JVM never goes idle.
     */
    @After
    fun runWhatTheTestLeftOnTheMainLooper() {
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test
    fun aFailedNameSubscriptionIsAFailureAndRetrySubscribesAgain() {
        val handle = SettingsHandle()
        val read = libraryNameRead(handle)

        read.watch()
        handle.names
            .last()
            .pulls
            .fail(offline())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(SettingsReadState.Failed(DisplayError("couldn't reach the cloud")), read.state)

        read.watch()
        handle.names
            .last()
            .pulls
            .deliver("Household")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(SettingsReadState.Loaded("Household"), read.state)
        assertEquals(2, handle.names.size)
    }

    @Test
    fun aReplacedNameSubscriptionCannotOverwriteItsReplacement() {
        val handle = SettingsHandle()
        val read = libraryNameRead(handle)
        read.watch()
        val replaced = handle.names.last().pulls
        read.watch()

        replaced.fail(offline())
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(SettingsReadState.Loading, read.state)
    }

    @Test
    fun aCancelledNameSubscriptionLeavesTheNameAsItWas() {
        val handle = SettingsHandle()
        val read = libraryNameRead(handle)
        read.watch()
        handle.names
            .last()
            .pulls
            .deliver("Household")
        shadowOf(Looper.getMainLooper()).idle()

        handle.names
            .last()
            .pulls
            .fail(BridgeException.Cancelled())
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(SettingsReadState.Loaded("Household"), read.state)
    }

    private fun libraryNameRead(handle: SettingsHandle) =
        SettingsRead("the library's name", TestErrorDisplays, scope) {
            handle.subscribeLibraryName().let { pulled(it::next, it::cancel) }
        }

    private fun offline() = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

    /** Core's answer for what each failure shows: a cancellation shows nothing. */
    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) =
            if (error is BridgeException.Cancelled) null else DisplayError("couldn't reach the cloud")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }

    private class SettingsHandle : FakeAppHandle() {
        val names = mutableListOf<FakeLibraryNameSubscription>()

        override fun subscribeLibraryName(): LibraryNameSubscription = FakeLibraryNameSubscription().also(names::add)
    }
}
