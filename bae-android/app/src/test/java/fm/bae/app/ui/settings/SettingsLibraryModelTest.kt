package fm.bae.app.ui.settings

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.data.LibraryWithoutStorage
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.FakeLibraryNameSubscription
import fm.bae.app.ui.PreviewData
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgePerson
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.LibraryNameSubscription

/**
 * A failed read of this device's person or of the library's name stays a
 * failure Settings shows with Retry, rather than an absent person or name that
 * silently hides Rename, History sharing and the name row. A cancelled read
 * shows nothing and leaves the read as it was.
 */
@RunWith(BaeRobolectricTestRunner::class)
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
        val handle = SettingsHandle(ownPerson = listOf(Result.failure(offline()), Result.success(PreviewData.person())))
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope) { _, _ -> }

        runBlocking { model.loadSelf() }
        assertEquals(SelfRead.Failed(DisplayError("couldn't reach the cloud")), model.self)

        runBlocking { model.loadSelf() }
        assertEquals(PreviewData.person().membership.id, (model.self as SelfRead.Loaded).person?.membership?.id)
    }

    @Test
    fun aFailedNameSubscriptionIsAFailureAndRetrySubscribesAgain() {
        val handle = SettingsHandle()
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope) { _, _ -> }

        model.watchName()
        handle.names
            .last()
            .pulls
            .fail(offline())
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(LibraryNameRead.Failed(DisplayError("couldn't reach the cloud")), model.name)

        model.watchName()
        handle.names
            .last()
            .pulls
            .deliver("Household")
        shadowOf(Looper.getMainLooper()).idle()
        assertEquals(LibraryNameRead.Named("Household"), model.name)
        assertEquals(2, handle.names.size)
    }

    @Test
    fun aReplacedNameSubscriptionCannotOverwriteItsReplacement() {
        val handle = SettingsHandle()
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope) { _, _ -> }
        model.watchName()
        val replaced = handle.names.last().pulls
        model.watchName()

        replaced.fail(offline())
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(LibraryNameRead.Loading, model.name)
    }

    @Test
    fun aCancelledSelfReadLeavesTheReadAsItWas() {
        val handle =
            SettingsHandle(ownPerson = listOf(Result.failure(BridgeException.Cancelled()), Result.failure(offline())))
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope) { _, _ -> }

        runBlocking { model.loadSelf() }
        assertEquals(SelfRead.Loading, model.self)

        runBlocking { model.loadSelf() }
        assertEquals(SelfRead.Failed(DisplayError("couldn't reach the cloud")), model.self)
    }

    @Test
    fun aCancelledNameSubscriptionLeavesTheNameAsItWas() {
        val handle = SettingsHandle()
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope) { _, _ -> }
        model.watchName()
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

        assertEquals(LibraryNameRead.Named("Household"), model.name)
    }

    @Test
    fun aFailedStorageReadIsAFailureAndReadingAgainRecovers() {
        val location = BridgeStorageLocation.S3("music", "us-east-1", null, null)
        val handle = SettingsHandle(storage = listOf(Result.failure(offline()), Result.success(location)))
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope) { _, _ -> }

        runBlocking { model.loadStorage() }
        assertEquals(Storage.Failed(DisplayError("couldn't reach the cloud")), model.storage)

        runBlocking { model.loadStorage() }
        assertEquals(Storage.At(location), model.storage)
    }

    @Test
    fun aLibraryWithNoStorageIsReportedAsABug() {
        val bugs = mutableListOf<Pair<String, Throwable>>()
        val handle = SettingsHandle(storage = listOf(Result.success(null)))
        val model = SettingsLibraryModel(handle, TestErrorDisplays, scope) { operation, error -> bugs += operation to error }

        runBlocking { model.loadStorage() }

        assertEquals(Storage.Missing, model.storage)
        assertEquals(LibraryWithoutStorage::class, bugs.single().second::class)
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

    private class SettingsHandle(
        private val ownPerson: List<Result<BridgePerson?>> = emptyList(),
        private val storage: List<Result<BridgeStorageLocation?>> = emptyList(),
    ) : FakeAppHandle() {
        var reads = 0
            private set
        private var storageReads = 0
        val names = mutableListOf<FakeLibraryNameSubscription>()

        override suspend fun storageLocation(): BridgeStorageLocation? = storage[storageReads++].getOrThrow()

        override suspend fun ownPerson(): BridgePerson? = ownPerson[reads++].getOrThrow()

        override fun subscribeLibraryName(): LibraryNameSubscription = FakeLibraryNameSubscription().also(names::add)
    }
}
