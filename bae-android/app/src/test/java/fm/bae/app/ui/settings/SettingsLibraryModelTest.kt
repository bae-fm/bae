package fm.bae.app.ui.settings

import android.os.Looper
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.playback.FakeLiveSubscription
import fm.bae.app.ui.PreviewData
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.LibraryNameCallback
import uniffi.bae_bridge.LiveSubscription
import uniffi.bae_bridge.NoHandle

/**
 * A failed read of this device's person or of the library's name stays a
 * failure Settings shows with Retry, rather than an absent person or name that
 * silently hides Rename, History sharing and the name row.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SettingsLibraryModelTest {
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
    fun aFailedSelfReadIsAFailureAndRetryReadsAgain() {
        val handle = SettingsHandle(people = listOf(Result.failure(offline()), Result.success(listOf(PreviewData.person()))))
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope)

        runBlocking { model.loadSelf() }
        assertEquals(SelfRead.Failed(DisplayError("couldn't reach storage")), model.self)

        runBlocking { model.loadSelf() }
        assertEquals(PreviewData.person().id, (model.self as SelfRead.Loaded).person?.id)
    }

    @Test
    fun aFailedNameSubscriptionIsAFailureAndRetrySubscribesAgain() {
        val handle = SettingsHandle()
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope)

        model.watchName()
        handle.nameCallbacks.last().onError(offline())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(LibraryNameRead.Failed(DisplayError("couldn't reach storage")), model.name)

        model.watchName()
        handle.nameCallbacks.last().onValue("Household")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(LibraryNameRead.Named("Household"), model.name)
        assertEquals(2, handle.nameCallbacks.size)
    }

    @Test
    fun aReplacedNameSubscriptionCannotOverwriteItsReplacement() {
        val handle = SettingsHandle()
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope)
        model.watchName()
        val replaced = handle.nameCallbacks.last()
        model.watchName()

        replaced.onError(offline())
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(LibraryNameRead.Loading, model.name)
    }

    private fun offline() = BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")

    private data object TestErrorDisplays : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) = DisplayError("couldn't reach storage")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null
    }

    private class SettingsHandle(
        private val people: List<Result<List<BridgePerson>>> = emptyList(),
    ) : AppHandle(NoHandle) {
        private var reads = 0
        val nameCallbacks = mutableListOf<LibraryNameCallback>()

        override suspend fun people(): List<BridgePerson> = people[reads++].getOrThrow()

        override fun subscribeLibraryName(callback: LibraryNameCallback): LiveSubscription {
            nameCallbacks += callback
            return FakeLiveSubscription()
        }
    }
}
