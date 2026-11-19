package fm.bae.app.ui.settings

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.LocalBugReports
import fm.bae.app.NoErrorDisplays
import fm.bae.app.OpenLibrary
import fm.bae.app.data.SyncLine
import fm.bae.app.data.SyncStatusStore
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.SessionActions
import fm.bae.app.ui.TestTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeLibraryClosing
import uniffi.bae_bridge.BridgeMessage
import uniffi.bae_bridge.BridgeMessageArg
import uniffi.bae_bridge.BridgeRestoreCodeShare
import uniffi.bae_bridge.BridgeSyncCause
import uniffi.bae_bridge.BridgeSyncCountdown
import uniffi.bae_bridge.BridgeSyncFix
import uniffi.bae_bridge.BridgeSyncShown
import uniffi.bae_bridge.BridgeSyncStatusSnapshot
import uniffi.bae_bridge.BridgeSyncStatusWord
import uniffi.bae_bridge.BridgeSyncTone
import java.time.Instant

/** The Sync page runs its status's fix as the library's storage takes it. */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35])
class SyncScreenTest {
    @get:Rule
    val compose = createComposeRule()

    /** Signed out of S3 says the changes stay on the device, and Sign in to S3 opens the S3 key dialog. */
    @Test
    fun signInToS3OpensTheKeyDialog() {
        val session = s3Session(keyNotAccepted)
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SyncScreen(session = session, actions = actions(), onBack = {})
                }
            }
        }

        compose.onNodeWithText("Your changes stay on this device.").assertExists()
        compose.onNodeWithText("Sign in to S3").performClick()
        compose.onNodeWithText(KEY_MESSAGE).assertExists()
    }

    /**
     * The Cloud home of an S3 library shows the key the library records for the person, and Replace key opens the S3
     * key dialog, whose Save stays off until both fields hold text; Cancel closes it.
     */
    @Test
    fun replaceKeyOpensTheKeyDialog() {
        val session = s3Session(synced)
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SyncScreen(session = session, actions = actions(), onBack = {})
                }
            }
        }

        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Replace key"))
        compose.onNodeWithText("AKIA4EXAMPLEOLD").assertIsDisplayed()
        compose.onNodeWithText("Replace key").performClick()
        compose.onNodeWithText(KEY_MESSAGE).assertExists()
        compose.onNodeWithText("Save").assertIsNotEnabled()

        compose.onNode(hasSetTextAction() and hasText("Access key ID")).performTextInput("AKIA4EXAMPLENEW")
        compose.onNodeWithText("Save").assertIsNotEnabled()
        compose.onNode(hasSetTextAction() and hasText("Secret access key")).performTextInput("abcdefghijkl")
        compose.onNodeWithText("Save").assertIsEnabled()

        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithText(KEY_MESSAGE).assertDoesNotExist()
    }

    /** A refused key shows the fields again as the person typed them, under why bae could not use the key. */
    @Test
    fun aRefusedKeyKeepsWhatThePersonTyped() {
        var sheet by mutableStateOf<S3KeyReplacement.Sheet>(S3KeyReplacement.Sheet.Fields(null))
        compose.setContent { TestTheme { S3KeyDialog(sheet = sheet, onSave = { _, _ -> }, onCancel = {}) } }
        compose.onNode(hasSetTextAction() and hasText("Access key ID")).performTextInput("AKIA4EXAMPLENEW")

        sheet = S3KeyReplacement.Sheet.Checking
        compose.onNodeWithText("Checking the key…").assertExists()
        sheet = S3KeyReplacement.Sheet.Fields(S3KeyReplacement.Failure.Refused)

        compose.onNodeWithText("Could not use this key").assertExists()
        compose.onNodeWithText("S3 storage does not accept this access key.").assertExists()
        compose.onNodeWithText("AKIA4EXAMPLENEW").assertExists()
    }

    /** A library on S3 storage whose sync shows [shown], over a handle that only makes its restore code. */
    private fun s3Session(shown: BridgeSyncShown): OpenLibrary {
        val handle =
            object : FakeAppHandle() {
                override suspend fun generateRestoreCode() = restoreCodeShare

                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val status = SyncStatusStore()
        val session = openTestLibrary(handle, status)
        status.apply(
            BridgeSyncStatusSnapshot(shown = shown, setupRunning = false, undoneSetup = null),
            NoErrorDisplays,
        )
        return session
    }

    /** A full disk is worded for a phone: "This device", not "this Mac". */
    @Test
    fun aFullDiskIsWordedForThePhone() {
        val handle =
            object : FakeAppHandle() {
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
                setupRunning = false,
                undoneSetup = null,
            ),
            NoErrorDisplays,
        )
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SyncScreen(session = session, actions = actions(), onBack = {})
                }
            }
        }

        compose.onNodeWithText("This device is out of space. Free some up and bae will try again.").assertExists()
    }

    /** The provider's wait says who asked bae to slow down, and the line under it counts down to when sync resumes. */
    @Test
    fun aProvidersWaitCountsDownToWhenSyncResumes() {
        val resumesAtMs = 1_800_000_000_000L
        val waiting =
            BridgeSyncShown(
                status = BridgeSyncStatusWord.WAITING_TO_SYNC,
                statusKey = "core.sync.status.waiting_to_sync",
                tone = BridgeSyncTone.WARNING,
                cause =
                    BridgeSyncCause.Message(
                        message =
                            BridgeMessage(
                                "core.sync.cause.provider_wait",
                                listOf(BridgeMessageArg("provider", "Dropbox")),
                            ),
                        countdown =
                            BridgeSyncCountdown(
                                key = "core.sync.cause.provider_wait.countdown",
                                until = Instant.ofEpochMilli(resumesAtMs),
                            ),
                    ),
                fix = null,
                lastSynced = null,
            )
        compose.setContent {
            TestTheme {
                SyncStatusBlock(line = SyncLine.Read(waiting), onFix = {}, nowMs = { resumesAtMs - 41_500L })
            }
        }

        compose.onNodeWithText("Dropbox asked bae to slow down.").assertExists()
        compose.onNodeWithText("Resuming in 42 seconds.").assertExists()
    }

    /**
     * After another of the person's devices made a new restore code, the status says, before the scan, that the restore
     * discards what this device has not sent, and its Enter restore code button asks the app to close the library for the
     * restore screen.
     */
    @Test
    fun aNewRestoreCodeIsEnteredFromTheStatus() {
        var restores = 0
        val handle =
            object : FakeAppHandle() {
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
                        status = BridgeSyncStatusWord.NEW_RESTORE_CODE,
                        statusKey = "core.sync.status.new_restore_code",
                        tone = BridgeSyncTone.WARNING,
                        cause = BridgeSyncCause.EnterNewRestoreCode,
                        fix = BridgeSyncFix.EnterRestoreCode,
                        lastSynced = null,
                    ),
                setupRunning = false,
                undoneSetup = null,
            ),
            NoErrorDisplays,
        )
        compose.setContent {
            TestTheme {
                CompositionLocalProvider(LocalBugReports provides session.bugs) {
                    SyncScreen(session = session, actions = actions(onRestoreFromCode = { restores += 1 }), onBack = {})
                }
            }
        }

        compose.onNodeWithText("New restore code").assertExists()
        compose
            .onNodeWithText(
                "Enter the new restore code to restore the library on this device. " +
                    "The restore discards any changes this device has not sent.",
            ).assertExists()
        compose.onNodeWithText("Enter restore code…").performClick()

        assertEquals(1, restores)
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
                override suspend fun generateRestoreCode(): BridgeRestoreCodeShare {
                    made += 1
                    return restoreCodeShare
                }

                override fun closeLibrary(closing: BridgeLibraryClosing?) = Unit

                override fun close() = Unit
            }
        val session = openTestLibrary(handle, SyncStatusStore())
        compose.setContent {
            TestTheme {
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

    private val restoreCodeShare =
        BridgeRestoreCodeShare(
            code = "CVR1 ABCD EFGH JKMN",
            maskedGroups = listOf("CVR1", "ABCD", "••••", "JKMN"),
            qrText = "qr",
        )

    private val synced =
        BridgeSyncShown(
            status = BridgeSyncStatusWord.SYNCED,
            statusKey = "core.sync.synced",
            tone = BridgeSyncTone.SUCCESS,
            cause = null,
            fix = null,
            lastSynced = null,
        )

    private val keyNotAccepted =
        BridgeSyncShown(
            status = BridgeSyncStatusWord.SIGNED_OUT_OF_S3,
            statusKey = "core.sync.status.signed_out_s3",
            tone = BridgeSyncTone.WARNING,
            cause = BridgeSyncCause.ChangesStayOnDevice,
            fix = BridgeSyncFix.EnterAccessKey,
            lastSynced = null,
        )

    private fun actions(onRestoreFromCode: () -> Unit = {}) =
        SessionActions(
            oauthProviders = null,
            switchLibrary = {},
            resetDevice = {},
            restoreFromCode = onRestoreFromCode,
            leaveLibrary = {},
            reloadLibraries = {},
        )
}

private const val KEY_MESSAGE = "Enter a key that can use this library’s S3 storage."
