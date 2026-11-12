package fm.bae.app.shots

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import fm.bae.app.ui.components.DialogParts
import fm.bae.app.ui.settings.InviteAccessInput
import fm.bae.app.ui.settings.cancelInviteParts
import fm.bae.app.ui.settings.inviteCodeParts
import fm.bae.app.ui.settings.inviteFormParts
import fm.bae.app.ui.settings.joinRequestParts
import org.junit.Test
import uniffi.bae_bridge.BridgeInvite
import uniffi.bae_bridge.BridgeInviteProgress
import uniffi.bae_bridge.BridgeInviteShown
import uniffi.bae_bridge.BridgeStorageLocation

/** The shots of the ready android.settings.invite.* boards: inviting a person, from the People and devices page. */
class SettingsInviteShots : BoardShot() {
    /** The invite form for a Dropbox library. */
    @Test
    fun androidSettingsInviteDropbox() =
        shot("android.settings.invite.dropbox") { Invite { InviteForm(sceneDropbox, InviteAccessInput(email = "dan@example.com")) } }

    /** The invite form for a OneDrive library. */
    @Test
    fun androidSettingsInviteOnedrive() =
        shot("android.settings.invite.onedrive") {
            Invite {
                InviteForm(
                    BridgeStorageLocation.OneDrive("drive", "folder", "name@example.com", null),
                    InviteAccessInput(email = "dan@example.com"),
                )
            }
        }

    /** The invite form for an S3 library, which asks for the person's access key. */
    @Test
    fun androidSettingsInviteS3() =
        shot("android.settings.invite.s3") {
            Invite {
                InviteForm(
                    BridgeStorageLocation.S3("my-bucket", "us-east-1", "https://s3.home.lan", null),
                    InviteAccessInput(accessKeyId = "AKIAEXAMPLEKEY", secretAccessKey = "••••••••••••••••"),
                )
            }
        }

    /** The invite form for an S3 library, when the invited person uses this device's own key. */
    @Test
    fun androidSettingsInviteS3OwnKey() =
        shot("android.settings.invite.s3-own-key") {
            Invite {
                InviteForm(
                    BridgeStorageLocation.S3("my-bucket", "us-east-1", "https://s3.home.lan", null),
                    InviteAccessInput().apply { usesOwnKey = true },
                )
            }
        }

    /** The invite's code, to scan or copy, while Dan hasn't asked yet. */
    @Test
    fun androidSettingsInviteCode() =
        shot("android.settings.invite.code") {
            Invite { InviteDialog(danInvite(waitingCode)) }
        }

    /** The invite's dialog while this device is offline, so nothing is shared with Dan yet. */
    @Test
    fun androidSettingsInviteNoCodeYet() =
        shot("android.settings.invite.no-code-yet") {
            Invite { InviteDialog(danInvite(BridgeInviteShown.Granting), offline = true) }
        }

    /** The invite's dialog while sync shares the library with Dan. */
    @Test
    fun androidSettingsInviteSharing() =
        shot("android.settings.invite.sharing") {
            Invite { InviteDialog(danInvite(BridgeInviteShown.Granting)) }
        }

    /** Dan's request to join, with the number to compare. */
    @Test
    fun androidSettingsInviteRequest() =
        shot("android.settings.invite.request") {
            Invite(invites = listOf(danInvite(waitingCode))) {
                joinRequestParts(name = "Dan", confirmation = "412 778", onApprove = {}, onDecline = {})
            }
        }

    /** Dan joining: his row on the page. */
    @Test
    fun androidSettingsInviteJoining() =
        shot("android.settings.invite.joining") {
            ScreenScene {
                PeoplePageScene(
                    listOf(ana(), ben()),
                    invites = listOf(danInvite(BridgeInviteShown.Code("0000 0000", BridgeInviteProgress.Joining("Dan")))),
                )
            }
        }

    /** The question before cancelling Dan's invite. */
    @Test
    fun androidSettingsInviteCancel() =
        shot("android.settings.invite.cancel") {
            Invite(
                invites = listOf(danInvite(waitingCode)),
            ) { cancelInviteParts(danInvite(waitingCode), onConfirm = {}, onDismiss = {}) }
        }
}

@Composable
private fun Invite(
    invites: List<BridgeInvite> = emptyList(),
    dialog: @Composable () -> DialogParts,
) = InPlaceDialogScene(behind = { PeoplePageScene(listOf(ana(), ben()), invites = invites) }, dialog = dialog)

@Composable
private fun InviteForm(
    storage: BridgeStorageLocation,
    input: InviteAccessInput,
) = inviteFormParts(storage = storage, error = null, busy = false, onCreate = {}, onDismiss = {}, input = input)

@Composable
private fun InviteDialog(
    invite: BridgeInvite,
    offline: Boolean = false,
) = inviteCodeParts(
    invite = invite,
    providerName = "Dropbox",
    offline = offline,
    error = null,
    busy = false,
    onCancelInvite = {},
    onDismiss = {},
)
