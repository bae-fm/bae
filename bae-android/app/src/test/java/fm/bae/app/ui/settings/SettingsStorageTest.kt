package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.core.app.ApplicationProvider
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BridgeFixtures
import fm.bae.app.DisplayError
import fm.bae.app.LocalBugReports
import fm.bae.app.data.LiveValue
import fm.bae.app.formatFileSize
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.playback.FakeDeviceStorageSubscription
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.components.AndroidDialog
import fm.bae.app.ui.library.LibraryDestination
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCacheLimitChoice
import uniffi.bae_bridge.BridgeDeviceStorage
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.DeviceStorageSubscription

/**
 * The Storage row of Settings says the total the phone keeps once core has measured it. The Storage page shows
 * "From before the reset" only while a reset's data is here and turns Clear cache off while the cache is empty; its
 * rows ask for the changes they name.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class SettingsStorageTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = ApplicationProvider.getApplicationContext<Application>()

    @Test
    fun theSummaryIsNoneBeforeTheFirstMeasurement() {
        assertNull(storageSummary(null, context))
    }

    @Test
    fun theSummaryIsNoneWhileTheLastReadFailed() {
        assertNull(storageSummary(LiveValue.Failed(DisplayError("couldn't measure")), context))
    }

    @Test
    fun theSummaryIsTheTotalOnThisPhone() {
        assertEquals("${size(3_860_000_000)} on this phone", storageSummary(LiveValue.Current(BridgeFixtures.deviceStorage()), context))
    }

    @Test
    fun theTotalCountsWhatAResetKept() {
        val storage = BridgeFixtures.deviceStorage(beforeReset = 540_000_000uL)

        assertEquals("${size(4_400_000_000)} on this phone", storageSummary(LiveValue.Current(storage), context))
    }

    @Test
    fun theSettingsListShowsTheStorageRowUnderTransfersAndOpensIt() {
        val pushed = mutableListOf<LibraryDestination>()
        compose.setContent {
            BaeTheme {
                AndroidSettingsList(
                    summaries = summaries(storage = "3.9 GB on this phone"),
                    onBack = {},
                    onOpen = {},
                    onPush = pushed::add,
                )
            }
        }

        compose.onNodeWithText("3.9 GB on this phone").assertExists()
        compose.onNodeWithText("Storage").performClick()

        assertEquals(listOf<LibraryDestination>(LibraryDestination.Storage), pushed)
    }

    @Test
    fun theStorageRowHasNoSecondLineBeforeTheFirstMeasurement() {
        compose.setContent {
            BaeTheme { AndroidSettingsList(summaries = summaries(storage = null), onBack = {}, onOpen = {}, onPush = {}) }
        }

        compose.onNodeWithText("Storage").assertExists()
        compose.onNodeWithText("on this phone", substring = true).assertDoesNotExist()
    }

    @Test
    fun fromBeforeTheResetShowsOnlyWhileAResetKeptData() {
        var storage: BridgeDeviceStorage by mutableStateOf(BridgeFixtures.deviceStorage())
        compose.setContent { BaeTheme { Page(storage) } }

        compose.onNodeWithText("Delete data from before the reset").assertDoesNotExist()

        storage = BridgeFixtures.deviceStorage(beforeReset = 540_000_000uL)
        compose.waitForIdle()
        compose.onNodeWithText("Delete data from before the reset").assertExists()
        compose.onNodeWithText("${size(540_000_000)} · nothing uses it").assertExists()
    }

    @Test
    fun clearCacheIsOffWhileTheCacheIsEmpty() {
        val empty = BridgeFixtures.deviceStorage().copy(cache = 0uL, canClearCache = false)
        var cleared = 0
        compose.setContent { BaeTheme { Page(empty, onClearCache = { cleared++ }) } }

        compose.onNodeWithText("Clear cache").assertIsNotEnabled()
        compose.onNodeWithText("Clear cache").performScrollTo().performClick()

        assertEquals(0, cleared)
    }

    @Test
    fun clearCacheNamesTheCacheAndAsksToClearIt() {
        var cleared = 0
        compose.setContent { BaeTheme { Page(BridgeFixtures.deviceStorage(), onClearCache = { cleared++ }) } }

        // The row names the cache's size, as the legend line of the bar does.
        compose.onAllNodesWithText(size(1_200_000_000)).assertCountEquals(2)
        compose.onNodeWithText("Clear cache").performScrollTo().performClick()

        assertEquals(1, cleared)
    }

    @Test
    fun keepUpToNamesTheLimit() {
        compose.setContent { BaeTheme { Page(BridgeFixtures.deviceStorage()) } }

        compose.onNodeWithText("Keep up to").assertExists()
        compose.onNodeWithText(size(20_000_000_000)).assertExists()
    }

    @Test
    fun theSwitchAsksForTheOppositeOfTheSetting() {
        val asked = mutableListOf<Boolean>()
        compose.setContent {
            BaeTheme { Page(BridgeFixtures.deviceStorage(), asksBeforeUnpinning = true, onAskBeforeUnpinning = asked::add) }
        }

        compose.onNode(isToggleable()).performScrollTo().performClick()

        assertEquals(listOf(false), asked)
    }

    @Test
    fun theKeepUpToDialogOffersEveryLimitAndChoosesOne() {
        val storage = BridgeFixtures.deviceStorage()
        val chosen = mutableListOf<BridgeCacheLimitChoice>()
        compose.setContent {
            BaeTheme {
                AndroidDialog(onDismissRequest = {}, parts = keepUpToParts(storage.cacheLimits, storage.cacheLimit.choice, chosen::add, {}))
            }
        }

        for (gigabytes in listOf(1L, 5L, 10L, 20L, 50L)) {
            compose.onNode(inDialog(size(gigabytes * 1_000_000_000))).assertExists()
        }
        compose.onNode(inDialog(size(5_000_000_000))).performClick()

        assertEquals(listOf(BridgeCacheLimitChoice.GB5), chosen)
    }

    @Test
    fun theDeleteDialogNamesTheBytesAndAnswersCancelAndDelete() {
        var deleted = 0
        var dismissed = 0
        compose.setContent {
            BaeTheme {
                AndroidDialog(
                    onDismissRequest = {},
                    parts = deleteBeforeResetParts(540_000_000uL, onConfirm = { deleted++ }, onDismiss = { dismissed++ }),
                )
            }
        }

        compose.onNode(inDialog("Delete data from before the reset?")).assertExists()
        compose
            .onNode(
                inDialog("540 MB will be removed from this phone. It can’t be recovered. Your library isn’t affected."),
            ).assertExists()
        compose.onNode(inDialog("Cancel")).performClick()
        assertEquals(0 to 1, deleted to dismissed)

        compose.onNode(inDialog("Delete")).performClick()
        assertEquals(1 to 1, deleted to dismissed)
    }

    @Test
    fun theScreenFollowsCoresMeasurementAndRunsEachRowsCommand() {
        val handle = StorageHandle()
        val session = openTestLibrary(handle)
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) { StorageScreen(session = session, onBack = {}) }
            }
        }

        compose.onNodeWithText("Clear cache").assertDoesNotExist()
        handle.subscription.deliver(BridgeFixtures.deviceStorage(beforeReset = 540_000_000uL))
        compose.waitForIdle()

        compose.onNodeWithText("Clear cache").performScrollTo().performClick()
        compose.onNode(isToggleable()).performScrollTo().performClick()
        compose.onNodeWithText("Keep up to").performScrollTo().performClick()
        compose.onNode(inDialog(size(5_000_000_000))).performClick()
        compose.onNodeWithText("Delete data from before the reset").performScrollTo().performClick()
        compose.onNode(inDialog("Delete")).performClick()
        compose.waitForIdle()

        assertEquals(
            listOf("clearCache", "setAskBeforeUnpinning(false)", "setCacheLimit(GB5)", "deleteDataFromBeforeReset"),
            handle.calls,
        )
        compose.onNode(inDialog("Delete data from before the reset?")).assertDoesNotExist()
    }

    @Test
    fun aFailedReadShowsItsErrorInPlaceOfThePage() {
        val handle = StorageHandle()
        val session = openTestLibrary(handle)
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) { StorageScreen(session = session, onBack = {}) }
            }
        }

        handle.subscription.fail(BridgeException.Diagnostic(BridgeErrorCategory.Network, "offline"))
        compose.waitForIdle()

        compose.onNodeWithText("failed").assertExists()
        compose.onNodeWithText("Clear cache").assertDoesNotExist()
    }

    private fun size(bytes: Long) = context.formatFileSize(bytes)

    private fun inDialog(text: String) = hasText(text) and hasAnyAncestor(isDialog())

    @Composable
    private fun Page(
        storage: BridgeDeviceStorage,
        asksBeforeUnpinning: Boolean = true,
        onClearCache: () -> Unit = {},
        onAskBeforeUnpinning: (Boolean) -> Unit = {},
    ) = AndroidSettingsStorage(
        storage = storage,
        asksBeforeUnpinning = asksBeforeUnpinning,
        onBack = {},
        onClearCache = onClearCache,
        onKeepUpTo = {},
        onAskBeforeUnpinning = onAskBeforeUnpinning,
        onDeleteBeforeReset = {},
    )

    private fun summaries(storage: String?) =
        SettingsSummaries(
            appearance = "System · Midnight · Pink",
            library = "Home music · Synced",
            playback = "Off",
            transfers = "3 pins at once",
            storage = storage,
            casting = "Off",
            about = "Version 0.0-dev",
        )

    /** A handle that records the storage commands in the order they arrive. */
    private class StorageHandle : FakeAppHandle() {
        val subscription = FakeDeviceStorageSubscription()
        val calls = mutableListOf<String>()

        override fun subscribeDeviceStorage(): DeviceStorageSubscription = subscription

        override suspend fun clearCache() {
            calls += "clearCache"
        }

        override suspend fun setCacheLimit(choice: BridgeCacheLimitChoice) {
            calls += "setCacheLimit(${choice.name})"
        }

        override suspend fun deleteDataFromBeforeReset() {
            calls += "deleteDataFromBeforeReset"
        }

        override suspend fun setAskBeforeUnpinning(enabled: Boolean) {
            calls += "setAskBeforeUnpinning($enabled)"
        }

        override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

        override fun close() = Unit
    }
}
