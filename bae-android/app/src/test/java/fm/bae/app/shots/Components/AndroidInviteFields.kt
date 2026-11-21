package fm.bae.app.shots

import androidx.compose.runtime.Composable
import fm.bae.app.ui.settings.AndroidInviteFields
import fm.bae.app.ui.settings.InviteAccessInput
import org.junit.Test
import uniffi.bae_bridge.BridgeStorageLocation

/** The component shots of `AndroidInviteFields`, one for each row of its preview. */
class AndroidInviteFieldsShots : BoardShot() {
    @Test
    fun androidComponentInviteFieldsS3TheirKey() =
        component("android.component.invite-fields.s3-their-key", wide = true) {
            Fields(sceneS3, InviteAccessInput(accessKeyId = "AKIAEXAMPLEKEY", secretAccessKey = "••••••••••••••••"))
        }

    @Test
    fun androidComponentInviteFieldsS3MyKey() =
        component("android.component.invite-fields.s3-my-key", wide = true) {
            Fields(sceneS3, InviteAccessInput().apply { usesOwnKey = true })
        }

    @Test
    fun androidComponentInviteFieldsDropbox() =
        component("android.component.invite-fields.dropbox", wide = true) {
            Fields(sceneDropbox, InviteAccessInput(email = "dan@example.com"))
        }

    @Test
    fun androidComponentInviteFieldsOnedrive() =
        component("android.component.invite-fields.onedrive", wide = true) {
            Fields(
                BridgeStorageLocation.OneDrive("drive", "folder", "name@example.com", null),
                InviteAccessInput(email = "dan@example.com"),
            )
        }
}

@Composable
private fun Fields(
    storage: BridgeStorageLocation,
    input: InviteAccessInput,
) = AndroidInviteFields(storage = storage, input = input, error = null)
