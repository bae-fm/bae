package fm.bae.app.ui.settings

import android.os.Looper
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OAuthProviders
import fm.bae.app.playback.FakeAppHandle
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AppHandleInterface
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgeLibrarySetup
import uniffi.bae_bridge.BridgePlayOutcome
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeStorageFailure
import uniffi.bae_bridge.BridgeStorageFolder
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.BridgeSyncRetryOutcome

/**
 * The folder choice moves into a folder only once its listing loads: a failed move keeps the
 * folder shown, with the failure and Retry. A made folder joins the list and is chosen, and a
 * folder setup found holding other files is not set up again.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class FolderBrowserTest {
    private val root = folder("OneDrive")
    private val music = folder("Music")
    private val albums = folder("Albums")

    /**
     * The browser's state is Compose state: once any Compose UI has run in
     * the process, each write posts Compose's snapshot apply to the main
     * thread. Robolectric drops whatever is still queued there when a test
     * ends, which leaves Compose's process-wide UI dispatcher waiting on a
     * dispatch that never runs, so no later Compose test in the process goes
     * idle. Run that work as the app's main thread would.
     */
    @After
    fun runTheMainThreadsPendingWork() {
        shadowOf(Looper.getMainLooper()).idle()
    }

    @Test
    fun aFailedOpenKeepsTheFolderShownWithTheFailureAndRetry() =
        runBlocking {
            val drive =
                Drive(
                    mapOf(
                        root to listOf(Result.success(listOf(music))),
                        music to listOf(offline(), Result.success(listOf(albums))),
                    ),
                )
            val browser = browser(drive)
            browser.openTop()
            val atRoot = checkNotNull(browser.shown)

            browser.open(atRoot, music)

            assertEquals(atRoot, browser.shown)
            val failure = checkNotNull(browser.failure)
            assertEquals(DisplayError("couldn't reach OneDrive"), failure.error)

            failure.retry()

            assertEquals(listOf(root, music), browser.shown?.path)
            assertEquals(listOf(albums), browser.shown?.folders)
            assertNull(browser.failure)
        }

    @Test
    fun aFailedFirstListingShowsOnlyTheFailureUntilRetryLoadsIt() =
        runBlocking {
            val drive = Drive(mapOf(root to listOf(offline(), Result.success(listOf(music)))))
            val browser = browser(drive)

            browser.openTop()

            assertNull(browser.shown)
            val failure = checkNotNull(browser.failure)

            failure.retry()

            assertEquals(listOf(root), browser.shown?.path)
            assertEquals(listOf(music), browser.shown?.folders)
        }

    @Test
    fun aFailedUpStaysInTheFolder() =
        runBlocking {
            val drive =
                Drive(
                    mapOf(
                        root to listOf(Result.success(listOf(music)), offline()),
                        music to listOf(Result.success(listOf(albums))),
                    ),
                )
            val browser = browser(drive)
            browser.openTop()
            browser.open(checkNotNull(browser.shown), music)
            val inMusic = checkNotNull(browser.shown)

            browser.up(inMusic)

            assertEquals(inMusic, browser.shown)
            assertNotNull(browser.failure)
        }

    @Test
    fun aMadeFolderJoinsTheListAndIsChosen() =
        runBlocking {
            val drive = Drive(mapOf(root to listOf(Result.success(listOf(music)))), created = albums)
            val browser = browser(drive)
            browser.openTop()
            val atRoot = checkNotNull(browser.shown)

            browser.create(atRoot, "Albums")

            assertEquals(listOf(root), browser.shown?.path)
            assertEquals(listOf(music, albums), browser.shown?.folders)
            assertEquals(albums, browser.target)
            assertEquals(1, drive.creates)
        }

    @Test
    fun aNameAlreadyTakenLeavesTheListAndNamesIt() =
        runBlocking {
            val taken =
                Result.failure<BridgeStorageFolder>(
                    BridgeException.Diagnostic(BridgeErrorCategory.Storage(BridgeStorageFailure.AlreadyExists), "taken"),
                )
            val drive = Drive(mapOf(root to listOf(Result.success(listOf(music)))), createResult = taken)
            val browser = browser(drive)
            browser.openTop()
            val atRoot = checkNotNull(browser.shown)

            browser.create(atRoot, "Music")

            assertEquals("Music", browser.takenName)
            assertNull(browser.failure)
            assertEquals(atRoot, browser.shown)
            assertNull(browser.selected)
        }

    @Test
    fun aFolderSetupFoundHoldingOtherFilesIsNamedAndNotSetUpAgain() =
        runBlocking {
            val drive = Drive(mapOf(root to listOf(Result.success(listOf(music, albums)))))
            val browser = browser(drive)
            browser.openTop()
            browser.select(music)

            assertTrue(browser.place(storage(BridgeStorageFailure.OwnLocationRequired)))
            assertEquals(music, browser.notEmpty)
            assertNull(browser.target)

            browser.select(albums)
            assertEquals(albums, browser.target)
            assertFalse(browser.place(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline")))
        }

    private fun storage(failure: BridgeStorageFailure) = BridgeException.Diagnostic(BridgeErrorCategory.Storage(failure), "")

    private fun browser(drive: Drive) = FolderBrowser(FolderCalls.oneDrive(FakeAppHandle(), drive), Errors, "OneDrive")

    private fun folder(name: String) = BridgeStorageFolder(name, BridgeStorageLocation.OneDrive("drive", name, null, null))

    private fun offline(): Result<List<BridgeStorageFolder>> =
        Result.failure(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline"))

    private data object Errors : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) = DisplayError("couldn't reach OneDrive")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null

        override fun display(outcome: BridgePlayOutcome): DisplayError? = null
    }

    /** A OneDrive whose folders answer each listing in turn from [listings]. */
    private inner class Drive(
        listings: Map<BridgeStorageFolder, List<Result<List<BridgeStorageFolder>>>>,
        private val created: BridgeStorageFolder? = null,
        private val createResult: Result<BridgeStorageFolder>? = null,
    ) : OAuthProviders {
        private val answers = listings.mapKeys { it.key.location }.mapValues { it.value.toMutableList() }
        var creates = 0
            private set

        override suspend fun oneDriveRoot(handle: AppHandleInterface) = root

        override suspend fun oneDriveFolders(
            handle: AppHandleInterface,
            parent: BridgeStorageLocation,
        ): List<BridgeStorageFolder> = checkNotNull(answers[parent]).removeAt(0).getOrThrow()

        override suspend fun createOneDriveFolder(
            handle: AppHandleInterface,
            parent: BridgeStorageLocation,
            name: String,
        ): BridgeStorageFolder {
            creates++
            return (createResult ?: Result.success(checkNotNull(created))).getOrThrow()
        }

        override fun register(host: BridgeHost) = error("not used")

        override suspend fun signIn(
            handle: AppHandleInterface,
            provider: BridgeCloudProvider,
        ) = error("not used")

        override suspend fun signInAgain(
            handle: AppHandleInterface,
            deviceName: String,
            setup: BridgeLibrarySetup,
        ): BridgeStorageSetup = error("not used")

        override suspend fun dropboxSharedFolders(handle: AppHandleInterface): List<BridgeStorageFolder> = error("not used")

        override suspend fun createDropboxSharedFolder(
            handle: AppHandleInterface,
            name: String,
        ): BridgeStorageFolder = error("not used")

        override suspend fun setUpStorage(
            handle: AppHandleInterface,
            location: BridgeStorageLocation,
            deviceName: String,
            setup: BridgeLibrarySetup,
        ): BridgeStorageSetup = error("not used")
    }
}
