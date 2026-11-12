package fm.bae.app.ui.settings

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performScrollToNode
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.LocalBugReports
import fm.bae.app.NoErrorDisplays
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.SessionActions
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeCloudProvider
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgePendingAction
import uniffi.bae_bridge.BridgePendingReason
import uniffi.bae_bridge.BridgePendingRecord
import uniffi.bae_bridge.BridgeRestoreCodeShare
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone

/** The Sync page's list of what sync waits on runs each record's action as the library's storage takes it. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class SyncScreenTest {
    @get:Rule
    val compose = createComposeRule()

    /**
     * An S3 library's Sign In asks for a new access key for its bucket, as Settings does for a device that lost its
     * key, and never runs a provider's account sign-in, which an S3 bucket has none of.
     */
    @Test
    fun anS3SignInAsksForANewAccessKey() {
        val handle =
            object : FakeAppHandle() {
                override suspend fun storageLocation(): BridgeStorageLocation =
                    BridgeStorageLocation.S3(
                        bucket = "music-library",
                        region = "us-east-1",
                        endpoint = null,
                        profile = null,
                    )

                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val status = SyncStatusStore()
        val session = openTestLibrary(handle, status)
        status.apply(
            BridgeSyncStatusSnapshot(
                shown = keyNotAccepted,
                pending = listOf(s3SignIn()),
                setupRunning = false,
                undoneSetup = null,
            ),
            NoErrorDisplays,
            BugReports(),
        )
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SyncScreen(session = session, actions = actions(), onBack = {})
                }
            }
        }

        // The list of what sync waits on sits below the storage section, past the test screen's height.
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Sign in"))
        compose.onNodeWithText("Sign in").performClick()

        compose
            .onNodeWithText("This library is stored in an S3-compatible bucket. Enter the access key you use for it.")
            .assertExists()
        compose.onNodeWithText("This library needs cloud sign-in, which isn't configured on this build.").assertDoesNotExist()
    }

    /** A tap on a pending row opens what is behind its reason; the row's action is its own button. */
    @Test
    fun aTapOnAPendingRowOpensItsDetails() {
        var ran: BridgePendingAction? = null
        val record =
            BridgePendingRecord(
                subject = BridgeMessage(key = "core.pending.subject.connection", args = emptyList()),
                reason =
                    BridgePendingReason(
                        key = "core.pending.reason.storage",
                        detail = "request failed: connection refused",
                        causes = emptyList(),
                    ),
                action = BridgePendingAction.Reconnect,
                actionKey = "core.pending.action.reconnect",
                sendsFiles = false,
            )
        compose.setContent { BaeTheme { PendingSection(records = listOf(record), onAction = { ran = it }) } }

        compose.onNodeWithText("Tap a row to see what happened.").assertExists()
        compose.onNodeWithText("Cloud request failed").performClick()
        compose.onNodeWithText("request failed: connection refused").assertExists()

        assertEquals(null, ran)
    }

    /** A full disk is worded for a phone: "This device", not "this Mac". */
    @Test
    fun aFullDiskIsWordedForThePhone() {
        val handle =
            object : FakeAppHandle() {
                override suspend fun storageLocation(): BridgeStorageLocation = s3Location

                override suspend fun generateRestoreCode() = restoreCodeShare

                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val status = SyncStatusStore()
        val session = openTestLibrary(handle, status)
        status.apply(
            BridgeSyncStatusSnapshot(
                shown =
                    BridgeSyncShown(
                        status = BridgeSyncStatusWord.SYNC_FAILED,
                        statusKey = "core.sync.failed",
                        tone = BridgeSyncTone.DANGER,
                        cause = BridgeSyncCause.DeviceDiskFull,
                        fix = null,
                        lastSynced = null,
                    ),
                pending = emptyList(),
                setupRunning = false,
                undoneSetup = null,
            ),
            NoErrorDisplays,
            BugReports(),
        )
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SyncScreen(session = session, actions = actions(), onBack = {})
                }
            }
        }

        compose.onNodeWithText("This device is out of space. Free some up and bae will try again.").assertExists()
    }

    /**
     * The Sync page holds the library's restore code, masked until shown, where setup's Done lands. The code is made
     * once per visit: scrolling its section away and back shows the same code without making it again.
     */
    @Test
    fun theSyncPageHoldsTheRestoreCodeItMadeOnce() {
        var made = 0
        val handle =
            object : FakeAppHandle() {
                override suspend fun storageLocation(): BridgeStorageLocation = s3Location

                override suspend fun generateRestoreCode(): BridgeRestoreCodeShare {
                    made += 1
                    return restoreCodeShare
                }

                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val session = openTestLibrary(handle, SyncStatusStore())
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SyncScreen(session = session, actions = actions(), onBack = {})
                }
            }
        }

        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Show code"))
        compose.onNodeWithContentDescription("Restore code, partly hidden").assertIsDisplayed()
        compose.onNode(hasScrollAction()).performScrollToIndex(0)
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Show code"))

        compose.onNodeWithContentDescription("Restore code, partly hidden").assertIsDisplayed()
        compose.onNodeWithText("Making your restore code…").assertDoesNotExist()
        assertEquals(1, made)
    }

    private val s3Location =
        BridgeStorageLocation.S3(
            bucket = "music-library",
            region = "us-east-1",
            endpoint = null,
            profile = null,
        )

    private val restoreCodeShare =
        BridgeRestoreCodeShare(code = "CVR1 ABCD EFGH JKMN", qrText = "qr")

    private val keyNotAccepted =
        BridgeSyncShown(
            status = BridgeSyncStatusWord.ACCESS_KEY_NOT_ACCEPTED,
            statusKey = "core.sync.status.access_key_not_accepted",
            tone = BridgeSyncTone.WARNING,
            cause = null,
            fix = BridgeSyncFix.EnterAccessKey,
            lastSynced = null,
        )

    private fun s3SignIn() =
        BridgePendingRecord(
            subject = BridgeMessage(key = "core.pending.subject.connection", args = emptyList()),
            reason = BridgePendingReason(key = "core.pending.reason.provider_pending", detail = null, causes = emptyList()),
            action = BridgePendingAction.SignIn(BridgeCloudProvider.S3),
            actionKey = "core.pending.action.sign_in",
            sendsFiles = false,
        )

    private fun actions() =
        SessionActions(
            oauthProviders = null,
            switchLibrary = {},
            resetDevice = {},
            restoreFromCode = {},
            leaveLibrary = {},
            reloadLibraries = {},
        )
}
