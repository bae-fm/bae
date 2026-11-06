package fm.bae.app.ui.settings

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.LocalBugReports
import fm.bae.app.NoErrorDisplays
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.BaeTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeErrorCategory
import uniffi.bae_bridge.BridgeException
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeStorageLocation
import uniffi.bae_bridge.BridgeSyncStatusSnapshot

@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class SettingsSyncSectionTest {
    @get:Rule
    val compose = createComposeRule()

    /**
     * A device with no storage set up under its recorded location offers Sign In in place of the status line, and a
     * start that failed there, such as on a keychain locked for the session, shows why under it.
     */
    @Test
    fun aFailedStartShowsBesideSignIn() {
        val handle =
            object : FakeAppHandle() {
                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val status = SyncStatusStore()
        val session = openTestLibrary(handle, status)
        status.apply(
            BridgeSyncStatusSnapshot(
                error = BridgeException.Diagnostic(BridgeErrorCategory.KeyringLocked, "keychain is locked"),
                pending = emptyList(),
                indicator = null,
                reconnectOffered = false,
                setupRunning = false,
                undoneSetup = null,
            ),
            NoErrorDisplays,
        )
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SettingsSyncSection(
                        onResetDevice = {},
                        session = session,
                        cloudHome =
                            CloudHome.At(
                                BridgeStorageLocation.S3(
                                    bucket = "music-library",
                                    region = "us-east-1",
                                    endpoint = null,
                                    profile = null,
                                ),
                            ),
                        oauthProviders = null,
                        onPush = {},
                    )
                }
            }
        }

        compose.onNodeWithText("Sign in").assertExists()
        // NoErrorDisplays words every failure "failed", with the error's diagnostic as its details.
        compose.onNodeWithText("failed").assertExists()
        compose.onNodeWithText("Reconnect").assertDoesNotExist()
    }
}
