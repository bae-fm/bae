package fm.bae.app.ui.settings

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.LocalBugReports
import fm.bae.app.NoErrorDisplays
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.BaeTheme
import fm.bae.app.ui.SessionActions
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
import uniffi.bae_bridge.BridgeStorageLocation
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
                shown = notSynced,
                pending = listOf(s3SignIn()),
                setupRunning = false,
                undoneSetup = null,
            ),
            NoErrorDisplays,
        )
        // The state's words come from the bridge's native catalog lookup, which a JVM test can't load; the page says
        // an unreadable status in its own, and keeps the pending list the snapshot read.
        status.fail()
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SyncScreen(session = session, actions = actions(), onBack = {}, onPush = {})
                }
            }
        }

        compose.onNodeWithText("Sign in").performClick()

        compose
            .onNodeWithText("This library is stored in an S3-compatible bucket. Enter the access key you use for it.")
            .assertExists()
        compose.onNodeWithText("This library needs cloud sign-in, which isn't configured on this build.").assertDoesNotExist()
    }

    private val notSynced =
        BridgeSyncShown(
            status = BridgeSyncStatusWord.NOT_SYNCED,
            tone = BridgeSyncTone.NEUTRAL,
            cause = null,
            fix = null,
            lastSynced = null,
        )

    private fun s3SignIn() =
        BridgePendingRecord(
            subject = BridgeMessage(key = "core.pending.subject.connection", args = emptyList()),
            reason = BridgePendingReason(key = "core.pending.reason.provider_pending", detail = null, causes = emptyList()),
            action = BridgePendingAction.SignIn(BridgeCloudProvider.S3),
            actionKey = "core.pending.action.sign_in",
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
