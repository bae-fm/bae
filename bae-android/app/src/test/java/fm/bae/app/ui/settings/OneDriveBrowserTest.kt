package fm.bae.app.ui.settings

import android.os.Looper
import fm.bae.app.DisplayError
import fm.bae.app.ErrorDisplays
import fm.bae.app.OAuthProviders
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.bae_bridge.AppHandle
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeHost
import uniffi.bae_bridge.BridgePlaybackErrorReason
import uniffi.bae_bridge.BridgeStorageFolder
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeStorageSetup
import uniffi.bae_bridge.BridgeSyncRetryOutcome
import uniffi.bae_bridge.NoHandle

/**
 * The OneDrive picker moves into a folder only once its listing loads: a
 * failed move keeps the folder shown, with the failure and Retry.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class OneDriveBrowserTest {
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
            val browser = OneDriveBrowser(AppHandle(NoHandle), drive, Errors)
            browser.openRoot()
            val atRoot = checkNotNull(browser.shown)

            browser.open(atRoot, music)

            assertEquals(atRoot, browser.shown)
            val failure = checkNotNull(browser.failure)
            assertEquals(DisplayError("couldn't reach OneDrive"), failure.error)

            failure.retry()

            assertEquals(listOf(root, music), browser.shown?.path)
            assertEquals(listOf(albums), browser.shown?.children)
            assertNull(browser.failure)
        }

    @Test
    fun aFailedFirstListingShowsOnlyTheFailureUntilRetryLoadsIt() =
        runBlocking {
            val drive = Drive(mapOf(root to listOf(offline(), Result.success(listOf(music)))))
            val browser = OneDriveBrowser(AppHandle(NoHandle), drive, Errors)

            browser.openRoot()

            assertNull(browser.shown)
            val failure = checkNotNull(browser.failure)

            failure.retry()

            assertEquals(listOf(root), browser.shown?.path)
            assertEquals(listOf(music), browser.shown?.children)
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
            val browser = OneDriveBrowser(AppHandle(NoHandle), drive, Errors)
            browser.openRoot()
            browser.open(checkNotNull(browser.shown), music)
            val inMusic = checkNotNull(browser.shown)

            browser.up(inMusic)

            assertEquals(inMusic, browser.shown)
            assertNotNull(browser.failure)
        }

    /** The folder exists once made, so Retry lists it again instead of making a second one. */
    @Test
    fun aMadeFolderWhoseListingFailsIsListedAgainOnRetry() =
        runBlocking {
            val drive =
                Drive(
                    mapOf(
                        root to listOf(Result.success(emptyList())),
                        albums to listOf(offline(), Result.success(emptyList())),
                    ),
                    created = albums,
                )
            val browser = OneDriveBrowser(AppHandle(NoHandle), drive, Errors)
            browser.openRoot()
            val atRoot = checkNotNull(browser.shown)

            browser.create(atRoot, "Albums")

            assertEquals(atRoot, browser.shown)
            checkNotNull(browser.failure).retry()

            assertEquals(listOf(root, albums), browser.shown?.path)
            assertEquals(1, drive.creates)
        }

    private fun folder(name: String) = BridgeStorageFolder(name, BridgeStorageLocation.OneDrive("drive", name, null))

    private fun offline(): Result<List<BridgeStorageFolder>> =
        Result.failure(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline"))

    private data object Errors : ErrorDisplays {
        override fun display(reason: BridgePlaybackErrorReason): DisplayError? = null

        override fun display(error: BridgeException) = DisplayError("couldn't reach OneDrive")

        override fun display(outcome: BridgeSyncRetryOutcome): DisplayError? = null
    }

    /** A OneDrive whose folders answer each listing in turn from [listings]. */
    private inner class Drive(
        listings: Map<BridgeStorageFolder, List<Result<List<BridgeStorageFolder>>>>,
        private val created: BridgeStorageFolder? = null,
    ) : OAuthProviders {
        private val answers = listings.mapKeys { it.key.location }.mapValues { it.value.toMutableList() }
        var creates = 0
            private set

        override suspend fun oneDriveRoot(handle: AppHandle) = root

        override suspend fun oneDriveFolders(
            handle: AppHandle,
            parent: BridgeStorageLocation,
        ): List<BridgeStorageFolder> = checkNotNull(answers[parent]).removeAt(0).getOrThrow()

        override suspend fun createOneDriveFolder(
            handle: AppHandle,
            parent: BridgeStorageLocation,
            name: String,
        ): BridgeStorageFolder {
            creates++
            return checkNotNull(created)
        }

        override fun register(host: BridgeHost) = error("not used")

        override suspend fun signIn(
            handle: AppHandle,
            provider: BridgeCloudProvider,
        ) = error("not used")

        override suspend fun signInAgain(
            handle: AppHandle,
            deviceName: String,
        ): BridgeStorageSetup = error("not used")

        override suspend fun dropboxSharedFolders(handle: AppHandle): List<BridgeStorageFolder> = error("not used")

        override suspend fun setUpStorage(
            handle: AppHandle,
            location: BridgeStorageLocation,
            deviceName: String,
        ): BridgeStorageSetup = error("not used")
    }
}
