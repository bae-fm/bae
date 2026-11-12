package fm.bae.app.ui.settings

import android.app.Application
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import fm.bae.app.BaeRobolectricTestRunner
import fm.bae.app.BugReports
import fm.bae.app.HANG_GUARD_MS
import fm.bae.app.LocalBugReports
import fm.bae.app.openTestLibrary
import fm.bae.app.playback.FakeAppHandle
import fm.bae.app.ui.BaeTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import uniffi.bae_bridge.BridgeDropboxLocation
import uniffi.bae_bridge.BridgeInviteAccess
import uniffi.bae_bridge.BridgeInviteLifetime
import uniffi.bae_bridge.BridgeMemberRole
import uniffi.bae_bridge.BridgeStorageLocation

/**
 * Creating an invite is one local write: the dialog hands back the new invite's id, and People shows the invite from
 * the list it is in from that write on.
 */
@RunWith(BaeRobolectricTestRunner::class)
@Config(sdk = [35], application = Application::class)
class InviteDialogTest {
    @get:Rule
    val compose = createComposeRule()

    /** What the form asked an invite of. */
    private data class Asked(
        val role: BridgeMemberRole,
        val access: BridgeInviteAccess,
        val lifetime: BridgeInviteLifetime,
    )

    /** A handle whose invite is made at once, as Ben's, with the role, access and lifetime asked for. */
    private class InvitingHandle : FakeAppHandle() {
        val asked = mutableListOf<Asked>()

        override suspend fun invitePerson(
            role: BridgeMemberRole,
            access: BridgeInviteAccess,
            lifetime: BridgeInviteLifetime,
        ): String {
            asked += Asked(role, access, lifetime)
            return "invite-1"
        }
    }

    @Test
    fun creatingAnInviteHandsBackItsId() =
        createInvite(
            choose = {},
            expected =
                Asked(
                    BridgeMemberRole.MEMBER,
                    BridgeInviteAccess.ProviderAccount("ben@example.com"),
                    BridgeInviteLifetime.SEVEN_DAYS,
                ),
        )

    /** The role and the length the person picks are the ones the invite is made with. */
    @Test
    fun theRoleAndLengthPickedAreTheOnesAsked() =
        createInvite(
            choose = {
                compose.onNodeWithText("Admin").performClick()
                compose.onNodeWithText("30 days").performClick()
            },
            expected =
                Asked(
                    BridgeMemberRole.ADMIN,
                    BridgeInviteAccess.ProviderAccount("ben@example.com"),
                    BridgeInviteLifetime.THIRTY_DAYS,
                ),
        )

    private fun createInvite(
        choose: () -> Unit,
        expected: Asked,
    ) {
        val handle = InvitingHandle()
        val session = openTestLibrary(handle)
        val created = mutableListOf<String>()
        compose.setContent {
            BaeTheme {
                CompositionLocalProvider(LocalBugReports provides BugReports()) {
                    InviteDialog(
                        session = session,
                        storage =
                            BridgeStorageLocation.Dropbox(
                                BridgeDropboxLocation.AppFolder,
                                accountLabel = "ana@example.com",
                                folderLabel = null,
                            ),
                        onCreated = { created += it },
                        onDismiss = {},
                    )
                }
            }
        }
        compose.onNodeWithText("Dropbox email").performTextInput("ben@example.com")
        choose()
        compose.onNodeWithText("Create invite").performClick()
        compose.waitUntil(HANG_GUARD_MS) { created.isNotEmpty() }

        assertEquals(listOf("invite-1"), created)
        assertEquals(listOf(expected), handle.asked)
    }
}
